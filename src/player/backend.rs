//! 基于 Rodio 0.22（内置 Symphonia 0.5.5）的播放后端。

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lofty::file::FileType;
use lofty::probe::Probe;
use rodio::decoder::DecoderBuilder;
use rodio::mixer::mixer;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};

use super::spectrum::{InternalSpectrumEvent, PcmBatch, PcmTap, SpectrumWorker};
use super::{PlayState, PlayerEvent};

const EARLY_EOS_TOLERANCE: Duration = Duration::from_secs(1);
const SEEK_END_GUARD: Duration = Duration::from_millis(50);
const POSITION_TRUST_WINDOW: Duration = Duration::from_millis(100);
const HEADLESS_CHANNELS: u16 = 2;
const HEADLESS_SAMPLE_RATE: u32 = 44_100;

enum InternalEvent {
    SourceEnded {
        generation: u64,
        /// 解码 Source 自己耗尽时的已播位置。不能在事件到达后再读
        /// `Player::position()`：队列此时已切到后续静音源，`get_pos()` 常为 0。
        position: Duration,
    },
    DeviceError(String),
}

enum Output {
    /// 必须与 `backend` 一起存活，否则 CPAL 输出流会被关闭。
    Device(#[allow(dead_code)] MixerDeviceSink),
    Headless {
        shutdown: Arc<AtomicBool>,
        pump: Option<JoinHandle<()>>,
    },
}

pub struct Player {
    _output: Output,
    backend: rodio::Player,
    events: Receiver<InternalEvent>,
    event_tx: Sender<InternalEvent>,
    spectrum_events: Receiver<InternalSpectrumEvent>,
    state: PlayState,
    current_path: Option<PathBuf>,
    current_duration: Option<Duration>,
    generation: Arc<AtomicU64>,
    logical_volume: AtomicU8,
    muted: AtomicBool,
    spectrum_enabled: Arc<AtomicBool>,
    spectrum_live: Arc<AtomicBool>,
    is_playing: AtomicBool,
    pcm_slot: Arc<Mutex<Option<PcmBatch>>>,
    force_position: Mutex<Option<Duration>>,
    _spectrum: SpectrumWorker,
}

impl Player {
    pub fn new() -> Result<Self, String> {
        let (event_tx, events) = mpsc::channel();
        let error_tx = event_tx.clone();
        let mut sink = DeviceSinkBuilder::from_default_device()
            .map_err(|error| format!("无法打开默认音频输出设备: {error}"))?
            .with_error_callback(move |error| {
                let _ = error_tx.send(InternalEvent::DeviceError(error.to_string()));
            })
            .open_stream()
            .map_err(|error| format!("无法打开音频输出流: {error}"))?;
        sink.log_on_drop(false);
        let backend = rodio::Player::connect_new(sink.mixer());
        Ok(Self::assemble(
            Output::Device(sink),
            backend,
            event_tx,
            events,
        ))
    }

