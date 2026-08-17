//! CAVA-inspired 频谱处理器（v2）。
//!
//! 输入是播放后端的逐帧 dB 幅值（`magnitude`，阈值 clamp 到
//! `SPECTRUM_THRESHOLD_DB`）；输出是每根可视 bar 的 0.0..=1.0 高度。
//! 管线（全部 linear amplitude 域）：
//!
//! 1. 映射：32 个对数视觉频段 → 非重叠、每段至少一个 FFT bin 的单调量化边界
//!    （CAVA 去堆叠法；bin 宽用精确公式 rate / (2·bands − 2)）。
//! 2. 聚合：段内 `10^(dB/20)` 线性幅度求算术平均。
//! 3. EQ：按段实际中心频率的温和高频提升（抵消音乐谱自然滚降）。
//! 4. Autosensitivity：CAVA 式全局灵敏度——撞顶快降、非静音慢升、
//!    启动与切歌后进入 fast-adapt 快速收敛。
//! 5. 时域：温和 attack + gravity 二次加速下落。v1  deliberately 不含
//!    integral smoothing；实测抖动再追加。
//!
//! 常数均为"每源帧"步长；源 interval 固定 20ms（50Hz），因此不需要 CAVA 的
//! 运行时帧率修正。

use crate::player::SPECTRUM_THRESHOLD_DB;

pub const VISUALIZER_BARS: usize = 32;
pub const VISUALIZER_MIN_HZ: f32 = 50.0;
pub const VISUALIZER_MAX_HZ: f32 = 5_000.0;

/// 高频提升强度（dB/oct，按段实际中心频率）。温和档；过强可调低。
const EQ_DB_PER_OCTAVE: f32 = 1.5;
/// EQ 参考频率：此频率处增益为 1（0 dB）。
const EQ_REFERENCE_HZ: f32 = 100.0;

// Autosensitivity（每帧步长，50Hz 源）。
const SENS_OVERSHOOT_DOWN: f32 = 0.02; // 任一 bar 撞顶：乘 0.98，约 0.7s 减半
const SENS_SLOW_UP: f32 = 0.001; // 非静音：乘 1.001，约 5%/s
const SENS_FAST_UP: f32 = 0.10; // fast-adapt：乘 1.10，快速爬升
const SENS_MIN: f32 = 0.25; // 防撞底；正常内容下不会触及
const SENS_MAX: f32 = 200.0; // 防长时间近静音后失控；0.001 幅度也只到 0.2

// 时域（每帧）。
const ATTACK: f32 = 0.6; // 上升：output += ATTACK × (target − output)
const GRAVITY: f32 = 4.3; // 下落：output = peak × (1 − fall² × GRAVITY)
const FALL_STEP: f32 = 0.03; // 每帧 fall 增量；峰值约 0.32s 加速落尽

/// 把逐帧 dB 幅值处理成可视 bar 高度。
pub struct SpectrumProcessor {
    /// 每根 bar 的 bin 区间 [lower, upper)，单调递增且互不重叠。
    lower: Vec<usize>,
    upper: Vec<usize>,
    /// 每根 bar 的线性域 EQ 增益（按实际中心频率）。
    eq: Vec<f32>,
    /// 全局灵敏度。
    sens: f32,
    /// 启动/切歌后的快速收敛阶段；首次撞顶后退出。
    fast_adapt: bool,
    /// gravity：下落起点峰值。
    peak: Vec<f32>,
    /// gravity：下落计时（每帧 +FALL_STEP）。
    fall: Vec<f32>,
    /// 输出（0.0..=1.0）。
    output: Vec<f32>,
    sample_rate: u32,
    bands: usize,
}

impl Default for SpectrumProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumProcessor {
    pub fn new() -> Self {
        Self {
            lower: vec![0; VISUALIZER_BARS],
            upper: vec![0; VISUALIZER_BARS],
            eq: vec![1.0; VISUALIZER_BARS],
            sens: 1.0,
            fast_adapt: true,
            peak: vec![0.0; VISUALIZER_BARS],
            fall: vec![0.0; VISUALIZER_BARS],
            output: vec![0.0; VISUALIZER_BARS],
            sample_rate: 0,
            bands: 0,
        }
    }

    /// 源参数（采样率 / bands）变化时重建映射与 EQ 表；开销可忽略。
    pub fn set_source(&mut self, sample_rate: u32, bands: usize) {
        if sample_rate == self.sample_rate && bands == self.bands {
            return;
        }
        self.sample_rate = sample_rate;
        self.bands = bands;
        self.rebuild_tables();
    }

