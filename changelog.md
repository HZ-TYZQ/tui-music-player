# Changelog

本项目所有新增的依赖/库都会记录在此文件中。

## [Unreleased]

### Windows

- 新增 Windows 11 x86_64 MSVC 编译和测试工作流
- 新增携带完整官方 GStreamer runtime 的 Inno Setup Installer 与 Portable ZIP
- Windows 使用内置 SQLite，不要求用户安装 GStreamer 或 SQLite
- Installer 默认当前用户安装，并提供默认关闭的用户 PATH 选项
- Portable ZIP 免安装，但配置、播放列表和缓存继续使用 Windows 标准 AppData 目录
- Windows 发行包暂不进行 Authenticode 签名，Release 提供统一 SHA-256 校验文件

### 构建与发行

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
