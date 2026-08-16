# 频谱可行性报告：GStreamer spectrum 能否支撑 CAVA 式效果

状态：研究完成，待评审
日期：2026-08-16
基线：`feature/v1.0.3-ui-improvements` @ `eb3efe3`（PR #1 head）
性质：纯研究。未修改 src/、Cargo、配置、CI；实验脚本在 `Temp/spectrum-research/`
（未跟踪）；本报告不提交 commit。
参考实现：工作区 `cava/`（用户提供的 CAVA 当前源码，cavacore.c / cava.c）。

## 0. 结论先行

**最终结论：B —— 继续用 GStreamer，但必须重做 mapping；且比"提高 bands"更
关键的是缩短 interval。** 具体而言：

1. 四大症状的首要机制都**不是**频率分辨率本身：
   - "低频多柱同步" = 映射策略 A（floor..ceil）让多根 bar 读同一个 source bin；
   - "低频长期顶高" = 共享 bin + max-dB 聚合 + 无自动灵敏度；
   - "中高频活动不足" 与 "下降缺乏打击感" 的主要来源是
     **interval(50ms) > FFT 窗(23ms) 导致的跨窗 dB 算术平均**，把 20ms 瞬态
     压低 29dB（实测 −43dB vs 持续音 −14dB）。把 interval 缩到 ≤ 一个 FFT 窗
     （20ms）后，同一 20ms 脉冲恢复到 **−14.1dB**（与持续音相同）——瞬态信息
     在 GStreamer 数据源里**存在**，目前是在 message 层被平均掉的。
2. bands 512 的频率分辨率（44.1kHz 下 43.2Hz/bin）在 **C 式单调量化映射**下
   足够：32 根 bar 全部非空、互不重叠，60Hz 与 100Hz 纯音的视觉模式清晰可分。
   提高 bands 到 1024 不是必须的（但见 §9 的权衡表）。
3. 换自有 FFT（方案 C）不是必要条件；它解决的是 GStreamer 不具备的
   overlap/功率域平均/双分辨率窗等"上限"问题，属于可选的长期增强。

## 1. 基线代码（eb3efe3）

| 位置 | 现状 |
| --- | --- |
| `src/player.rs:14-16` | `SPECTRUM_BANDS=512`、`SPECTRUM_INTERVAL_NS=50ms`、`SPECTRUM_THRESHOLD_DB=-72` |
| `src/player.rs` 管线 | spectrum 作为 playbin 的 `audio-filter`；`multi-channel=false`、`message-magnitude=true`；从 sink pad caps 动态读 `rate` |
| `src/app.rs` | `map_spectrum_frame`：32 个对数 bar（50Hz→8kHz），`band_max_magnitude` 半开区间取 dB max；`normalize_magnitude` (-72..-12)；gamma 0.75；attack 0.85 / decay 0.28 |
| `src/app.rs on_tick` | drain 全部 SpectrumFrame，只保留最后一帧进 `spectrum_target`；非 Playing 时清零 |
| `src/ui.rs` | `draw_visualizer`：bar 数 = min(32, ⌈内宽/2⌉)；`resample_spectrum` 重叠窗口 max-pooling；▁▂▃▄▅▆▇█ 八分之一块字符 |

## 2. 探测 1：GStreamer spectrum 数据语义（gstspectrum.c 1.28.6 源码事实）

容器与 Windows 捆绑版同为 **1.28.6**，单一源码覆盖两个平台。以下行号指
`Temp/spectrum-research/gstspectrum-1.28.6.c`。

