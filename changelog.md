# Changelog

本项目所有新增的依赖/库都会记录在此文件中。

## [Unreleased]

## [1.2.1] - 2026-08-20

### 修复

- 顺序播放遇到无法加载的曲目时继续尝试后续歌曲，不再重复尝试同一个坏文件
- 只在新曲目加载成功后更新播放历史和频谱适应状态
- Linux MPRIS 文件 URI 正确转义空格、Unicode、保留字符与非 UTF-8 路径字节
- Windows SMTC 在新曲目缺失歌手或专辑时清除上一首的字段

### 结构

- 按输入、播放、媒体控制和播放列表职责拆分 `app` 模块
- 按曲库、当前播放、频谱、弹层和文本工具拆分 `ui` 模块
- 保持 `App` 为单一业务状态所有者，不改变快捷键、布局或存储格式

### 依赖

- Linux 新增 `percent-encoding` 2.3，仅用于生成规范的 MPRIS 本地文件 URI

## [1.2.0] - 2026-08-18

### 播放

- 播放模式拆成循环（顺序 / 列表循环 / 单曲循环）和独立的随机开关
- 随机改为整轮 permutation，每轮重洗；队列插播不消耗随机袋
- 暂停切歌保持暂停，并从 0 装上新曲，不先出声再暂停
- `z` 只切换循环，`s` 开关随机

### 系统媒体控制

- Linux 通过 MPRIS 2 对接媒体键、`playerctl` 和桌面控件
- Windows 通过隐藏顶层窗口注册 SMTC，对接锁屏 / 媒体键 / 进度条
- 支持播放、暂停、上一首/下一首（上一首仍走播放历史）、精确 Seek、循环和随机
- Linux 可写应用音量；非零音量取消静音。Windows SMTC 不控制应用音量
- 系统 Stop：Windows 不显示该按钮；Linux 按 Pause 处理并保留进度
- 媒体会话注册失败不影响播放

### 依赖

- Linux：`mpris-server` 0.10、`pollster` 0.4、`async-io` 2
- Windows：`windows` 0.62.2（Media / WinRT / 建窗所需 Gdi）

## [1.1.0] - 2026-08-17

### 播放与音乐库

- 播放后端改为 Rodio 0.22（内置 Symphonia 0.5.5）+ CPAL，不再使用 GStreamer
- 媒体信息与时长改为 Lofty 0.25.1 读取；SQLite 缓存 schema 不变
- 频谱改为非阻塞 PCM tap + rustfft，不再依赖 GStreamer `spectrum`
- 支持 MP3、FLAC、WAV、OGG/OGA Vorbis、M4A/AAC、AAC ADTS、AIFF
- Opus 暂缓支持；APE、WMA 从支持列表移除，播放时明确拒绝

### 界面

- 播放/暂停指示器改为固定宽度的 ASCII 符号，避免 Windows 终端把 Unicode 图标画成 emoji
- 播放进度改为 ASCII 进度条
- 曲目列表按终端宽度自适应列，并用显示宽度截断
- 频谱处理改为 CAVA 风格的 attack / gravity；可视化范围 50–5000 Hz，并去掉频谱标题文字

### 构建与发行

- Linux CI / RPM 构建依赖改为 ALSA 与 SQLite，不再安装 GStreamer
- Windows 发行包不再附带 GStreamer runtime，也不再设置 GST 环境变量
- 发行材料附带 Apache-2.0 / MPL-2.0 全文，并列出 cpal、Symphonia、nucleo 等第三方许可证
- CI 与 Release 改为从官方 GitHub release（jrsoftware/issrc is-6_7_1）下载 Inno Setup 6.7.1 并校验 SHA-256，替换 chocolatey 安装，消除 runner 预装版本漂移
- 单元测试改用无声卡的 headless Player，避免 CI runner 因打不开默认音频设备失败

### 依赖

- 新增 `rodio` 0.22.2（`symphonia-aiff`）、`lofty` 0.25.1、`rustfft` 6.4
- 移除 `gstreamer`、`gstreamer-play`、`gstreamer-pbutils`

## [1.0.2] - 2026-08-15

### Windows

- 新增 Windows 11 x86_64 MSVC 编译和测试工作流
- 新增携带完整官方 GStreamer runtime 的 Inno Setup Installer 与 Portable ZIP
- Windows 使用内置 SQLite，不要求用户安装 GStreamer 或 SQLite
- Installer 默认当前用户安装，并提供默认关闭的用户 PATH 选项
- Portable ZIP 免安装，但配置、播放列表和缓存继续使用 Windows 标准 AppData 目录
- Windows 发行包暂不进行 Authenticode 签名，Release 提供统一 SHA-256 校验文件
- 包内附带 GStreamer 1.28.6 官方许可文本、LGPL 2.1 全文及 HZ-TYZQ 提供对应源码的书面承诺（至少三年）
- 打包改为校验并复制仓库内的许可资料；官方 GStreamer 安装器本身不会把许可文件装进 runtime 目录
- 安装器简体中文界面使用随仓库分发的 Inno Setup 非官方翻译（issrc is-6_7_1 精确匹配），不再依赖 choco Inno Setup 不自带的语言文件
- 打包时清理并断言 stage 中不存在 GStreamer 安装器残留的 unins*.exe/.dat/.msg，避免与应用自身的卸载数据冲突

### 构建与发行

- 普通 CI（master 推送与 PR）即编译 Installer 与 Portable ZIP 并验证安装/卸载，不再只在 tag 构建时执行

- Release 改为 Fedora RPM 与 Windows 包全部构建成功后统一发布
- GitHub Actions 更新为 Node.js 24 兼容版本
- 新增 Windows Unicode 路径、配置覆盖和安装器 PATH 行为验证

