# 系统媒体控制功能：当前情况与决策表

## 一、当前情况概览

项目当前基线：

- `master @ 9883a67`
- 版本：`v1.1.0`
- 播放后端已经迁移到：
  - Rodio 0.22.2
  - Lofty 0.25.1
  - RustFFT
- GStreamer 已经完全移除
- Linux / Windows 打包均已正常
- 频谱已经改为基于 PCM 的分析链路
- 当前正式支持：
  - Fedora Linux
  - Windows 11 x86_64

当前播放器架构大致为：

```text
App
├─ 曲库
├─ 播放队列
├─ 播放历史
├─ 播放模式
├─ 当前曲目
├─ Next / Previous
└─ Player
   └─ Rodio 播放单首音频
```

当前 `Player` 主要负责：

```text
play(path)
toggle_pause()
stop()
seek_relative()
position()
duration()
volume
mute
state
current_path
```

当前 `App` 才是真正掌握播放器行为的层：

```text
queue
history
playing_index
PlayMode
play_next()
play_previous()
错误跳过
自然 EOS
随机 / 循环
```

因此系统媒体控制的推荐架构已经基本确定为：

```text
Linux MPRIS / Windows SMTC
            │
            ▼
      MediaSession
            │
      MediaCommand
            │
            ▼
           App
            │
            ▼
          Player
            │
            ▼
          Rodio
```

状态同步反向：

```text
App
 │
 ▼
MediaSnapshot
 │
 ▼
MediaSession
 ├─ Linux MPRIS
 └─ Windows SMTC
```

核心原则：

- 平台回调线程不能直接操作 `App`
- 平台回调线程不能直接操作 Rodio
- 系统媒体控制和键盘控制必须尽量复用同一套 App 行为
- MPRIS / SMTC 初始化失败不能影响播放器正常启动
- Linux / Windows 均作为正式发布功能，而不是实验性单平台功能

---

# 二、平台技术路线

## Linux

已确认推荐：

```text
MPRIS 2
+
mpris-server 0.10
```

不推荐：

```text
souvlaki
```

主要原因：

- MPRIS 能力不够完整
- Linux 默认可能引入 libdbus
- Windows 端依然绕不开 HWND
- 依赖版本较旧
- 对第二阶段 Loop / Shuffle / Seek 等高级功能限制较多

Linux 第一版最终目标不是“最小 MPRIS Demo”，而是正式可用的完整系统媒体集成。

---

## Windows

已确认推荐：

```text
Windows SMTC
+
windows crate
+
自建隐藏 Win32 顶层 HWND
```

不建议依赖：

```text
GetConsoleWindow()
```

不建议把：

```text
HWND_MESSAGE
```

作为正式 SMTC 宿主。

推荐结构：

```text
独立 Windows MediaSession 线程
↓
初始化 COM / WinRT
↓
创建不可见 top-level HWND
↓
ISystemMediaTransportControlsInterop::GetForWindow
↓
SystemMediaTransportControls
```

隐藏窗口要求：

```text
不显示
不抢焦点
不出现在任务栏
不影响 Windows Terminal / ConPTY
程序退出时销毁
```

---

# 三、已定决策

## 决策 1：正式版本功能范围

状态：

**已定**

选择：

**B：正式发布时尽量做功能完整版本。**

原则：

```text
开发中的半成品
→ 内部测试即可

正式 Release
→ 尽量提供完整的系统媒体控制体验
```

因此正式目标不只包含：

```text
Play
Pause
Stop
Next
Previous
Metadata
```

还计划包括：

```text
精确 Seek
绝对 Position
系统 timeline
Volume
Loop
Shuffle
Linux MPRIS
Windows SMTC
```

封面除外，见后续决策。

---

## 决策 2：Linux / Windows 开发策略

状态：

**已定**

选择：

```text
Linux + Windows 一起完成
```

理由：