| 问题 | 源码事实 | 出处 |
| --- | --- | --- |
| FFT size 与 bands 关系 | **`nfft = 2 × bands − 2`**（512 → 1022，非 2 的幂；底层 kissfft 支持复合尺寸） | L246 `guint nfft = 2 * bands - 2` |
| band index → 频率 | 实 FFT 输出 nfft/2+1 = bands 个复数 bin，**band i 中心 = i × rate / (2·bands−2)**，i∈[0,511]；bin 0 = DC，bin 511 = nyquist | `freqdata[bands]` + FFT 语义 |
| 窗函数/长度 | **Hamming**，长度 = nfft 个采样（512 bands @44.1k = 23.2ms，@48k = 21.3ms） | `gst_spectrum_run_fft` 中 `gst_fft_f32_window(..., GST_FFT_WINDOW_HAMMING)` |
| interval 内多次 FFT？ | **是**。每消耗满 nfft 帧跑一次 FFT（窗与窗**无重叠**）；50ms@44.1k = 2205 帧 → **每消息 2 次 FFT**；每次的 dB 值**累加**，发消息时除以次数 | `transform_ip` 的 `num_frames % nfft == 0` 分支；`run_fft` 中 `spect_magnitude[i] += val`；`prepare_message_data` 中 `/= num_fft` |
| magnitude 的 dB 定义 | `val = (re² + im²) / nfft²; val = 10·log10(val)`，即**归一化功率的 10·log10 ≡ 幅度的 20·log10** | `run_fft` |
| threshold 行为 | 逐 FFT 在 **dB 域 clamp**（`if (val < threshold) val = threshold`），**再**参与跨窗平均。静音实测精确 −72.0 ✓ | `run_fft` |
| multi-channel=false | **时域合并**：各声道求算术平均成单声道后再 FFT | `input_data_mixed_*` |
| 采样覆盖 | 50ms interval 实际只分析 2×1022 = 2044 帧，**约 7%(@44.1k)/15%(@48k) 的采样从不进入任何 FFT 窗** | `transform_ip` 窗口推进逻辑 |

**对当前假设的修正：**

- 当前代码 `bin_width = nyquist / frame.len()` = rate/(2·bands)，真实值
  rate/(2·bands−2) —— 系统性偏小 0.2%（512 时），8kHz 处 bin 索引偏差约 0.4，
  影响轻微但应改为精确公式。
- `10f32.powf(db / 20.0)` 恢复的是**线性幅度**（含常数因子 1/nfft 与 Hamming
  相干增益 ≈0.54；相对比较时抵消）。要功率用 `10^(db/10)`。两者都有明确来源，
  不是猜测。
- 实测交叉验证（探测 3）：0.8 幅度正弦稳态读 −13.5~−14.9dB（理论
  20log10(0.8)−5.35(Hamming)−scalloping ≈ −7.3…−9，主瓣分裂到相邻 bin 后单
  bin 再低 5-6dB，吻合）；幅度降 6dB → 读数降 6.02dB（线性 ✓）。

## 3. 探测 2：频率覆盖表

32 根 log bar（50Hz→8kHz，每根 ×1.172）对 GStreamer source bins 的覆盖，
三种分配策略：A=当前 floor..ceil 半开区间（允许共享）；B=中心归属（bin 中心
落入哪根就只属于哪根）；C=单调量化边界（CAVA 式去堆叠，严格递增、互不重叠、
每 bar ≥1 bin）。

### 3.1 统计汇总

| rate | bands | bin 宽 | FFT 窗 | A：独立/共享 | B：独立/空 | C：非空（全部互斥） |
| --- | --- | --- | --- | --- | --- | --- |
| 44.1k | 512 | 43.2Hz | 23.2ms | 19 / **13** | 26 / 6 | 32/32 |
| 44.1k | 1024 | 21.6Hz | 46.4ms | 23 / 9 | 30 / 2 | 32/32 |
| 44.1k | 2048 | 10.8Hz | 92.8ms | 29 / 3 | 32 / 0 | 32/32 |
| 48k | 512 | 47.0Hz | 21.3ms | 18 / **14** | 26 / 6 | 32/32 |
| 48k | 1024 | 23.5Hz | 42.6ms | 23 / 9 | 29 / 3 | 32/32 |
| 48k | 2048 | 11.7Hz | 85.3ms | 27 / 5 | 31 / 1 | 32/32 |
| 96k | 512 | 93.9Hz | 10.6ms | 14 / **18** | 22 / 10 | 32/32 |
| 96k | 1024 | 46.9Hz | 21.3ms | 18 / 14 | 26 / 6 | 32/32 |
| 96k | 2048 | 23.4Hz | 42.6ms | 23 / 9 | 29 / 3 | 32/32 |

