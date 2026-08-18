# 媒体控制接口可行性报告（Linux MPRIS + Windows SMTC）

状态：探索完成，待决策（**尚未开始实现**）
日期：2026-08-18
基线：`master` @ `9883a67`（v1.1.0，Rodio + Lofty）
性质：源码核对 + 平台方案调研。未改 `src/`、Cargo、CI、packaging。
范围：让本机媒体控制器识别并遥控本播放器。不含 macOS、不含读其他应用的“正在播放”。

---

## 0. 结论先行

**可行。两端都有成熟、不改播放后端的接入路径。** 建议做成独立的
`MediaSession` 适配层，挂在 `App` 外侧，不要塞进 `Player`。

| 平台 | 系统接口 | 推荐 crate | 无桌面/无总线时 |
| --- | --- | --- | --- |
| Linux | MPRIS 2（session D-Bus） | `mpris-server` 0.10（zbus，纯 Rust） | 启动失败则静默降级，TUI 照常 |
| Windows | SMTC（`ISystemMediaTransportControlsInterop::GetForWindow`） | `windows` crate 的 `Media` + 自建隐藏 HWND | 同上；CI / 无控制台 HWND 时不注册 |

不建议用 `souvlaki` 做主方案：它能同时盖 Linux/Windows，但 MPRIS 不完整、
Linux 默认绑 `libdbus`、Windows 绑过时的 `windows 0.44`，且 TUI 仍要自己解决 HWND。

**建议分两期：**

1. **识别 + 基本遥控**（用户说的“被媒体控制器识别”）：元数据、播放/暂停/停止、上一首/下一首、播放状态。
2. **进度与模式**：绝对 seek、音量、循环/随机映射、封面（可选）。

第一期不改播放语义，工作量可控。第二期要补 `Player::seek` 绝对定位，并决定
“上一首”是历史回退还是曲库上一首。

---

## 1. 当前源码：控制面在哪

播放器对系统是“聋的”：只有终端按键能改状态。相关事实如下。

### 1.1 主循环是单线程、20/50ms 轮询

`src/main.rs`：`draw` → `event::poll` → `handle_key` → `on_tick`。没有异步运行时，
没有第二路输入。`LibraryWorker` 已经示范了正确模式：后台线程 `mpsc`，主循环
`drain`。

媒体按键 / D-Bus / SMTC 回调都在**别的线程**。必须同样走命令队列，禁止从
D-Bus/SMTC 线程直接 `&mut App` / `&mut Player`。

### 1.2 职责切分已经清楚

| 层 | 文件 | 已有能力 | 媒体接口该不该碰 |
| --- | --- | --- | --- |
| `Player` | `src/player/backend.rs` | `play(path)` / `toggle_pause` / `stop` / `seek_relative` / 音量静音 / `position` / `duration` / `state` / `current_path` / `drain_events` | **不要**。它只负责一根解码管线 |
| `App` | `src/app.rs` | 曲库、队列、历史、`play_next` / `play_previous`、模式、选中项 | **要**。Next/Previous/Play（Stopped 时）都在这里 |
| `Track` | `src/track.rs` | `title` / `artist` / `album` / `duration` / `path` | 够第一期元数据；无封面 |
| `PlayMode` | `src/track.rs` | Sequential / RepeatAll / RepeatOne / Shuffle 四态互斥 | MPRIS 是 LoopStatus × Shuffle 两轴，映射有损 |
| UI | `src/ui.rs` | 只读 `PlayState` | 不改 |
| 桌面项 | `packaging/music-player.desktop` | `Categories=AudioVideo;Audio;Player;`，`Terminal=true` | 可补 `StartupWMClass` / 与 MPRIS Identity 对齐 |

`PlayerEvent::StateChanged` 已定义，但后端**从不发出**；`App::on_tick` 里对应分支是空的。
媒体会话应自己在 `on_tick` 末尾推快照，不要指望这条事件。

### 1.3 与系统协议的缺口