    /// 创建不依赖系统音频设备的播放器，供自动化测试使用。
    #[doc(hidden)]
    pub fn new_for_tests() -> Result<Self, String> {
        let (event_tx, events) = mpsc::channel();
        let (mix, mut output) = mixer(
            rodio::ChannelCount::new(HEADLESS_CHANNELS)
                .expect("headless channel count is non-zero"),
            rodio::SampleRate::new(HEADLESS_SAMPLE_RATE).expect("headless sample rate is non-zero"),
        );
        let backend = rodio::Player::connect_new(&mix);
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let pump = thread::Builder::new()
            .name("rodio-test-pump".to_owned())
            .spawn(move || {
                let samples_per_ms =
                    u64::from(HEADLESS_SAMPLE_RATE) * u64::from(HEADLESS_CHANNELS) / 1_000;
                let mut consumed = 0_u64;
                while !thread_shutdown.load(Ordering::Relaxed) {
                    if output.next().is_none() {
                        break;
                    }
                    consumed += 1;
                    if consumed >= samples_per_ms {
                        consumed = 0;
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            })
            .map_err(|error| format!("无法启动测试音频泵: {error}"))?;
        Ok(Self::assemble(
            Output::Headless {
                shutdown,
                pump: Some(pump),
            },
            backend,
            event_tx,
            events,
        ))
    }

    fn assemble(
        output: Output,
        backend: rodio::Player,
        event_tx: Sender<InternalEvent>,
        events: Receiver<InternalEvent>,
    ) -> Self {
        let generation = Arc::new(AtomicU64::new(0));
        let spectrum_enabled = Arc::new(AtomicBool::new(true));
        let spectrum_live = Arc::new(AtomicBool::new(false));
        let pcm_slot = Arc::new(Mutex::new(None));
        let (spectrum_tx, spectrum_events) = mpsc::channel();
        let spectrum =
            SpectrumWorker::spawn(spectrum_tx, Arc::clone(&generation), Arc::clone(&pcm_slot));

        Self {
            _output: output,
            backend,
            events,
            event_tx,
            spectrum_events,
            state: PlayState::Stopped,
            current_path: None,
            current_duration: None,
            generation,
            logical_volume: AtomicU8::new(100),
            muted: AtomicBool::new(false),
            spectrum_enabled,
            spectrum_live,
            is_playing: AtomicBool::new(false),
            pcm_slot,
            force_position: Mutex::new(None),
            _spectrum: spectrum,
        }
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        self.load(path, PlayState::Playing)
    }

    /// 解码并装上曲目，保持暂停。Rodio 的 `append` 会解除 `stopped`，
    /// 必须在 append 之前把 Player 置为 paused，否则会直接出声。
    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        self.load(path, PlayState::Paused)
    }

    fn load(&mut self, path: &Path, target: PlayState) -> Result<(), String> {
        if !matches!(target, PlayState::Playing | PlayState::Paused) {
            return Err("内部错误: load 只能进入 Playing 或 Paused".to_owned());
        }
        if !path.is_file() {
            return Err(format!("音频文件不存在: {}", path.display()));
        }
        let absolute = path
            .canonicalize()
            .map_err(|error| format!("无法解析音频路径 {}: {error}", path.display()))?;
        reject_unsupported(&absolute)?;

        let file = File::open(&absolute)
            .map_err(|error| format!("无法打开 {}: {error}", absolute.display()))?;
        let byte_len = file
            .metadata()
            .map_err(|error| format!("无法读取 {} 的大小: {error}", absolute.display()))?
            .len();
        if byte_len == 0 {
            return Err(format!("空文件: {}", absolute.display()));
        }

        let mut builder = DecoderBuilder::new()
            .with_data(file)
            .with_byte_len(byte_len)
            .with_seekable(true);
        if let Some(extension) = absolute
            .extension()
            .and_then(|extension| extension.to_str())
        {
            builder = builder.with_hint(extension);
        }
        let decoder = builder
            .build()
            .map_err(|error| format!("无法识别或解码 {}: {error}", absolute.display()))?;
        let duration = decoder.total_duration();

        if target == PlayState::Paused {
            self.backend.pause();
        }
        let generation = self.advance_generation();
        self.backend.stop();
        if target == PlayState::Paused {
            self.backend.pause();
        }

        let tap = PcmTap::new(
            decoder,
            generation,
            Arc::clone(&self.pcm_slot),
            Arc::clone(&self.spectrum_live),
        );
        self.backend.append(EndSignal {
            inner: tap,
            generation,
            events: self.event_tx.clone(),
            samples: 0,
            signaled: false,
        });

        self.apply_volume();
        if target == PlayState::Playing {
            self.backend.play();
        } else {
            self.backend.pause();
        }
        self.state = target;
        self.current_path = Some(absolute);
        self.current_duration = duration;
        self.set_forced_position(Duration::ZERO);
        self.is_playing
            .store(target == PlayState::Playing, Ordering::Relaxed);
        self.refresh_spectrum_live();
        Ok(())
    }

    pub fn pause(&mut self) {
        if self.state == PlayState::Playing {
            self.backend.pause();
            self.state = PlayState::Paused;
            self.is_playing.store(false, Ordering::Relaxed);
            self.refresh_spectrum_live();
        }
    }

    pub fn resume(&mut self) {
        if self.state == PlayState::Paused {
            self.backend.play();
            self.state = PlayState::Playing;
            self.is_playing.store(true, Ordering::Relaxed);
            self.refresh_spectrum_live();
        }
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            PlayState::Playing => self.pause(),
            PlayState::Paused => self.resume(),
            PlayState::Stopped => {}
        }
    }