（B 的"空"= 无任何 bin 中心落入该 bar，即死柱；C 的边界严格递增所以永不重叠、
永不为空，代价是低频区每根 bar 恰好 1 个 bin、视觉位置与标称频率有约 ±1 根的
近似。）

### 3.2 前 12 根 bar 详表（48k / 512，bin=46.97Hz）

| bar | 标称范围(Hz) | A 读到的 bins | A 中被共享 | B 中心数 | C 拥有 bins |
| --- | --- | --- | --- | --- | --- |
| 0 | 50.0–58.6 | {1} | 1/1 | 0 | {1,2} |
| 1 | 58.6–68.7 | {1} | 1/1 | 0 | {3} |
| 2 | 68.7–80.5 | {1} | 1/1 | 0 | {4} |
| 3 | 80.5–94.3 | {1,2} | 2/2 | 1 | {5} |
| 4 | 94.3–110.5 | {2} | 1/1 | 0 | {6} |
| 5 | 110.5–129.5 | {2} | 1/1 | 0 | {7} |
| 6 | 129.5–151.7 | {2,3} | 2/2 | 1 | {8} |
| 7 | 151.7–177.8 | {3} | 1/1 | 0 | {9} |
| 8 | 177.8–208.4 | {3,4} | 2/2 | 1 | {10} |
| 9 | 208.4–244.2 | {4,5} | 2/2 | 1 | {11} |
| 10 | 244.2–286.2 | {5,6} | 2/2 | 1 | {12} |
| 11 | 286.2–335.4 | {6,7} | 2/2 | 1 | {13} |

**A 策略下 bar 0/1/2 读的是同一个 bin 1**（47Hz）；bar 4/5 同读 bin 2……
这就是"左边几根柱完全同步"的直接来源。96k（Hi-Res 文件）时更糟：18/32 共享。

## 4. 探测 3：受控信号实验

方法：Python 生成 44.1kHz 立体声 WAV 刺激 → 容器内
`gst-launch-1.0 ... ! spectrum bands=512 interval=50ms threshold=-72 ! fakesink`
采集消息（刺激与日志在 `Temp/spectrum-research/`，可复现）。

### 4.1 纯音稳态（幅度 0.8，bin=43.15Hz）

| 刺激 | 期望 bin | 峰值 bin | 峰值 dB | >-50dB bin 数 | 裙(峰值-30dB) |
| --- | --- | --- | --- | --- | --- |
| 60Hz | 1.39 | 1 (43Hz) | −14.40 | 4 | 0..3 |
| 80Hz | 1.85 | 2 (86Hz) | −13.49 | 4 | 1..3 |
| 100Hz | 2.32 | 2 (86Hz) | −14.05 | 4 | 1..4 |
| 150Hz | 3.48 | 3 (129Hz) | −14.85 | 4 | 2..5 |
| 250Hz | 5.79 | 6 (259Hz) | −13.63 | 4 | 5..7 |
| 1kHz | 23.17 | 23 (992Hz) | −13.55 | 4 | 22..24 |
| 4kHz | 92.70 | 93 | −13.96 | 4 | 91..94 |
| 8kHz | 185.40 | 185 | −14.43 | 4 | 184..187 |
| 100Hz 幅度0.4 | 2.32 | 2 | −20.07 | 4 | 1..4 |
| 100Hz 幅度0.1 | 2.32 | 2 | −32.11 | 3 | 1..4 |

要点：Hamming 主瓣 ±2 bin（一个纯音实际影响 **4-5 个 source bin** 于
>-40dB 水平，>-60dB 水平多达 16 个）；60Hz 与 80Hz 的峰值都落在 bin 1/2，
**低频区 source 层面就无法区分 60/80Hz**；幅度线性（−6dB→−6.02dB）。

### 4.2 瞬态与时间平均（本报告最重要的实验）