- 正式发布应尽量功能完整
- 不希望 Release 中出现一边有系统媒体控制、一边没有的情况
- 两个平台可以共享：
  - `MediaCommand`
  - `MediaSnapshot`
  - App 级播放行为
- 平台差异只留在：
  - `linux.rs`
  - `windows.rs`

推荐开发组织：

```text
共享架构先完成
↓
Linux MPRIS
↓
Windows SMTC
↓
双平台验收
↓
正式发布
```

---

## 决策 3：Previous 的语义

状态：

**已定**

选择：

```text
Previous = 当前播放器已有的 history 回退
```

即：

```text
当前键盘 p
=
系统 Previous
```

不额外实现：

```text
曲库索引 - 1
```

理由：

- 避免键盘和系统控制出现两套 Previous 语义
- 当前 `history` 已经是产品正式行为
- 不因为系统媒体控制顺便改变播放器导航模型

未来如果决定增加“曲库上一首”，应作为播放器整体产品行为单独设计。

---

## 决策 4：暂停后的播放位置

状态：

**已定**

要求：

```text
Pause
→ 保留当前位置

再次 Play
→ 从原位置继续
```

明确禁止：

```text
Pause
→ position 清零
```

因为这会严重破坏正常使用体验。

---

## 决策 5：Paused 状态切换 Next / Previous

状态：

**已定**

选择：

```text
Playing
→ Next / Previous
→ 新歌曲继续 Playing

Paused
→ Next / Previous
→ 切换歌曲
→ 新歌曲仍然保持 Paused
```

即：

```text
切歌保留播放状态
```

正式实现需要避免：

```text
App::play_next()
→ 无条件 Player::play()
```

这一现有行为直接用于系统媒体控制。

建议内部形成：

```text
切换目标歌曲
+
是否自动播放
```

两个相对独立的概念。

---

## 决策 6：封面

状态：

**已定**

选择：

```text
正式第一版系统媒体集成不做封面
```

暂缓：

```text
MPRIS mpris:artUrl
Windows SMTC Thumbnail
Lofty 内嵌 artwork 提取
图片缓存
临时文件生命周期
```

第一版 metadata 包含：

```text
Title
Artist
Album
Duration
Path / URL
```

封面未来单独作为增强功能开发。

---

# 四、当前强烈推荐但尚待最终确认的决策

## 决策 7：PlayMode 是否重构

状态：

**未最终拍板**

### 当前模型

现在是单一四态：

```rust
enum PlayMode {
    Sequential,
    RepeatAll,
    RepeatOne,
    Shuffle,
}
```

这四个状态互斥。

---

### Linux MPRIS 模型

MPRIS 是两个独立维度：

```text
LoopStatus:
- None
- Playlist
- Track

Shuffle:
- false
- true
```

因此可以表达：

```text
顺序播放
列表循环
单曲循环
随机播放
随机 + 列表循环
随机 + 单曲循环
```

---

### Windows SMTC 模型

Windows 同样是两个维度：

```text
AutoRepeatMode:
- None
- Track
- List

ShuffleEnabled:
- false
- true
```

也就是说：

```text
Linux
Windows
```

在这个问题上是高度一致的。

---

### 当前模型的问题

例如当前播放器处于：

```text
RepeatAll
```

然后系统设置：

```text
Shuffle = true
```

协议表达的是：

```text
列表循环 + 随机
```

但当前 `PlayMode` 只能存：

```text
RepeatAll
```

或者：

```text
Shuffle
```

无论选哪个都会丢信息。

---

### 推荐重构

建议改成：

```rust
enum RepeatMode {
    None,
    All,
    One,
}

struct PlaybackMode {
    repeat: RepeatMode,
    shuffle: bool,
}
```

这样：

```text
RepeatMode::None + false
→ 普通顺序

RepeatMode::All + false
→ 列表循环

RepeatMode::One + false
→ 单曲循环

RepeatMode::None + true
→ 普通随机

RepeatMode::All + true
→ 随机列表循环

RepeatMode::One + true
→ 随机状态下单曲循环
```