    pub fn stop(&mut self) {
        let _ = self.advance_generation();
        self.backend.stop();
        self.state = PlayState::Stopped;
        self.current_path = None;
        self.current_duration = None;
        self.set_forced_position(Duration::ZERO);
        self.is_playing.store(false, Ordering::Relaxed);
        self.refresh_spectrum_live();
    }

    pub fn position(&self) -> Duration {
        let backend = self.backend.get_pos();
        let mut forced = self
            .force_position
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(fallback) = *forced {
            let delta = backend.abs_diff(fallback);
            if delta <= POSITION_TRUST_WINDOW {
                *forced = None;
                return backend;
            }
            return fallback;
        }
        backend
    }

    pub fn duration(&self) -> Option<Duration> {
        self.current_duration
    }

    pub fn seek_to(&self, pos: Duration) -> bool {
        if self.state == PlayState::Stopped {
            return false;
        }
        let target = clamp_seek_target(pos, self.current_duration);
        if self.backend.try_seek(target).is_ok() {
            self.set_forced_position(target);
            true
        } else {
            false
        }
    }

    pub fn seek_relative(&self, offset_seconds: i64) {
        let _ = self.seek_relative_micros(offset_seconds.saturating_mul(1_000_000));
    }

    /// 相对当前进度偏移。正值前进，负值后退。成功 seek 返回 `Some(新位置)`。
    /// 调用方负责把越过曲尾的情况当成 Next；本方法只 saturate 到 0 并 clamp 末端。
    pub fn seek_relative_micros(&self, offset_micros: i64) -> Option<Duration> {
        if self.state == PlayState::Stopped {
            return None;
        }
        let current = self.position();
        let requested = if offset_micros.is_negative() {
            current.saturating_sub(Duration::from_micros(offset_micros.unsigned_abs()))
        } else {
            current.saturating_add(Duration::from_micros(offset_micros as u64))
        };
        self.seek_to(requested).then_some(self.position())
    }

    pub fn set_volume(&self, percent: u8) {
        self.logical_volume
            .store(percent.min(100), Ordering::Relaxed);
        self.apply_volume();
    }

    pub fn volume(&self) -> u8 {
        self.logical_volume.load(Ordering::Relaxed)
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        self.apply_volume();
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn set_spectrum_enabled(&self, enabled: bool) {
        self.spectrum_enabled.store(enabled, Ordering::Relaxed);
        self.refresh_spectrum_live();
    }

    pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
        let current = self.generation.load(Ordering::Relaxed);
        let mut drained = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            match event {
                InternalEvent::SourceEnded {
                    generation,
                    position,
                } => {
                    if generation != current {
                        continue;
                    }
                    let kind = classify_source_end(position, self.current_duration);
                    self.mark_stopped();
                    drained.push(match kind {
                        SourceEndKind::EndOfStream => PlayerEvent::EndOfStream,
                        SourceEndKind::Error => {
                            PlayerEvent::Error("音频可能损坏或解码提前终止".to_owned())
                        }
                    });
                }
                InternalEvent::DeviceError(message) => {
                    self.mark_stopped();
                    drained.push(PlayerEvent::Error(message));
                }
            }
        }
        while let Ok(frame) = self.spectrum_events.try_recv() {
            if frame.generation != current || !self.spectrum_enabled.load(Ordering::Relaxed) {
                continue;
            }
            drained.push(frame.into_player_event());
        }
        drained
    }