| 刺激 | 50ms interval 峰值 | 20ms interval 峰值 | 持续音参考 |
| --- | --- | --- | --- |
| 20ms 脉冲串 100Hz (0.8) | **−43.0dB** | **−14.1dB** | −14.0dB |
| 20ms 脉冲串 1kHz (0.8) | −42.9dB | — | −13.6dB |
| 底鼓 60Hz（τ=30ms 指数衰减，250ms 间隔） | −20.0dB | −15.5dB | −14.4dB |

- 50ms 消息的 −43dB 精确等于 `(−14 + −72)/2`：脉冲落进两个 FFT 窗之一，
  另一个窗静音，**dB 域算术平均**直接把瞬态压掉 29dB。
- 20ms interval（每消息强制恰好 1 次 FFT、无跨窗平均）后脉冲满血 −14.1dB；
  底鼓衰减包络在 50FPS 下被完整解析（−16,−22,−27,…,−72 每消息一阶）。
- 白噪声（RMS 0.2）：各 bin 中位 −55dB、最高 −46.9、最低 −68.3，基本平坦。
- 静音：全部精确 −72.0。

**结论：GStreamer spectrum 自身在 50ms interval 下做了"23ms Hamming 窗 × 2 次
dB 平均 + 7-15% 采样丢弃"。瞬态信息在源码层存在，是 message 聚合参数把它抹平的；
后处理参数选对（interval ≤ 窗长）即可恢复，无需换分析器。**

## 5. 探测 4：band aggregation 比较（实测数据驱动）

用实测 bin 级数据，经当前归一化（-72..-12 clamp + gamma 0.75）后前 10 根 bar：

```
== 60Hz 纯音(0.8) ==          bar0  bar1  bar2  bar3  bar4  bar5  ...
A共享+max dB [当前]           0.97  0.97  0.97  0.97  0.95  0.95  0.68 0.68 ...
A共享+mean amp                0.97  0.97  0.97  0.96  0.95  0.88  0.68 0.61 ...
A共享+mean power              0.97  0.97  0.97  0.96  0.95  0.91  0.68 0.64 ...
B中心+任意                     0.00  0.00  0.00  0.95  0.00  0.68 ...（死柱）
C单调+max dB                  0.97  0.95  0.68  0.35  0.35  0.35  0.34 0.33 ...
C单调+mean amp                0.92  0.95  0.68  0.35  0.35  0.35  0.34 0.33 ...

== 100Hz 纯音(0.8) ==
A共享+max dB [当前]           0.81  0.81  0.81  0.97  0.97  0.97  0.94 0.94 ...
C单调+max dB                  0.81  0.97  0.94  0.63  0.30  0.37  0.37 0.36 ...

== 白噪声(0.2) ==
A共享+max dB [当前]           0.37  0.37  0.37  0.41  0.41  0.44  0.44 0.44 ...
C单调+max dB                  0.37  0.41  0.44  0.40  0.33  0.30  0.45 0.50 ...
```

判读：

- **aggregation 与 assignment 是正交问题**，实测证明：A 策略下把 max 换成
  mean amp / mean power，bar 0-2 依然一模一样（它们就是同一个 bin 的同一个值）。
  "max → mean" 无法从不独立的数据里变出独立信息。
- A+max 下 60Hz 与 100Hz 的 bar 模式几乎相同（都是 0-5 根顶满）——
  "低频多柱同步 + 顶高"定量复现；C 策略下两者立即可分，且噪声低频端不再成对重复。
- mean amp / mean power 的作用体现在 C 策略的中高频多 bin 区：略微压低单 bin
  尖峰对整根的支配（60Hz bar0 0.97→0.92），对噪声有 1-3 点的平滑。
  差别不大但方向健康；mean power 与 mean amp 行为接近，mean amp 实现更直接。
- 单一异常 bin 支配：max 聚合在 C 策略下仍会让"一根噪 bin 顶满一根 bar"，
  mean amp 在 ≥2 bin 的 bar 上天然缓解；512 bands 低频区每 bar 只有 1 bin，
  此时两者等价。

**推荐：C 单调量化分配 + mean linear amplitude（10^(db/20) 求算术平均后再取
20·log10）**。max dB 可以删除——它的唯一优势（对单 bin 峰最敏感）正是低频
顶死的帮凶。