可以完整无损映射：

```text
App
↕
MPRIS
↕
SMTC
```

---

### 如果选择重构，还应顺便处理 Shuffle 算法

当前 Shuffle 大致是：

```text
每次需要下一首
→ 随机抽一个 index
→ 避免连续抽到当前歌曲
```

因此可能出现：

```text
A
→ C
→ B
→ C
→ A
→ C
```

在 D 一次都没播放前，C 已经重复多次。

这不是严格意义上的“随机播放列表”。

推荐改为：

```text
Shuffle Bag / Random Permutation
```

例如：

```text
原曲库：
A B C D E

本轮随机顺序：
D A E B C
```

依次消费。

到末尾后：

```text
Repeat=None
→ 停止

Repeat=All
→ 重新洗牌生成下一轮

Repeat=One
→ 当前歌曲继续循环
```

这个模型更容易正确实现：

```text
Shuffle
+
Repeat
```

的二维组合。

---

### 该决策需要 Kimi 一起讨论的问题

需要确认：

1. 是否愿意正式废弃四态 `PlayMode`
2. 是否重构为：
   ```text
   RepeatMode + shuffle bool
   ```
3. 是否同步把 Shuffle 改为 permutation / shuffle bag
4. TUI 是否从一个 `z` 键拆成：
   ```text
   一个键控制 Repeat
   一个键控制 Shuffle
   ```
5. 现有 config.toml 如何兼容旧 `play_mode`
6. v1.1.0 用户升级后如何迁移：
   ```text
   Sequential
   RepeatAll
   RepeatOne
   Shuffle
   ```
   到新模型
7. 是否允许：
   ```text
   RepeatOne + Shuffle=true
   ```
   这种协议合法但 UI 上略奇怪的组合

当前推荐：

**重构。**

原因：

不是单纯为了迎合 MPRIS，而是 Linux 与 Windows 都使用同样的二维播放模型，说明现有四态枚举本身把“播放顺序”和“循环方式”混成了一个概念。

---

# 五、尚未决定的重要产品语义

## 决策 8：Stop 到底应该是什么

状态：

**未定**

这是目前最大的剩余产品决策之一。

用户已经明确：

```text
“位置重置非常败坏体验”
```

但需要区分：

```text
Pause
```

和：

```text
Stop
```

---

### 方案 A：严格传统 Stop

```text
Pause
→ 保留位置

Stop
→ 停止
→ Play 从歌曲开头开始
```

优点：

- 接近 MPRIS Stop 的标准语义
- 与很多传统播放器一致

缺点：

- 用户明确不喜欢位置被重置
- 系统误触 Stop 后体验较差

---

### 方案 B：Stop 也保留位置

```text
Pause
→ 保留位置

Stop
→ 停止输出 / 状态变 Stopped
→ 仍然保留当前歌曲和当前位置

Play
→ 从 Stop 前位置继续
```

优点：

- 更符合用户希望的连续播放体验
- 不会因为 Stop 意外丢进度

缺点：

- 与传统 MPRIS Stop 语义存在偏差
- 要明确区分：
  ```text
  Paused
  Stopped-but-resumable
  ```

---

### 方案 C：根本不向系统暴露 Stop

```text
系统支持：
Play
Pause
Next
Previous
Seek
...

Stop:
不提供 / 不启用
```

本地内部仍然可以有真正的：

```text
Player::stop()
```

用于：

```text
自然播放结束
退出程序
播放错误
切换后端状态
```

但用户系统媒体面板不提供 Stop。

优点：

- 日常音乐播放器实际上很少需要 Stop
- 避免位置语义冲突
- 不会出现用户误按 Stop 后进度丢失

缺点：

- “功能完整版”会少一个协议能力
- 某些媒体控制器可能仍尝试调用 Stop，需要定义 fallback 行为

---

### 当前倾向