    fn advance_generation(&self) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.clear_pcm();
        generation
    }

    fn mark_stopped(&mut self) {
        self.state = PlayState::Stopped;
        self.is_playing.store(false, Ordering::Relaxed);
        self.spectrum_live.store(false, Ordering::Relaxed);
    }

    fn apply_volume(&self) {
        let volume = if self.muted.load(Ordering::Relaxed) {
            0.0
        } else {
            f32::from(self.logical_volume.load(Ordering::Relaxed)) / 100.0
        };
        self.backend.set_volume(volume);
    }

    fn refresh_spectrum_live(&self) {
        let live = self.spectrum_enabled.load(Ordering::Relaxed)
            && self.is_playing.load(Ordering::Relaxed);
        self.spectrum_live.store(live, Ordering::Relaxed);
        if !live {
            self.clear_pcm();
        }
    }

    fn clear_pcm(&self) {
        if let Ok(mut slot) = self.pcm_slot.lock() {
            *slot = None;
        }
    }

    fn set_forced_position(&self, position: Duration) {
        *self
            .force_position
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(position);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.advance_generation();
        self.is_playing.store(false, Ordering::Relaxed);
        self.spectrum_live.store(false, Ordering::Relaxed);
        self.backend.stop();
        if let Output::Headless { shutdown, pump } = &mut self._output {
            shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = pump.take() {
                let _ = handle.join();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceEndKind {
    EndOfStream,
    Error,
}

/// 解码 Source 耗尽时发出结束事件，并带上按采样计数的播放位置。
struct EndSignal<S> {
    inner: S,
    generation: u64,
    events: Sender<InternalEvent>,
    samples: u64,
    signaled: bool,
}

impl<S: Source<Item = f32>> EndSignal<S> {
    fn elapsed(&self) -> Duration {
        let channels = u64::from(self.inner.channels().get());
        let rate = u64::from(self.inner.sample_rate().get());
        if channels == 0 || rate == 0 {
            return Duration::ZERO;
        }
        let frames = self.samples / channels;
        Duration::from_nanos(frames.saturating_mul(1_000_000_000) / rate)
    }
}

impl<S: Source<Item = f32>> Iterator for EndSignal<S> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            Some(sample) => {
                self.samples += 1;
                Some(sample)
            }
            None => {
                if !self.signaled {
                    self.signaled = true;
                    let _ = self.events.send(InternalEvent::SourceEnded {
                        generation: self.generation,
                        position: self.elapsed(),
                    });
                }
                None
            }
        }
    }
}

impl<S: Source<Item = f32>> Source for EndSignal<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)?;
        let channels = u64::from(self.inner.channels().get());
        let rate = u64::from(self.inner.sample_rate().get());
        self.samples = duration_to_samples(pos, channels, rate);
        Ok(())
    }
}

fn duration_to_samples(position: Duration, channels: u64, rate: u64) -> u64 {
    if channels == 0 || rate == 0 {
        return 0;
    }
    position
        .as_nanos()
        .saturating_mul(rate as u128)
        .saturating_mul(channels as u128)
        .checked_div(1_000_000_000)
        .unwrap_or(0) as u64
}

fn classify_source_end(position: Duration, duration: Option<Duration>) -> SourceEndKind {
    match duration {
        Some(duration) if position + EARLY_EOS_TOLERANCE < duration => SourceEndKind::Error,
        _ => SourceEndKind::EndOfStream,
    }
}

fn clamp_seek_target(target: Duration, duration: Option<Duration>) -> Duration {
    match duration {
        Some(duration) if duration <= SEEK_END_GUARD => Duration::ZERO,
        Some(duration) => target.min(duration.saturating_sub(SEEK_END_GUARD)),
        None => target,
    }
}

fn reject_unsupported(path: &Path) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("ape") => {
            return Err(format!("不支持 APE 格式: {}", path.display()));
        }
        Some("wma") => {
            return Err(format!("不支持 WMA 格式: {}", path.display()));
        }
        Some("opus") => {
            return Err(format!("暂不支持 Opus: {}", path.display()));
        }
        _ => {}
    }

    if let Ok(probe) = Probe::open(path)
        && let Ok(probe) = probe.guess_file_type()
    {
        match probe.file_type() {
            Some(FileType::Ape) => {
                return Err(format!("不支持 APE 格式: {}", path.display()));
            }
            Some(FileType::Opus) => {
                return Err(format!("暂不支持 Opus: {}", path.display()));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_eos_is_classified_as_error() {
        assert_eq!(
            classify_source_end(Duration::from_secs(5), Some(Duration::from_secs(10))),
            SourceEndKind::Error
        );
        assert_eq!(
            classify_source_end(Duration::from_secs(9), Some(Duration::from_secs(10))),
            SourceEndKind::EndOfStream
        );
        assert_eq!(
            classify_source_end(Duration::from_secs(3), None),
            SourceEndKind::EndOfStream
        );
    }

    #[test]
    fn seek_never_lands_on_duration() {
        assert_eq!(
            clamp_seek_target(Duration::from_secs(10), Some(Duration::from_secs(10))),
            Duration::from_secs(10) - SEEK_END_GUARD
        );
        assert_eq!(
            clamp_seek_target(Duration::from_millis(10), Some(Duration::from_millis(40))),
            Duration::ZERO
        );
        assert_eq!(
            clamp_seek_target(Duration::from_secs(2), None),
            Duration::from_secs(2)
        );
    }
}