### 依赖

- `rusqlite` 启用 `bundled-windows` 特性；只在 Windows 内置 SQLite，Linux 仍使用系统 SQLite
- Windows 打包使用 Inno Setup 6.7.1；它只用于 CI 生成安装程序，不是应用运行依赖
- Windows 私有 runtime 固定为 GStreamer 1.28.6 MSVC x86_64

## [1.0.1] - 2026-08-15

### 修复

- 播放发生异步错误时尝试前进到下一首，减少单曲循环反复重试同一坏文件的情况
- 后台重新扫描完成后按路径恢复选中的歌曲位置，不再重置到列表顶部

### 文档

- README 说明扫描不跟随符号链接的限制

### 界面

- 默认主题保留终端背景，主要文字改用固定柔和白色，普通边框和次要信息使用分层灰白
- 保留选中行的局部深灰背景，移除常规界面的青、洋红、黄、绿装饰色
- 音频频谱改为灰白渐变，使整个默认界面保持统一的无彩色体系
- 新增集中式主题结构，当前只提供一个默认主题，不增加配置项
- 强制播放和暂停操作图标使用文本字形，避免终端把暂停符号渲染成带底框的 emoji

### 桌面集成

- 新增 `Music Player` 桌面菜单入口，简体中文环境显示“音乐播放器”
- 使用桌面环境的默认终端启动程序，不绑定 GNOME Terminal 或 Konsole
- 新增由 HZ-TYZQ 原稿整理的正式 SVG 图标和 48 px 兼容图标
- RPM 安装桌面入口和 hicolor 图标，并在构建时用 `desktop-file-validate` 检查入口规范

### 依赖

- 没有新增 Rust crate 或 RPM 运行依赖
- RPM 构建依赖新增 `desktop-file-utils`，只用于验证桌面入口

## [1.0.0] - 2026-08-14

### 正式版

- 发布首个稳定版本，整合音乐库扫描、增量索引、播放控制、搜索、队列、播放列表和实时音频频谱
- 修复 Nucleo 一次性 `changed` 事件在重新解析查询时被消费后未同步 snapshot 的问题
- 曲库重建期间立即清除旧搜索索引，避免旧结果指向新曲库中的错误歌曲
- Fedora RPM 版本更新为 `1.0.0-1`

### 依赖

- 没有新增 Rust crate 或 RPM 运行依赖

## [0.3.0] - 2026-08-14

### 功能

- 新增基于 GStreamer `spectrum` 的实时音频频谱，不采集其他应用声音
- 在曲目列表下方将 50–8000 Hz 对数映射为最多 32 根带间隔的柱状图
- 新增 `v` 快捷键切换频谱，并在 XDG 配置中记住开关状态
- 暂停、停止和切歌时平滑清理频谱，窄终端下自动缩小或暂时隐藏
- 频谱改用频段中心插值并为低频保留显示余量，减少左侧柱子成片顶格
- 播放图标改为操作语义：播放中显示暂停按钮，暂停时显示继续按钮

### 依赖

- 没有新增 Rust crate 或 RPM 运行依赖；`spectrum` 由已有的 `gstreamer1-plugins-good` 提供

## [0.2.0] - 2026-08-14

### 新增依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| clap | 4.6.6 | 命令行参数、帮助和版本信息 |
| dirs | 6.0.0 | XDG Music 与用户基础目录解析 |
| gstreamer | 0.25.3 | GStreamer 核心类型与初始化 |
| gstreamer-play | 0.25.0 | 播放、暂停、跳转、音量和异步事件 |
| gstreamer-pbutils | 0.25.2 | 音频元数据与时长发现 |
| rusqlite | 0.40.2 | 系统 SQLite 增量音乐索引 |
| serde | 1.0.229 | 配置和播放列表数据模型 |
| serde_json | 1.0.151 | 独立 JSON 播放列表文件 |
| toml | 1.1.4 | TOML 用户配置 |
| nucleo | 0.5.0 | 后台 Unicode 模糊搜索 |
| tempfile（开发依赖） | 3.27.0 | 隔离配置、缓存和播放测试 |

### 功能

- 使用 GStreamer 取代 ffplay 子进程，并支持进度、跳转、音量、静音和播放事件
- 默认采用 XDG Music 目录，支持会话目录覆盖和 `--set-library` 持久设置
- 新增后台曲库扫描、SQLite 增量元数据缓存和扫描状态
- 新增四种播放模式、临时队列、实际播放历史与坏文件跳过
- 新增实时模糊搜索和独立 JSON 命名播放列表
- 新增完整快捷键帮助、窄终端提示、GitHub CI 与标签发布工作流
- RPM 改为依赖 GStreamer 插件和系统 SQLite，不再依赖 ffplay

### 移除依赖

- 移除 `libc`，不再使用 Unix 信号控制 ffplay 子进程

## [0.1.0-1 RPM] - 2026-08-14

### 打包

- 新增 Fedora 44 x86_64 RPM 打包支持，包括离线依赖归档、RPM spec 和 man 手册

## [0.1.0] - 2025-08-14

### 新增依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| ratatui | 0.30.2 | TUI 界面渲染框架 |
| crossterm | 0.29.0 | 跨平台终端后端（事件输入 / 终端控制） |
| libc | 0.2.189 | 向 ffplay 子进程发送 SIGSTOP / SIGCONT 信号以实现暂停/恢复 |

### 功能

- 初始版本：扫描目录音频文件、列表浏览、播放 / 暂停 / 停止 / 上下曲切换、自动连播
- 播放后端：系统 `ffplay` 子进程（`-nodisp -autoexit`）