需要用户 + Kimi 决定：

```text
A. Stop = 真停止并清位置
B. Stop = 停止但可从原位置恢复
C. 不暴露 Stop
```

---

# 六、Seek / Timeline 的实现范围

状态：

**范围已确定要做，但具体 API 尚未定**

因为选择了“正式版功能完整”，因此计划实现：

```text
MPRIS Seek
MPRIS SetPosition
Windows PlaybackPositionChangeRequested
Windows Timeline
```

---

### 当前 Player 缺口

现在只有：

```rust
seek_relative(i64 seconds)
```

精度是：

```text
整数秒
```

正式系统接口需要：

```text
精确 relative seek
精确 absolute seek
```

推荐最终 Player API 类似：

```rust
seek_relative_precise(...)
seek_to(Duration)
```

或者其他等价设计。

核心要求：

```text
MPRIS Seek(+2.35s)
→ 真正 seek +2.35s

SetPosition(01:32.500)
→ 真正到 01:32.500
```

不能把任意 Seek：

```text
粗暴映射成 ±10 秒
```

---

### Timeline 更新策略

不应该：

```text
20ms 主循环
→ 20ms 更新系统 timeline
```

推荐：

```text
切歌
→ 立即更新

Play/Pause
→ 立即更新

Seek
→ 立即更新

正常播放 position
→ 大约 500ms ~ 1s 更新一次
```

Linux MPRIS 的 Position 通常由客户端自行推算，不需要持续 PropertiesChanged。

Windows timeline 可低频刷新。

具体频率可以在实现中调，不需要产品层拍板。

---

# 七、Volume 的系统控制语义

状态：

**需要确认细节**

Linux MPRIS 支持：

```text
Volume
```

Windows SMTC 本身并不是应用音量控制 API，因此 Windows 不一定有与 MPRIS 完全对等的应用 Volume setter。

当前本地模型：

```text
volume: 0..100
muted: bool
```

需要决定 Linux 外部设置：

```text
Volume = 0.5
```

时：

```text
本地 volume = 50
```

如果当前：

```text
muted = true
```

系统把 Volume 改为：

```text
0.5
```

是否：

```text
自动取消 mute
```

推荐：

```text
是
```

因为用户明确设置了一个非零音量。

但这个细节尚未正式拍板。

---

# 八、Loop / Shuffle 对系统是否可写

状态：

**未定，与 PlayMode 重构绑定**

既然正式目标选择了完整版，可以有两个选择。

### A. 完整可读可写

系统可以：

```text
开/关 Shuffle
切换 None / Track / Playlist Loop
```

本地 UI 即时跟随。

这是最完整的实现。

前提：

```text
PlaybackMode
=
RepeatMode + shuffle
```

---

### B. 系统只读

系统能看到当前状态，但不能修改。

这样实现简单，但和“完整版”目标不完全一致。

---

当前推荐：

```text
A. 完整可读可写
```

但应先完成播放模式二维重构。

---

# 九、Play 在 Stopped 状态下播放什么

状态：

**未正式拍板**

候选：

### A

```text
如果存在 current track
→ 恢复 current track

否则
→ 播当前 selected track
```

这是当前推荐。

### B

```text
始终播放 selected track
```

问题：

系统媒体控制根本没有“选中项”概念，因此可能和 TUI 当前光标位置产生意外耦合。

推荐：

```text
A
```

即：

```text
current track 优先
selected track 作为 fallback
```

如果采用 Stop 保留进度，则：

```text
Play
→ current track current position
```

---

# 十、多实例行为

状态：

**未定，但优先级较低**

Linux：

```text
第一个实例：
org.mpris.MediaPlayer2.music_player

第二个实例：
org.mpris.MediaPlayer2.music_player.instance<PID>
```

或：

```text
第二实例不注册 MPRIS
```

Windows：

两个进程可能分别建立 SMTC session。

候选：

```text
A. 每个播放器实例都注册媒体会话
B. 只有第一个实例注册
```