| 协议需要 | 现状 | 影响 |
| --- | --- | --- |
| Play / Pause 分立 | 只有 `toggle_pause`；`Stopped` 时空操作 | Play 在停止态必须改成“播当前或选中曲” |
| Stop | `Player::stop` 有；App 没有对外 Stop，顺序播完才 `stop` + 清 `playing_index` | 要补 App 级 `stop_playback` |
| Next | `play_next(false)`，含队列优先 | 可直接复用 |
| Previous | **只弹 `history`**，历史空则提示，不回到曲库上一首 | 与 MPRIS/SMTC “上一首”常见语义不一致，见 §5 |
| Seek 相对 | `seek_relative(±秒)`，内部已有 `clamp_seek_target` | 第一期可把系统 Seek 映射成 ±10s |
| Seek 绝对 / SetPosition | **没有** | 第二期加 `seek_to(Duration)`，复用现成 clamp |
| Volume 0.0–1.0 | 0–100 + 独立 `muted` | 可映射；设音量 > 0 时应取消静音 |
| 元数据 | title / artist / album / duration / path | 够用 |
| 封面 `mpris:artUrl` / SMTC thumbnail | 扫描不读内嵌图 | 第一期不做 |
| LoopStatus / Shuffle | 一个 `PlayMode` | 第二期再映射 |
| Raise | TUI 没有可前置的窗口 | no-op |
| Quit | `should_quit` | 可接 |
| OpenUri | 只能播库内路径 | 第一期不做 |
| TrackList / Playlists 接口 | 有内部播放列表 | 第一期不做 |

### 1.4 测试约束（沿用 v1.1.0 教训）

`App::new` 会打开 CPAL 默认设备；CI 无声卡，已拆出 `App::new_for_tests`。
媒体会话同样必须：

- 测试路径**不**连 session bus / SMTC；
- 生产路径连不上则**失败降级**，不能让 `App::new` 再因“没 D-Bus / 没 HWND”启动失败。

---

## 2. Linux：MPRIS 2

### 2.1 系统实际认什么

桌面不认“终端里有个播放器”，认的是 session bus 上的：

```text
总线名   org.mpris.MediaPlayer2.<identity>
对象路径 /org/mpris/MediaPlayer2
接口     org.mpris.MediaPlayer2
         org.mpris.MediaPlayer2.Player
```

接上之后，这些东西会自动出现：

- GNOME / KDE 媒体控件、锁屏、通知区
- 键盘媒体键（多数发行版经 MPRIS）
- `playerctl metadata` / `playerctl play-pause`
- 部分耳机/蓝牙 AVRCP（经 PipeWire / 桌面桥）

本机已有 `music-player.desktop`，Identity 建议固定为 `music-player`，
显示名 `Music Player`，与桌面项文件名对齐，方便图标匹配。

TUI **不需要**图形窗口。session bus 在普通图形登录下一定在；SSH / 纯 TTY /
无 `DBUS_SESSION_BUS_ADDRESS` 时可以没有——必须降级。

容器：`dev-fedora` 经 Distrobox 通常能看到宿主机 session bus，本机可用
`playerctl` 验收。GitHub Actions 默认没有可用的用户 session bus，单元测试
不要依赖它。

### 2.2 规格里第一期该实现的面

`org.mpris.MediaPlayer2`：`Raise`（空）、`Quit`、`Identity`、`DesktopEntry`、
`CanQuit=true`、`CanRaise=false`、`HasTrackList=false`、`SupportedUriSchemes=[]`、
`SupportedMimeTypes=[]`。

`org.mpris.MediaPlayer2.Player`（第一期）：

| 方法 / 属性 | 建议 |
| --- | --- |
| Next / Previous / Pause / PlayPause / Stop / Play | 做 |
| Seek / SetPosition | 第二期；第一期 `CanSeek=false` 或只把 Seek 映射成 ±10s |
| OpenUri | 不做 |
| PlaybackStatus | Playing / Paused / Stopped |
| Metadata | `xesam:title/artist/album`、`mpris:length`、`mpris:trackid`、`xesam:url`（`file://`） |
| Volume | 第二期也可顺手做，映射成本地 0–100 |
| Position | 只读，微秒；客户端自己轮询，不必每 tick 推 PropertiesChanged |
| Rate / Min / Max | 固定 1.0 |
| LoopStatus / Shuffle | 第二期 |
| CanGoNext / CanGoPrevious / CanPlay / CanPause / CanControl | 按状态填 |
| Seeked 信号 | 有绝对 seek 后再发 |

`TrackList` / `Playlists` 两套可选接口：规格完整，但对“被识别 + 媒体键”无帮助，
第一期明确不做。

### 2.3 crate 对比

