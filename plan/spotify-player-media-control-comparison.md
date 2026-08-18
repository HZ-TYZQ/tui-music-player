# spotify-player 媒体控制对照表

对照对象：

- 我们：`plan/Decision.md` + 结论简表（v1.1.0 本地播放器）
- 参考：`Temp/spotify-player/`（aome510/spotify-player，浅克隆 `0.24.1`）
- 媒体层几乎全在：`spotify_player/src/media_control.rs`
- 播放命令：`spotify_player/src/client/request.rs` + `client/mod.rs`
- 二维模式与洗牌袋：`spotify_player/src/state/queue.rs`
- 键盘与系统共用命令：`spotify_player/src/event/mod.rs`

性质：只对照，不改本仓库播放代码。

图例：

- **抄**：实现时按这个做
- **可参考**：思路对，细节按我们的决策改
- **不抄**：和已拍板冲突，或依赖 Spotify / souvlaki 特有限制

---

## 0. 一句话

spotify-player 能验证三件事：命令队列、隐藏顶层 HWND、`Repeat × Shuffle` + permutation。
它不能当完整媒体会话范本：走 souvlaki、Windows 还开了会抢焦点的 winit、系统层不暴露 Repeat/Shuffle/Stop 语义。

---

## 1. 总表

| 我们正在讨论的点 | 我们的结论 | spotify-player 怎么做 | 借鉴 |
| --- | --- | --- | --- |
| 架构 | `MediaSession` 线程 → `MediaCommand` → `App` → `Player` | 独立 `media-control` 线程；souvlaki 回调只 `flume` 发 `PlayerRequest`；状态从 `SharedState` 回读 | **抄** 队列方向，不抄 crate |
| 键盘与系统共用行为 | 同一套 App 函数 | 键盘 `Command` 与媒体事件都变成同一个 `PlayerRequest` | **抄** |
| 初始化失败 | 静默降级，不挡 `App::new` | 线程里出错只 `tracing::error`，进程继续 | **抄** |
| Linux 技术栈 | `mpris-server` 0.10 + zbus | souvlaki 0.8.3（默认 libdbus / 可选旧 zbus） | **不抄** crate |
| Windows 技术栈 | `windows` crate + 隐藏顶层 HWND | souvlaki + 自建 0×0 顶层窗 + **主线程 winit** | HWND **可参考**；winit **不抄** |
| `HWND_MESSAGE` | 不用 | 注释写 “message only”，实现父窗口是 `None`，其实是顶层窗 | 实现与我们一致，注释不要学 |
| `GetConsoleWindow` | 不用 | 不用 | 一致 |
| Play / Pause 分立 | 要 | `Resume` / `Pause` / `ResumePause` 三个命令 | **抄** 分立，不要只留 toggle |
| Pause 保位置 | 已定 | 交给 Spotify / librespot，媒体层不碰 position | **可参考**（我们自己的 Rodio 必须显式保证） |
| Paused 切歌仍 Paused | 已定 | 只调 `next_track` / `previous_track`，**没有**“切歌后保持暂停”的本地逻辑 | **不抄**；要我们自己拆「装上这首 / 是否自动播」 |
| Previous | 等于键盘 `p` = history | 自定义队列是 `play_order` 上一位；否则走 Spotify Previous | **不抄** 语义 |
| 精确 Seek / SetPosition | 要，微秒级 | `SetPosition` → `SeekTrack(Duration)`；相对 seek 也先加成绝对再 seek | **抄** API 形状 |
| Timeline | 状态变化立刻刷；播放中 0.5–1s | 固定 1s 全量刷，因为 souvlaki Linux 循环大约 1s 才处理一次 | **可参考** 低频；不要被 souvlaki 绑死 1s |
| Repeat × Shuffle | 拆 `RepeatMode + shuffle` | 本地早已是 `RepeatState` × `shuffle_state` 两轴；TUI 两个键 | **抄** 模型 |
| 系统可写 Repeat / Shuffle | Linux 读写；Windows `AutoRepeatMode` + `ShuffleEnabled` | 媒体层**完全不接** Repeat/Shuffle 事件 | **不抄** 缺口；本地模型仍可抄 |
| Shuffle 算法 | permutation / bag | Fisher-Yates；当前曲置顶；保留 `original_tracks` | **抄** |
| Repeat + bag 边界 | None 停 / All 重洗 / One 不换曲 | `advance()`：Track 不前进；Context 绕回；Off 则 `EndOfQueue` | **抄** |
| Stop | Windows 关按钮；Linux `Stop→Pause`，对外仍 Paused | `MediaControlEvent::Stop` 落入 `_ => {}`；souvlaki 仍启用 Stop 按钮 | **可参考**「系统 Stop 不进播放器」；我们要比它更明确 |
| Linux Volume | 可写；非零取消 mute | `SetVolume` → `u8` 百分比，并 `mute_state = None` | **抄** 取消 mute |
| Windows 应用音量 | SMTC 做不到 | 媒体层照样接 `SetVolume`（souvlaki 在 Win 上基本不会来） | **不抄** 假装两端对称 |
| Metadata | title / artist / album / duration / path | 同上 + `cover_url` | 字段 **可参考**；封面按已定推迟 |
| 元数据刷新 | 切歌 / 状态变再推 | `title/album` 字符串变了才 `set_metadata` | **抄** dirty-check |
| Artwork | 推迟 | 直接塞 Spotify CDN URL | **不抄**（本轮） |
| OpenUri | 不做 | 解析 `spotify:track:...` 等 | **不抄** |
| 多实例 | 每实例各自注册 | 总线名写死 `spotify_player`，第二实例 `MediaControls::new` 失败后线程退出 | **可参考** 失败降级；Linux 我们仍建议 `instance<pid>` |
| 配置开关 | 可选，非必须 | `enable_media_control`；Linux 默认开，Win/mac 默认关（怕抢焦点） | 开关 **可参考**；Win 默认关 **不抄** |
| 封面 / TrackList / Playlists / 读别人 | 本轮不做 | 媒体层无 TrackList；有封面 | 范围一致（除封面） |