推荐：

```text
A
```

因为播放器本身目前没有单实例限制。

但可以在实现阶段实测后再定。

---

# 十一、媒体控制失败时的行为

状态：

**基本已定**

原则：

```text
MPRIS 初始化失败
→ 播放器照常运行

SMTC 初始化失败
→ 播放器照常运行
```

不能：

```text
系统媒体控制不可用
→ App::new() 失败
```

建议用户侧最多看到：

```text
一条非阻塞 warning
```

甚至可选择静默失败。

具体是否显示提示还未定，但：

```text
不能影响播放
```

已经确定。

---

# 十二、第一版不做的内容

已确定：

```text
封面
MPRIS TrackList
MPRIS Playlists interface
OpenUri
读取其他播放器媒体状态
macOS MediaSession
```

这些不是本次正式系统媒体集成的目标。

---

# 十三、当前决策状态总表

| 编号 | 决策项 | 当前状态 | 当前选择 / 推荐 |
|---|---|---|---|
| 1 | 正式版功能范围 | 已定 | 完整版，包含高级控制 |
| 2 | Linux / Windows | 已定 | 同期完成 |
| 3 | Previous 语义 | 已定 | history |
| 4 | Pause 后位置 | 已定 | 原位置续播 |
| 5 | Paused 下 Next/Previous | 已定 | 切歌后保持 Paused |
| 6 | 封面 | 已定 | 推迟 |
| 7 | PlayMode 模型 | 未定 | 推荐拆成 RepeatMode + Shuffle |
| 8 | Shuffle 算法 | 未定 | 推荐 permutation / shuffle bag |
| 9 | Stop 语义 | 未定 | A 清位置 / B 保留位置 / C 不暴露 |
| 10 | 精确 Seek / SetPosition | 已定要做 | API 设计待实现 |
| 11 | Timeline | 已定要做 | 低频刷新 |
| 12 | MPRIS Volume | 细节未定 | 推荐非零设置自动取消 mute |
| 13 | Loop / Shuffle 系统写入 | 未定 | 推荐完整可读可写 |
| 14 | Stopped 后 Play 目标 | 未定 | 推荐 current track 优先 |
| 15 | 多实例 | 未定 | 推荐每实例独立媒体 session |
| 16 | 初始化失败降级 | 已定 | 不影响正常播放 |
| 17 | Linux 技术栈 | 基本已定 | mpris-server |
| 18 | Windows 技术栈 | 基本已定 | windows crate + hidden top-level HWND |
| 19 | TrackList / Playlist API | 已定 | 本轮不做 |
| 20 | Artwork | 已定 | 后续版本 |

---

# 十四、建议和 Kimi 优先讨论的 5 个问题

优先级最高：

## 1. 是否重构播放模式模型

```text
PlayMode 四态
↓
RepeatMode + Shuffle
```

这是 Loop / Shuffle 完整系统控制的前置决策。

---

## 2. Shuffle 是否升级为真正随机队列

```text
随机抽签
↓
shuffle bag / permutation
```

否则：

```text
Shuffle + Repeat=None
```

很难有清晰的“播放完成”语义。

---

## 3. Stop 到底做什么

三选一：

```text
A. 真 Stop，清进度
B. Stop 但保留进度
C. 不暴露 Stop
```

用户当前明显不接受“意外丢失进度”的体验，因此 A 需要非常强的理由。

---

## 4. Loop / Shuffle 是否允许系统修改

如果已经重构二维模型，推荐：

```text
允许
```

否则正式版仍然会存在系统控制不完整的问题。

---

## 5. Play 在 Stopped 状态的恢复逻辑

推荐：

```text
current track 存在
→ 恢复 current

否则
→ selected track

两者都不存在
→ no-op
```

如果 Stop 保留位置：

```text
恢复 current position
```

如果最终定义 Stop 清位置：

```text
从 current track 0 开始
```