| | `mpris-server` 0.10.0（2026-04） | `souvlaki` 0.8.3（2025-06） |
| --- | --- | --- |
| 角色 | 只做 MPRIS **服务端** | Linux+Windows+macOS 薄封装 |
| 协议完整度 | Root + Player + 可选 TrackList/Playlists | 常用方法；无 LoopStatus/Shuffle/Can* 细控 |
| 传输 | zbus（纯 Rust） | 默认 `dbus`+`dbus-crossroads`（要 `dbus-devel`）；可选旧 zbus 3.9 |
| 许可证 | MPL-2.0（仓库已有该许可证文本） | MIT |
| 运行时 | 默认同 zbus（async-io）；可选 tokio | zbus 模式自己起 pollster 线程 |
| 与 `App` 的贴合 | `LocalServer`：实现类型**不必** `Send` | 回调必须 `Send + 'static` |

项目现在**没有** tokio。`mpris-server` 可不引入 tokio：独立线程里跑 zbus/async-io，
方法体只往 `mpsc` 丢命令，立刻返回。`Position` 用 `Arc<Mutex<Snapshot>>` 或原子
读取主线程写好的快照，避免回调里碰 `Player`。

`souvlaki` 的 Linux 默认后端会给 RPM 增加 `dbus-devel` / `dbus-libs`，和
v1.1.0“运行时只留 ALSA + SQLite + glibc”的方向相反。若坚持用它，至少要
`default-features = false, features = ["use_zbus"]`，但仍是一份过时的 zbus 3。

**Linux 推荐：`mpris-server` 0.10，不用 tokio feature。**

---

## 3. Windows：SMTC

### 3.1 系统实际认什么

Win10/11 的“正在播放”来自 **System Media Transport Controls**，不是 MPRIS。
出现位置：

- 锁屏 / `Win+A` 媒体控件 / 任务栏缩略图
- 键盘媒体键、部分蓝牙耳机
- 音量合成器里的应用会话名（元数据侧）

UWP/`Windows.Media.Playback.MediaPlayer` 会自动挂 SMTC。本程序用 Rodio/CPAL，
**不会**自动出现，必须手动注册。不要为此换回 MediaPlayer 播放栈。

Win32 正确入口（souvlaki 源码也是这条）：

```text
ISystemMediaTransportControlsInterop::GetForWindow(HWND)
  → SystemMediaTransportControls
  → DisplayUpdater（Music：Title / Artist / AlbumTitle / Thumbnail）
  → TimelineProperties（Start/End/Position/MinSeek/MaxSeek）
  → ButtonPressed + PlaybackPositionChangeRequested
```

没有 HWND 就注册不上。这是 Windows 侧唯一硬约束。

### 3.2 TUI 没有窗口怎么办

| 方案 | 评价 |
| --- | --- |
| `GetConsoleWindow()` | 经典 conhost 可用。Windows Terminal / ConPTY 下文档写明返回的是“仅供消息队列”的隐藏 HWND，**有可能**仍能 `GetForWindow`，但不能当唯一策略 |
| 消息专用窗口 `HWND_MESSAGE` | 控制台程序的常规做法：`CreateWindowEx` 父窗口 `HWND_MESSAGE`，再把该 HWND 交给 SMTC。不抢焦点、不出现任务栏窗体 |
| 上 winit 开隐形窗 | 过重，不采用 |

**推荐：自己建消息专用 HWND；`GetConsoleWindow()` 只作回退。**
SMTC 按钮是 WinRT `TypedEventHandler`，不依赖我们泵 `WM_*`。窗口线程按 STA
创建、保持窗口存活即可。GitHub Actions 的 Windows runner 无交互会话时
`GetForWindow` 可能失败——必须降级，测试不调用。

### 3.3 SMTC 能力对照

| SMTC | 对应本程序 | 第一期 |
| --- | --- | --- |
| Play / Pause / Stop / Next / Previous | App 已有或易补 | 做 |
| FastForward / Rewind | 映射 `seek_relative(±10)` | 可做 |
| `PlaybackPositionChangeRequested` | 需要绝对 seek | 第二期 |
| Timeline 进度 | `position` + `duration` | 第一期可每秒或状态变化时更新，避免 20ms 狂刷 COM |
| Volume | SMTC **没有**应用音量属性 | 不做（系统总音量是另一回事） |
| Thumbnail | 需文件或内存流 | 第一期不做 |
| Shuffle / Repeat | 部分客户端有扩展按钮，不是核心 SMTC | 第二期可选 |