---

## 2. 关键逻辑细表

### 2.1 命令队列（抄）

spotify-player：

```text
souvlaki attach 回调
  → ClientRequest::Player(PlayerRequest::…)
  → 播放线程 / Spotify client 执行

SharedState.player
  → 每 1s update_control_metadata
```

对应我们：

```text
MPRIS / SMTC 回调
  → MediaCommand
  → App::on_tick
  → Player

App 写 MediaSnapshot
  → MediaSession 推系统
```

键盘与媒体必须进同一组 `play_or_resume` / `pause` / `play_next` / `play_previous`。
他们已经这样做了，见 `event/mod.rs` 与 `media_control.rs` 共用 `PlayerRequest`。

### 2.2 Play / Pause 分立（抄）

他们不把系统 Play 映射成 toggle：

| 事件 | 命令 | 实现 |
| --- | --- | --- |
| Play | `Resume` | 仅当 `!is_playing` 才恢复 |
| Pause | `Pause` | 仅当 `is_playing` 才暂停 |
| Toggle | `ResumePause` | 键盘空格用这个 |

我们现在只有 `toggle_pause`，Stopped 时空操作。实现时按他们拆开，再补 Stopped 时 Play = current 优先。

### 2.3 Windows HWND（可参考窗口，不抄 winit）

`DummyWindow::new()`：

- `CreateWindowExW`，父窗口 `None`，尺寸 0×0
- **不是** `HWND_MESSAGE`
- 媒体线程里每秒 `PeekMessageW` 泵一次

同时 `main.rs` 在 Windows/macOS **主线程**跑空的 winit `EventLoop`。文档写明这会抢终端焦点，所以 Win 默认关掉媒体控制。

我们应：