    /// 切歌：保留已收敛的 sensitivity，重新进入 fast-adapt 阶段。
    pub fn on_track_change(&mut self) {
        self.fast_adapt = true;
    }

    /// 清空可视输出（gravity 状态一并清零）；sensitivity 保留。
    pub fn reset_output(&mut self) {
        self.output.fill(0.0);
        self.peak.fill(0.0);
        self.fall.fill(0.0);
    }

    pub fn bars(&self) -> &[f32] {
        &self.output
    }

    #[cfg(test)]
    pub fn sensitivity(&self) -> f32 {
        self.sens
    }

    /// 处理一帧 dB 幅值。一次 drain 中的多帧必须逐帧传入，
    /// 否则瞬态峰值会被 last-wins 丢弃。
    pub fn process_frame(&mut self, frame: &[f32], sample_rate: u32) {
        if frame.is_empty() {
            return;
        }
        self.set_source(sample_rate, frame.len());

        let silence = frame
            .iter()
            .all(|db| !db.is_finite() || *db <= SPECTRUM_THRESHOLD_DB + 0.01);
        let mut overshoot = false;
        for bar in 0..VISUALIZER_BARS {
            let target = self.band_mean_linear(frame, bar) * self.eq[bar] * self.sens;
            self.advance(bar, target);
            if self.output[bar] > 1.0 {
                overshoot = true;
                self.output[bar] = 1.0;
                // gravity 下落起点必须是显示高度（已 clamp），否则未 clamp 的 peak
                // 会让后续帧持续假性撞顶，sens 被反复快降。
                self.peak[bar] = self.peak[bar].min(1.0);
            }
        }

        if overshoot {
            self.sens *= 1.0 - SENS_OVERSHOOT_DOWN;
            self.fast_adapt = false;
        } else if !silence {
            let step = if self.fast_adapt {
                SENS_FAST_UP
            } else {
                SENS_SLOW_UP
            };
            self.sens *= 1.0 + step;
        }
        self.sens = self.sens.clamp(SENS_MIN, SENS_MAX);
    }

    /// 暂停/停止期间让输出按 gravity 落向 0；不触碰 sensitivity。
    pub fn fade_step(&mut self) {
        for bar in 0..VISUALIZER_BARS {
            self.advance(bar, 0.0);
        }
    }

    /// 温和 attack（上升）+ gravity（下落，不低于当前 target，避免持续音被拖穿）。
    fn advance(&mut self, bar: usize, target: f32) {
        if target >= self.output[bar] {
            self.output[bar] += (target - self.output[bar]) * ATTACK;
            self.peak[bar] = self.output[bar];
            self.fall[bar] = 0.0;
            return;
        }
        let curve = (self.peak[bar] * (1.0 - self.fall[bar] * self.fall[bar] * GRAVITY)).max(0.0);
        if curve <= target {
            self.output[bar] = target;
            self.peak[bar] = target;
            self.fall[bar] = 0.0;
        } else {
            self.output[bar] = curve;
            self.fall[bar] += FALL_STEP;
        }
    }

    /// 段内 bin 的线性幅度均值；空段或非有限值按 0 计。
    fn band_mean_linear(&self, frame: &[f32], bar: usize) -> f32 {
        let (lower, upper) = (self.lower[bar], self.upper[bar]);
        if lower >= upper || upper > frame.len() {
            return 0.0;
        }
        let (sum, count) = frame[lower..upper]
            .iter()
            .fold((0.0f32, 0usize), |acc, db| {
                if db.is_finite() {
                    (acc.0 + 10f32.powf(db / 20.0), acc.1 + 1)
                } else {
                    acc
                }
            });
        if count == 0 { 0.0 } else { sum / count as f32 }
    }