## 6. 探测 5：CAVA Frequency EQ（以工作区当前源码为准）

CAVA linear 模式的完整链路（cavacore.c）：

1. band 内对原始 FFT 复数 bin 取 `hypot(re, im)` **线性幅度累加**（L396-411）；
2. 乘 `eq[n]`（L296-310），`eq[n] = 2^-28 × f_c^0.85 ÷ log2(FFTsize) ÷ bin数`：
   - `2^-28`：FFTW 未归一化输出的绝对尺度常数；
   - `f_c^0.85`：按**真实截止频率**的高频提升；
   - `÷ log2(FFTsize)`：FFT 尺寸归一化；
   - `÷ bin数`：bin 计数归一化（= 把累加变成均值）；
3. 低频区（<100Hz）用 **2 倍长度的独立 FFT**（bass buffer），换取低频分辨率。

我们的处境不同：输入已是 GStreamer 处理后的 **dB/固定 43Hz band**，不是原始
FFT。因此：

- 绝对尺度常数、FFT 尺寸归一化与我们无关；
- bin 计数归一化已包含在"mean amp"聚合里（§5）；
- 剩下唯一有价值的是**温和的高频提升**，用于抵消音乐频谱的自然滚降
  （粉噪约 −3dB/oct）。建议：**按 bar 真实中心频率的温和 EQ**（而非按 bar 位置——
  C 量化后低频区 bar 位置与真实频率有偏差，且采样率变化会移动 bin），
  强度做成一个可调常数，初始建议约 +1.5~3 dB/oct（远弱于 CAVA 的 0.85 次方
  在其量纲下的效果），并用 autosens 兜底整体响度。**禁止照抄 f^0.85**——
  那个指数是和 CAVA 自己的线性幅度量纲、bin 求和、双 FFT 结构耦合调出来的。
- 备选：无 EQ 也可接受（gamma 0.75 已经提了中低幅值），EQ 是锦上添花而非必需。

## 7. 探测 6：Autosensitivity A/B

CAVA 当前实现（cavacore.c L436-491）：全局 `sens`，每帧
`overshoot(任一 bar>1) → sens ×= (1 − 0.02×m)`；否则非静音
`sens ×= (1 + 0.001×m×autosens)`；`m = 66/实测帧率`；启动阶段 10×m 快速爬升。

| 维度 | CAVA 式全局 sens（撞顶快降/非静音慢升） | Target-peak AGC（desired=target/peak） |
| --- | --- | --- |
| pumping | 撞顶才降（2%/帧），慢升 0.1%/帧；鼓点偶尔触发整谱缓降，较温和 | 每帧追踪峰值，鼓点一来整谱立刻缩小，**pumping 明显** |
| 鼓点表现 | 撞顶后缓慢让步，多数鼓点能冲满 | 鼓点把 gain 拉低，后续段落整体变小，听感"闪缩" |
| 安静段 | 非静音即慢升，安静段逐步提亮（约 ×1.06/s@60fps） | peak 小 → gain 大，提亮快但噪声地板也被抬 |
| 响歌→轻歌 | 恢复慢（sens 需从低位爬回，数十秒级） | 快（peak 一小立刻升 gain） |
| 轻歌→响歌 | 快（撞顶多帧内即降） | 快 |
| 稳定时间 | 启动 10%/帧快升，约 1-2s 收敛 | 几乎即时 |

**推荐：CAVA 式全局 sens**（pumping 更可控；响→轻恢复慢可用"启动快升阶段"
和对 sens 设上下限缓解），配合 §5 的轻 EQ。Target-peak 的 pumping 对
"鼓点是否导致整个频谱突然缩小"这个问题的回答是肯定的，不建议。

**切歌时 gain 策略**（CAVA 没有的场景）：三选分析——
- reset：每首歌前 1-2s 要么爆顶要么过暗，体验断裂；
- 全保留：跨曲目响度差（如无 ReplayGain，可达 10dB+）会让轻歌前半段偏暗，
  但 sens 会在数秒内跟上；