`smtc-suite` 是**读**系统里别人的会话，方向反了，不要用。

**Windows 推荐：`windows` crate 直接调 SMTC，不要经 souvlaki。**
运行时无额外 DLL（Win10 1607+ 自带 WinRT）。许可证 MIT OR Apache-2.0，与现有
`windows` 生态一致。功能面勾选 `Media`、`Foundation`、`Win32_Foundation`、
`Win32_System_WinRT`、`Win32_UI_WindowsAndMessaging`，避免把整个 Win32 编进来。

### 3.4 为何不把 souvlaki 当跨平台主方案

它能减少样板，但落到本仓库会被这些点卡住：

1. Windows 仍要我们提供 HWND，省不掉最难的那步。
2. 依赖 `windows 0.44`（当前生态已远新于它）。
3. MPRIS 子集不够第二期（LoopStatus / Shuffle / Can* / Seeked）。
4. 默认 Linux 后端引入 libdbus，和现有“无额外系统多媒体库”的包装策略冲突。

若只想最快出第一期 Demo，souvlaki 可以当 PoC；正式依赖不建议。

---

## 4. 建议架构（两端共用）

与 `LibraryWorker` 同构，**不要**把 D-Bus/COM 放进 `Player`。

```text
媒体键 / playerctl / GNOME
        │
   [MediaSession 线程]
     Linux: mpris-server LocalServer
     Windows: 隐藏 HWND + SMTC
        │  mpsc::Sender<MediaCommand>
        ▼
   main.rs 循环  →  App::on_tick  drain
        │  执行现有 play_next / toggle / stop …
        ▼
   App 写 MediaSnapshot（状态、曲目、音量、位置）
        │  锁或 watch
        ▼
   MediaSession 推 PropertiesChanged / set_playback / set_metadata
```

```text
MediaCommand:
  Play, Pause, Toggle, Stop, Next, Previous,
  SeekRel(i64), SeekTo(Duration), SetVolume(u8),
  SetLoop(...), SetShuffle(bool), Quit
  // Raise / OpenUri：忽略
```

`App` 侧需要把现在的 `play_next` / `play_previous` / `change_volume` 等从
`handle_key` 抽成内部方法（本来就是 `fn`，只是缺 Stop / 分立 Pause / Play-when-stopped）。
`handle_key` 与媒体命令走同一条函数，避免两套切歌逻辑。

`Player` 第一期尽量不动。第二期只加：

```text
pub fn pause(&mut self)      // Playing → Paused，其余 no-op
pub fn resume(&mut self)     // Paused → Playing，其余 no-op
pub fn seek_to(&self, pos: Duration)  // 复用 clamp_seek_target
```

`Play` 在 `Stopped`：由 App 播 `playing_index` 或当前选中，而不是叫 `toggle_pause`。

启动时机：`App::new` 成功之后、进 alternate screen 之前或之后均可；失败只记一条
`message` 警告。`App::new_for_tests` / `Player::new_for_tests` 不创建会话。

配置：默认可开；可选 `media_session = false` 留给 SSH / 无总线环境，非必须。

---

## 5. 必须先拍板的产品语义

这些不是技术障碍，但会写进行为，不宜实现时再改。

### 5.1 Previous

现在 `p` = 弹出播放历史。MPRIS/SMTC 的 Previous 在多数播放器里是“列表上一首”，
且常见附加规则：进度 > 3s 则先回到 0。

建议第一期：**系统 Previous = 与键盘 `p` 相同（历史）**，避免两套上一首。
第二期若要“列表上一首”，再单独做，不要悄悄改键盘 `p`。

### 5.2 PlayMode ↔ MPRIS

| 本程序 | LoopStatus | Shuffle |
| --- | --- | --- |
| Sequential | None | false |
| RepeatAll | Playlist | false |
| RepeatOne | Track | false |
| Shuffle | None 或 Playlist | true |

系统端把 Shuffle 和 Loop 拆开写时，无法无损表示“随机 + 列表循环”。
建议：Shuffle=true 时本地进入 `Shuffle`；LoopStatus 在 Shuffle 下只作展示，
或规定 Shuffle 覆盖 Loop。第一期属性只读、不可写更干净。

### 5.3 封面