- 学 0×0 顶层窗 + 独立线程泵消息
- 补 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`（他们没设，窗口样式是 default）
- **不要**在 TUI 主线程跑 winit
- 程序退出 `DestroyWindow`（他们有 `Drop`）

### 2.4 Seek（抄形状）

媒体层只接绝对位置：

```text
SetPosition(dur) → SeekTrack(dur)
```

键盘相对 seek 也是「当前进度 ± N 秒」再走同一个 `SeekTrack`。
我们应提供 `seek_to(Duration)`，相对 seek 用它实现，不要再走整数秒 `seek_relative`。

他们精度来自 `chrono::Duration` / Spotify 毫秒。我们应对齐 MPRIS 微秒，内部用 `Duration`。

### 2.5 Repeat × Shuffle（抄本地模型）

`PlaybackMetadata`：

```text
repeat_state: Off | Track | Context
shuffle_state: bool
```

TUI：一个键循环 Repeat，一个键 toggle Shuffle。
自定义队列 `CustomQueue` 两轴独立存储。

这直接支持我们废弃四态 `PlayMode`。
差别：他们的媒体会话**不读写**这两轴。我们正式版 Linux/Windows 都要接系统写入。

### 2.6 Shuffle bag（抄）

`set_shuffle_mode(Shuffle)`：

```text
从 original_tracks 去掉当前曲
Fisher-Yates 打乱
当前曲插到 position 0
truncate_batch，本曲播完再生效
```

`advance()`：

| Repeat | 袋尽 |
| --- | --- |
| Track（One） | 不前进 |
| Context（All） | `position = 0` 绕回；未重洗，仍用同一 permutation |
| Off（None） | `EndOfQueue` |

要注意：他们 Repeat=Context 绕回**同一袋**，不是每轮重洗。
我们结论简表写的是 All 时「重新洗牌」。两者都合法，实现前写死一句：

- 跟他们：同一 permutation 循环（用户听到固定随机序）
- 或每轮重洗（更接近“再来一轮随机”）

推荐：**All + shuffle 每轮重洗**，与结论简表一致；Previous 仍走 history，不在袋里倒退。

他们 `retreat()` 是袋内上一首，和我们已定的 history Previous **相反**，不要抄。

### 2.7 Paused 切歌（不抄）

他们 Next/Previous 不读、不写 `is_playing`。Paused 切歌后是否仍暂停，取决于 Spotify / librespot。
我们是本地 Rodio，`play()` 总会进入 Playing。必须按决策 5 拆：

```text
load_track(path)
+
autoplay: bool   // 当时是 Playing 才 true
```

Paused 切歌：装源、seek 0、保持 Paused，不要 `play()` 再立刻 `pause()`（会闪一声）。

### 2.8 Stop（可参考意图）

```text
MediaControlEvent::Stop | Seek | Raise | Quit => {}
```

等于系统 Stop 被扔掉。souvlaki 仍 `SetIsStopEnabled(true)`，面板上可能有 Stop，按了没反应。

我们比他们完整：

| 平台 | 做法 |
| --- | --- |
| Windows | `IsStopEnabled=false`，按钮不出现 |
| Linux | `Stop()` 做成 Pause，对外 `Paused`，保位置 |

不要做成「Stopped + 旧位置续播」。

### 2.9 Volume 与 mute（Linux 抄）

`PlayerRequest::Volume`：

```text
playback.volume = Some(volume);
playback.mute_state = None;   // 设音量即取消 mute
```

与结论简表一致。Windows 不要接 SMTC 音量。

### 2.10 元数据推送（抄 dirty-check）

他们用 `"{title}/{album}"` 变了才 `set_metadata`，避免 1s 循环反复打 COM/D-Bus。
播放状态每次循环都 `set_playback`（带 progress）。

我们：

- Metadata：曲目身份变了再推
- PlaybackStatus：Play/Pause/切歌立刻推
- Position：Windows timeline 0.5–1s；Linux Position 让客户端自己推算，Seek 后发 `Seeked`

不要学他们启动时强行 `set_playback(Playing)`——那是 macOS 状态栏的 workaround。

---

## 3. 明确不要跟的点

1. **souvlaki** 当正式依赖。
2. **winit 主线程事件循环**（Win 默认关媒体控制就是因为它）。
3. **Previous = 列表上一首 / Spotify Previous**。
4. **媒体层不接 Repeat/Shuffle**。
5. **封面 URL、OpenUri**。
6. **1s 全量刷新**当唯一策略（那是 souvlaki 限制）。
7. **Win 默认关闭**媒体控制。我们正式发布两端都要开，靠隐藏顶层窗 + 不抢焦点解决。

---

## 4. 建议直接落进实现的几条

从这份参考里可以冻结的实现要点：

```text
1. MediaSession 独立线程 + mpsc，失败只 warning
2. PlayerRequest 式分立命令：Resume / Pause / Toggle / Next / Previous / SeekTo / SetVolume / SetRepeat / SetShuffle / Quit
3. 键盘与系统走同一组 App 方法
4. Player::seek_to(Duration)；相对 seek 用它
5. RepeatMode + shuffle；配置迁移旧四态
6. Shuffle = 保留原序 + 当前曲置顶的 permutation
7. Windows：0×0 顶层 HWND + 隐藏扩展样式 + 会话线程泵消息；无 winit
8. set_volume(>0) 清 mute
9. metadata dirty-check
10. Stop 不进入 Player::stop()
```

spotify-player **帮不上忙**、必须我们自己写的：

```text
- Paused 切歌不自动播
- Previous = history
- Linux Stop → Pause 且对外 Paused
- Windows 关掉 Stop 按钮
- SMTC ShuffleEnabled / AutoRepeatMode 可写
- mpris-server 属性推送（非 1s 轮询）
```