- **推荐：保留 + 向 1.0 回归 50%（折半继承）**：既不留断档，又加速重新收敛；
  配合 sens 的上下限 clamp，任何路径都有界。

## 8. 探测 7：Temporal response

| 方案 | 行为 |
| --- | --- |
| 当前 Attack0.85/Decay0.28（线性 EMA @20FPS） | 上升 τ≈1.2 tick≈58ms，下降 τ≈3.6 tick≈178ms；尾巴匀速拖长 |
| Gravity only（上升即时 + 二次加速下落） | 冲击强、落得脆；但**单帧抖动明显**——50FPS 源相邻消息同 bin 可跳 ±3-6dB（噪声实测 bin 间 ±3dB），没有记忆项时柱子会"哆嗦" |
| Gravity + minimal smoothing | 上升保留小 attack 或轻积分，下落二次加速；兼顾冲击与稳定 |
| CAVA falloff+integral（当前源码） | `out = mem×0.77/m^0.1 + new` 的漏积分（记忆约 4 帧@60fps）+ `peak×(1−fall²×g)` 二次下落（fall 每帧 +0.028，约 330ms 落尽@60fps） |

**回答"只实现 gravity 会不会过于抖动"：会。** CAVA 的 smoothing 之所以同时存在
falloff 与 integral，正是因为纯 gravity 在真实帧率下抖动。但我们的输入是 dB 域
（天然比 CAVA 的线性幅度平滑），不需要 CAVA 那么重的记忆（k≈0.76）——
推荐：**保留一个温和 attack（0.5~0.7）+ gravity 式二次加速下落 + 很轻的积分
（k≈0.2~0.3）**，常数做成可调，以目验为准。不为"像 CAVA"照搬数值。

## 9. 探测 8：UI 层审查（本阶段不修改）

`resample_spectrum`（ui.rs）把 32 bars 按 `⌈内宽/2⌉` max-pooling：

| 终端内宽 | bar 数 | 池化窗口 | 效果 |
| --- | --- | --- | --- |
| ≥64 列 | 32 | 1:1 | 无影响 |
| ~40 列 | 20 | 重叠 1-2 根取 max | 强峰向两侧各扩一列 |
| ~20 列 | 10 | 重叠 3-4 根取 max | 低频强峰覆盖整片低频区，**重新产生峰值支配** |

窄终端下 max 是峰值支配的二次来源。mean 会稀释瞬态；**加权 mean（三角核）**
是窄宽度的更好折中。但治本在源头映射与灵敏度，UI 池化属于收尾优化。

垂直分辨率澄清：`spectrum_block` 用 ▁▂▃▄▅▆▇█ 八分块（`(fill×8).ceil()`），
2-5 行高对应 **16-40 档**，不是"只有 5 档"——垂直分辨率不是嫌疑。

## 10. 探测 9：512 / 1024 / 2048 的真正代价

| bands | nfft | bin@44.1k | 窗长@44.1k | 50ms 内 FFT 次数 | 20ms 脉冲实测峰值 | 低频 32 bar 独立性（C 策略） |
| --- | --- | --- | --- | --- | --- | --- |
| 512 | 1022 | 43.2Hz | 23.2ms | 2（dB 平均） | −43.0dB | 全部独立（低频每 bar 1 bin） |
| 1024 | 2046 | 21.6Hz | 46.4ms | 1 | −22.4dB | 全部独立（bin 更多、位置更准） |
| 2048 | 4094 | 10.8Hz | 92.8ms | 1 | 预计更差（窗长≈2×interval，能量稀释加剧） | 全部独立 |
| 512 @interval=20ms | 1022 | 43.2Hz | 23.2ms | 1（强制） | **−14.1dB** | 同上 |

- CPU：nfft=1022/2046/4094 均非 2 的幂（含 73/31/89 等质因子），kissfft 在
  大质因子上退化，但绝对规模极小（每帧数十 µs 量级，每秒 ≤60 次），
  **CPU 不是限制因素**。