Lofty 能读内嵌图，库扫描现在不存。要封面需要：扫库时抽图，或切歌时写临时
`file://` / SMTC 流。磁盘、缓存失效、扫描变慢都是新问题。第一期用应用图标
或空封面即可。

### 5.4 多实例

MPRIS 总线名同一 identity 只能有一个主名。第二实例应加后缀
（`org.mpris.MediaPlayer2.music_player.instance<pid>`）或直接放弃注册。
建议失败降级，不抢第一实例。

---

## 6. 包装、许可证、CI

### Linux RPM

- zbus / mpris-server：**无新的 BuildRequires / Requires**。session D-Bus 是桌面机已有运行时，不必写进 spec。
- 若误选 souvlaki 默认后端：需要 `pkgconfig(dbus-1)`，运行时 `dbus-libs`。不推荐。
- `.desktop` 已是 Player；Identity 与 `music-player.desktop` 文件名对齐即可。

### Windows

- 无新 DLL、不改 Inno 捆绑清单。
- Win10 1607+ / Win11 均有 SMTC。

### 许可证

| crate | 许可 | 现有合规是否够 |
| --- | --- | --- |
| mpris-server + 其 zbus 树 | MPL-2.0（crate 本身）+ zbus MIT | 已有 `MPL-2.0.txt`；需把 crate 名补进 `THIRD-PARTY-NOTICES.txt` |
| windows（SMTC 功能） | MIT OR Apache-2.0 | 与现有 Apache 文本兼容；按实际启用的 crate 补名单 |
| souvlaki（若 PoC） | MIT | 无新 copyleft |

不必改 Fedora `License:` 字段的大类，除非引入新的 copyleft 家族。

### CI

- Linux Test：不连 bus，现有 57+12 应保持绿。
- 可选 job：`dbus-run-session playerctl` 冒烟，非必须。
- Windows Test：不创建 SMTC。无卡、无桌面不能当失败。

### 体积

zbus 会增大 Linux 二进制（纯 Rust D-Bus 栈）。`windows` crate 按 feature 裁剪，
增量通常小于“勾选整个 Win32”。正式做时在容器里量一次 `strip` 后体积，再写进 PR。

---

## 7. 风险与非目标

| 风险 | 等级 | 处理 |
| --- | --- | --- |
| 无 session bus / 无 HWND | 中 | 启动失败降级，与无声卡策略一致 |
| Windows Terminal 下 HWND 行为 | 中 | 消息专用窗口为主，真机验收 |
| 后台线程碰 `Player` | 高 | 只许命令队列；快照只读 |
| Position 每 20ms 推 D-Bus/COM | 中 | 状态变化才 PropertiesChanged；进度最多 1Hz |
| Previous / PlayMode 语义 | 中 | §5 先定，再写代码 |
| 引入 tokio | 低 | 第一期不要 |
| 封面、OpenUri、TrackList | — | 非目标 |
| 改 Rodio / 频谱 / SQLite | — | 非目标 |
| 读其他应用的正在播放 | — | 非目标 |

---

## 8. 建议落地顺序（尚未开工）

只在你拍板后做。推荐顺序：

1. 抽出 `MediaCommand` + `MediaSnapshot`，`on_tick` drain；键盘与会话共用
   `play_or_resume` / `pause` / `stop_playback` / `play_next` / `play_previous`。
2. Linux：`mpris-server` LocalServer，第一期方法 + Metadata + PlaybackStatus。
   容器内 `playerctl status` / `play-pause` / `next` 验收。
3. Windows：隐藏 HWND + SMTC，同一套命令。真机看锁屏/媒体键（你这边 Win 构建已通）。
4. 测试：命令映射单测；`new_for_tests` 断言不注册会话。
5. 文档 / `.desktop` Identity / THIRD-PARTY-NOTICES；changelog 作小功能版本。
6. （可选第二期）`seek_to`、Volume、LoopStatus、Seeked、封面。

---

## 9. 决策清单

请确认后再实现：

1. **方案**：Linux `mpris-server` + Windows 自研 SMTC（推荐） / 先只做 Linux / 用 souvlaki 出 PoC。
2. **第一期范围**：识别 + Play/Pause/Stop/Next/Previous + 元数据（推荐） / 一次做满 seek/音量/模式。
3. **Previous**：与键盘 `p` 同为历史（推荐） / 改成曲库上一首。
4. **PlayMode**：第一期只读展示 / 允许系统改循环与随机。
5. **封面**：第一期不做（推荐）。