    /// C 式单调量化边界（CAVA 去堆叠）：log 频段边缘 ceil 到 bin，边界严格递增，
    /// 保证每根 bar 至少独占一个 bin 且互不重叠；顶部越界 clamp 到 bands（空段）。
    fn rebuild_tables(&mut self) {
        if self.bands < 2 || self.sample_rate == 0 {
            self.lower.fill(0);
            self.upper.fill(0);
            self.eq.fill(1.0);
            return;
        }
        // GStreamer spectrum 公式：Δf = rate / (2·bands − 2)。当前 Rodio 输入是
        // FFT-1024 的 512 个线性 bin，真实 Δf = rate / 1024；相对误差约 0.2%。
        let bin_width = self.sample_rate as f32 / (2 * self.bands - 2) as f32;
        let ratio = VISUALIZER_MAX_HZ / VISUALIZER_MIN_HZ;
        let mut bound = ((VISUALIZER_MIN_HZ / bin_width).ceil() as usize).min(self.bands);
        for bar in 0..VISUALIZER_BARS {
            let edge_hz = VISUALIZER_MIN_HZ * ratio.powf((bar + 1) as f32 / VISUALIZER_BARS as f32);
            let next = ((edge_hz / bin_width).ceil() as usize)
                .max(bound + 1)
                .min(self.bands);
            self.lower[bar] = bound;
            self.upper[bar] = next;
            let center_hz = if next > bound {
                ((bound as f32 * bin_width) * ((next - 1) as f32 * bin_width)).sqrt()
            } else {
                VISUALIZER_MIN_HZ
            };
            self.eq[bar] =
                10f32.powf(EQ_DB_PER_OCTAVE * (center_hz / EQ_REFERENCE_HZ).log2() / 20.0);
            bound = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [u32; 3] = [44_100, 48_000, 96_000];
    // 512 是当前 baseline；513/1025/2049 使 nfft=2·bands−2 为 2 的幂（后续 A/B）。
    const BANDS: [usize; 4] = [512, 513, 1025, 2049];

    #[test]
    fn mapping_is_monotonic_non_overlapping_and_non_empty_for_music_rates() {
        for rate in RATES {
            for bands in BANDS {
                let mut processor = SpectrumProcessor::new();
                processor.set_source(rate, bands);
                for bar in 0..VISUALIZER_BARS {
                    assert!(
                        processor.lower[bar] < processor.upper[bar],
                        "rate={rate} bands={bands} bar={bar} 为空段"
                    );
                    assert!(processor.upper[bar] <= bands);
                    if bar > 0 {
                        assert!(
                            processor.lower[bar] >= processor.upper[bar - 1],
                            "rate={rate} bands={bands} bar={bar} 与上一根重叠"
                        );
                    }
                }
                // 低频端跳过 DC bin。
                assert!(processor.lower[0] >= 1);
            }
        }
    }

    #[test]
    fn bin_width_uses_exact_gstreamer_formula() {
        let mut processor = SpectrumProcessor::new();
        processor.set_source(44_100, 512);
        // 精确 bin 宽 = 44100 / 1022 ≈ 43.15Hz；bar 0（50Hz 起）应落在 bin 2。
        assert_eq!(processor.lower[0], 2);
    }

    #[test]
    fn aggregation_means_linear_amplitude_not_db() {
        let mut processor = SpectrumProcessor::new();
        processor.set_source(48_000, 512);
        // 直接指定一个两 bin 段：-20dB(0.1) 与 -40dB(0.01) 的线性均值 = 0.055。
        processor.lower[0] = 2;
        processor.upper[0] = 4;
        let mut frame = vec![SPECTRUM_THRESHOLD_DB; 512];
        frame[2] = -20.0;
        frame[3] = -40.0;
        let mean = processor.band_mean_linear(&frame, 0);
        assert!((mean - 0.055).abs() < 1e-4, "mean={mean}");
    }

    #[test]
    fn eq_boosts_higher_bars_by_actual_center_frequency() {
        let mut processor = SpectrumProcessor::new();
        processor.set_source(48_000, 512);
        // 单调递增；100Hz 附近的 bar 增益 ≈ 1；最高 bar 约 +9~10dB（×3 左右）。
        assert!(processor.eq.windows(2).all(|pair| pair[1] >= pair[0]));
        let near_100hz = processor.eq[2]; // bar 2 中心 ≈ 141Hz @48k/512
        assert!((0.9..=1.3).contains(&near_100hz), "eq={near_100hz}");
        let top = processor.eq[VISUALIZER_BARS - 1];
        assert!((2.0..=4.0).contains(&top), "eq={top}");
    }

    #[test]
    fn attack_softens_rise_and_overshoot_drops_sensitivity() {
        let mut processor = SpectrumProcessor::new();
        processor.set_source(48_000, 512);
        processor.process_frame(&vec![0.0; 512], 48_000);
        // 满幅帧：所有 bar target ≥ sens=1.0，attack 后 output=0.6×target，
        // 高频段 EQ>1 必然撞顶 → sens 快降且退出 fast-adapt。
        assert!(processor.bars().iter().all(|value| *value > 0.5));
        assert!(processor.sensitivity() < 1.0);
        let sens_after = processor.sensitivity();
        processor.on_track_change();
        assert_eq!(processor.sensitivity(), sens_after);
    }

    #[test]
    fn quiet_frames_raise_sensitivity_but_silence_does_not() {
        let mut processor = SpectrumProcessor::new();
        processor.set_source(48_000, 512);
        // 先用满幅帧撞顶退出 fast-adapt。
        processor.process_frame(&vec![0.0; 512], 48_000);
        let sens0 = processor.sensitivity();

        // 非静音的安静帧：慢升。
        for _ in 0..10 {
            processor.process_frame(&vec![-60.0; 512], 48_000);
        }
        let expected = sens0 * 1.001f32.powi(10);
        assert!(
            (processor.sensitivity() - expected).abs() < 1e-4,
            "sens={} expected={} sens0={}",
            processor.sensitivity(),
            expected,
            sens0
        );

        // 静音帧（全部 threshold）：sens 不变。
        let sens1 = processor.sensitivity();
        for _ in 0..10 {
            processor.process_frame(&vec![SPECTRUM_THRESHOLD_DB; 512], 48_000);
        }
        assert_eq!(processor.sensitivity(), sens1);
    }

    #[test]
    fn gravity_fall_accelerates_and_sustained_target_is_not_undershot() {
        let mut processor = SpectrumProcessor::new();
        processor.set_source(48_000, 512);
        let top = VISUALIZER_BARS - 1;
        // 满谱驱动至稳态：autosens 收敛后最强（最高频）bar 接近满幅。
        for _ in 0..12 {
            processor.process_frame(&vec![-6.0; 512], 48_000);
        }
        assert!(processor.bars()[top] > 0.9, "top={}", processor.bars()[top]);

        // 持续中等强度（-26dB ≈ 0.05 线性）：输出不得跌穿当前 target。
        for _ in 0..60 {
            processor.process_frame(&vec![-26.0; 512], 48_000);
        }
        let steady =
            (10f32.powf(-26.0 / 20.0) * processor.eq[top] * processor.sensitivity()).min(1.0);
        let settled = processor.bars()[top];
        assert!(
            settled >= steady * 0.95,
            "settled={settled} steady={steady}"
        );

        // 随后静音：gravity 下落加速，约 0.5s（25 帧）内落尽。
        processor.process_frame(&vec![-6.0; 512], 48_000);
        let mut drops = Vec::new();
        let mut previous = processor.bars()[top];
        for _ in 0..25 {
            processor.process_frame(&vec![SPECTRUM_THRESHOLD_DB; 512], 48_000);
            drops.push(previous - processor.bars()[top]);
            previous = processor.bars()[top];
        }
        assert!(processor.bars()[top] < 0.01);
        // 加速下落：后段单帧落差应大于前段。
        assert!(drops[10] > drops[1], "drops={drops:?}");
    }

    #[test]
    fn fade_step_falls_to_zero_without_touching_sensitivity() {
        let mut processor = SpectrumProcessor::new();
        processor.set_source(48_000, 512);
        processor.process_frame(&vec![-6.0; 512], 48_000);
        assert!(processor.bars().iter().any(|value| *value > 0.3));
        let sens = processor.sensitivity();
        for _ in 0..40 {
            processor.fade_step();
        }
        assert!(processor.bars().iter().all(|value| *value < 0.01));
        assert_eq!(processor.sensitivity(), sens);
    }

    #[test]
    fn reset_output_clears_bars_but_keeps_sensitivity() {
        let mut processor = SpectrumProcessor::new();
        processor.process_frame(&vec![0.0; 512], 48_000);
        let sens = processor.sensitivity();
        processor.reset_output();
        assert!(processor.bars().iter().all(|value| *value == 0.0));
        assert_eq!(processor.sensitivity(), sens);
    }

    #[test]
    fn non_finite_bins_are_ignored_in_mean() {
        let mut processor = SpectrumProcessor::new();
        processor.set_source(48_000, 512);
        processor.lower[0] = 2;
        processor.upper[0] = 4;
        let mut frame = vec![f32::NAN; 512];
        frame[2] = -20.0;
        // bin 3 为 NaN 被忽略；均值等于 -20dB 的线性值。
        let mean = processor.band_mean_linear(&frame, 0);
        assert!((mean - 0.1).abs() < 1e-4, "mean={mean}");
        // 全 NaN 段 -> 0。
        assert_eq!(processor.band_mean_linear(&[f32::NAN; 512], 0), 0.0);
    }
}