- 关键权衡不是"bands 越多低音分得越开"——C 映射在 512 下已解决分离；
  而是 **interval 与窗长的比例决定瞬态抹平程度**。1024@50ms 恰好
  "每消息 1 次 FFT"所以瞬态比 512@50ms 好 21dB，但它的窗长 46ms 使分析延迟
  和窗内稀释翻倍；2048 的 93ms 窗对打击乐是灾难。
- **推荐：512 bands + interval 缩到 ≈20ms（≤窗长），瞬态满血且分辨率够用；
  1024 仅在"坚持 50ms interval"时作为次优（意外获得单 FFT/消息）。**

## 11. 探测 10：刷新率配合

- 当前：source 20FPS（interval 50ms）、UI poll ≈50ms，`on_tick` drain 后只保留
  最后一帧。1:1 匹配时没有浪费，但任何相位抖动都会偶发丢帧。
- 若 source 升到 50FPS 而 UI 仍 20FPS：**60% 的帧被直接丢弃**（白算且浪费
  瞬态信息——丢掉的帧里可能正包含鼓点峰值）。
- 建议配合方式（研究结论，不实施）：source 与 render 同频（如都 30 或 50FPS），
  或 source 高频 + render 端对丢弃帧做 peak-hold 聚合而不是简单 last-wins。
  附带收益：UI poll 从 50ms 降到 20-33ms 同时改善键盘响应延迟。

## 12. 四大症状的机制归因汇总

| 症状 | 机制 | 证据 |
| --- | --- | --- |
| 低频长期顶高 | bin1(43Hz)承载全部 sub-bass 且被 bar0-2 共享；max-dB 聚合对 Hamming 泄漏裙敏感；-72..-12 归一化让稳态低音(−14dB)≈0.97；无自动灵敏度 | §3.2 覆盖表；§5 实测 0.97×4 |
| 低频多柱同步 | A 映射多 bar 读同一 bin（512 下 13-14/32 根共享，96k 下 18/32） | §3 统计；60Hz vs 100Hz 在 A 下视觉模式几乎相同、C 下清晰可分 |
| 中高频活动不足 | 音乐谱自然滚降 + 无 EQ；50ms 双窗 dB 平均把中高频瞬态内容压掉 29dB | §4.2（burst −43dB）；§4.1 稳态谱 |
| 下降缺乏打击感 | 瞬态峰值起点先被平均压低；线性 decay 尾巴匀速；20FPS 采样稀疏 | §4.2；§8；§11 |

## 13. 决策矩阵

评分 1-5（越高越好）。

| 维度 | 方案 A：最小改动（512 + 新映射/聚合/EQ/autosens/temporal，interval 仍 50ms） | 方案 B：增强 GStreamer（512 + interval≈20ms + 方案 A 全部后处理） | 方案 C：自有 FFT / cavacore 类 |
| --- | --- | --- | --- |
| 视觉效果 | 3（低频分离解决，瞬态仍被平均压制） | 5（分离+瞬态兼得，实测满血） | 5（上限最高：overlap、功率域平均、双分辨率窗） |
| 低频分离 | 4（C 映射，512 下低频每 bar 1 bin） | 4 | 5（可加 bass 长窗） |
| 瞬态响应 | 2（双窗平均是 source 侧限制，后处理救不回） | 5（实测 −14.1dB） | 5 |
| 实现复杂度 | 5（只动 app.rs 映射/后处理） | 4（多改 player.rs 一个属性 + 帧率配合） | 2（FFT 选型、环形缓冲、窗函数、多率适配） |
| CPU | 5 | 5（FFT 次数≈翻倍但绝对量可忽略） | 4（自己管 overlap 会更多 FFT） |
| Windows 打包影响 | 5（无） | 5（无，同一 1.28.6 安装器） | 2（新依赖进 Cargo.lock/vendor/交叉编译验证） |
| 维护成本 | 5 | 5 | 3（自管分析器长期维护） |

**推荐方案 B。** 方案 A 留着"interval 不动"的硬伤；方案 C 只在未来确需
overlap/双窗/功率域平均等 GStreamer 给不了的特性时再议。

## 14. 十二问逐一回答

1. **GStreamer magnitude 的精确定义**：`10·log10((re²+im²)/nfft²)`，nfft=2·bands−2，
   Hamming 窗后单通道实 FFT 的 bin 功率归一化值；interval 内多次 FFT 的该值做
   dB 域算术平均；低于 threshold 逐 FFT clamp 到 threshold。
2. **nyquist / frame.len() 假设**：近似正确但分母应为 `2·bands−2` 而非
   `2·bands`；512 时偏小 0.2%，应改用精确公式 `rate/(2·bands−2)`。
3. **512 bands 下 32 log bars 的 source-data collision**：A 策略 48k 下 14/32 根
   共享（44.1k 13/32，96k 18/32）；前 3 根 bar 读同一个 bin。
4. **"左边几根同步顶住"有多少来自分辨率/映射**：绝大部分来自映射（A 共享）+
   max-dB 聚合 + 无 autosens；分辨率本身只决定"低频每根 bar 只有 1 bin"，
   经 C 映射后不再同步。实测：60/100Hz 在 A 下不可分、C 下清晰可分。
5. **当前 max dB 应否删除**：应删除，换 mean linear amplitude；它对单 bin 峰的
   敏感性正是低频顶死的帮凶，且在共享 bin 上无任何信息优势。
6. **推荐 band aggregation**：mean linear amplitude（bin 级 `10^(db/20)` 平均
   后再取 dB）；512 低频单 bin 区与 max 等价，多 bin 区更稳。
7. **推荐 source-band assignment**：C 单调量化边界（CAVA 式去堆叠，严格递增、
   互不重叠、每 bar ≥1 bin）。B 中心归属在低频产生死柱，不可用。
8. **512/1024/2048 推荐**：**512**（配 interval≈20ms）。1024 是"坚持 50ms
   interval"时的次优（恰好单 FFT/消息，但窗长翻倍、分析延迟翻倍）；2048 的
   93ms 窗对瞬态有害。CPU 都不是问题。
9. **是否需要 Frequency EQ**：非必需但有益。若做，按 bar 真实中心频率的温和
   曲线（约 +1.5~3dB/oct 量级、可调），不按 bar 位置、不照抄 CAVA f^0.85。
10. **Autosens 推荐**：CAVA 式全局 sens（撞顶快降 ~2%/帧、非静音慢升 ~0.1%/帧、
    帧率修正、启动快升阶段），加上下限 clamp；**切歌时折半继承（向 1.0 回归
    50%）**，不 reset 也不全保留。不用 target-peak AGC（pumping 明显）。
11. **Gravity 是否必须搭配 temporal integration**：必须搭配（或保留温和
    attack）。纯 gravity 在 50FPS 真实源下会抖动；我们的 dB 域输入只需要
    比 CAVA 轻得多的记忆（k≈0.2-0.3）。
12. **最终结论**：**B —— 继续用 GStreamer，但必须重做 mapping（C 策略）**；
    同时把 interval 缩到 ≤ 一个 FFT 窗（≈20ms）以解除 source 侧瞬态抹平；
    bands 保持 512 即可，提高 bands 不是必要条件。GStreamer 的频率分辨率
    不构成主要限制；50ms interval 的双窗 dB 平均才是。

## 15. 附录：可复现性

- 源码：`Temp/spectrum-research/gstspectrum-1.28.6.c`（GitLab 官方 tag 1.28.6，
  与容器及 Windows 捆绑版一致）。
- 覆盖表：`python3 Temp/spectrum-research/coverage_tables.py`
- 刺激生成：`python3 Temp/spectrum-research/gen_stimuli.py`
- 采集（容器内）：`bash Temp/spectrum-research/run_experiments.sh 512 native`
  （另有人工跑的 1024 bands 与 20ms interval 两组，日志在
  `Temp/spectrum-research/logs/`）
- 分析：`python3 Temp/spectrum-research/analyze.py <日志目录>`、
  `python3 Temp/spectrum-research/aggregate_compare.py`
- CAVA 源码：工作区 `cava/`（cavacore.c 频率分配 L190-320、聚合 L388-433、
  smoothing/autosens L436-491；cava.c monstercat 空间滤波 L268 起）。
