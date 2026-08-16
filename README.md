# Music Player

Music Player 是一个用 Rust 编写的终端音乐库播放器。它使用 Rodio 播放、Lofty 读取媒体信息，用 SQLite 保存可随时重建的增量索引，并把配置与播放列表保存在平台标准用户目录中。

## 功能

- 后台递归扫描音乐库，界面不会因大型目录而停止响应
- 展示标题、歌手、专辑、格式、时长和实时播放进度
- 播放、平滑暂停、前后跳转、音量和静音
- 内置实时音频频谱，将 50–8000 Hz 对数映射为最多 32 根柱，并支持记忆开关状态
- 顺序、列表循环、单曲循环和随机四种播放模式
- Unicode 友好的实时模糊搜索，覆盖标题、歌手、专辑和相对路径
- 临时播放队列与持久命名播放列表
- 播放列表中的失效歌曲会被标记并跳过；删除列表绝不删除音乐文件

默认界面继承终端自身的背景，不绘制应用专属的全局背景。主要文字使用柔和白色，次要信息和边框采用分层灰白，音频频谱使用统一的灰白渐变。

## 安装与运行要求

项目正式支持 Fedora Linux 与 Windows 11 x86_64：

- Fedora RPM 使用系统的 ALSA/PipeWire 与 SQLite
- Windows Installer 与 Portable ZIP 把 SQLite 编入程序，通过 WASAPI 输出，用户无需另行安装解码运行时

Windows Installer 默认安装到当前用户目录，不要求管理员权限，并创建开始菜单入口。Portable ZIP 解压后运行 `music-player.cmd`。两种 Windows 发行包都可在 Windows Terminal 中使用。

第一版 Windows 包尚未进行 Authenticode 签名，Windows Defender SmartScreen 可能显示未知发布者提示。请只从项目 GitHub Release 下载，并使用同一 Release 中的 `SHA256SUMS.txt` 核对文件。

Fedora 44 的 RPM 会自动声明运行依赖。本机源码构建必须在项目配置的
`dev-fedora` 容器中进行，容器内需要：

```sh
sudo dnf install \
  rust cargo \
  alsa-lib-devel \
  sqlite-devel
```

## 使用

```sh
# 首次运行默认使用系统的 Music 目录
cargo run --locked

# 只在本次运行打开另一个目录
cargo run --locked -- /path/to/music

# 永久设置主音乐库
cargo run --locked -- --set-library /path/to/music
```

查看完整命令行帮助：

```sh
music-player --help
```

通过 RPM 安装后，也可以在桌面应用菜单中搜索“Music Player”或“音乐播放器”。菜单项会使用桌面环境配置的默认终端启动程序，不要求安装某个特定的终端模拟器。

扫描不会跟随音乐库目录内部的符号链接：目录软链接会被跳过以避免循环，指向音频文件的软链接目前也不会被收录。如果音乐存放在其他位置，请把真实文件或硬链接放进音乐库目录。

## 按键

| 按键 | 操作 |
|---|---|
| `↑/↓`、`j/k` | 选择歌曲 |
| `Enter` | 立即播放选中歌曲，保留队列 |
| `Space` | 暂停/继续 |
| `←/→`、`h/l` | 后退/前进 10 秒 |
| `-`、`=` | 音量降低/提高 5% |
| `m` | 静音 |
| `n/p` | 下一首/上一首历史 |
| `z` | 切换播放模式 |
| `v` | 显示/隐藏音频频谱 |
| `/` | 实时模糊搜索 |
| `r` | 后台重新扫描 |
| `a/A` | 加到队尾/设为下一首 |
| `P` | 播放列表面板 |
| `?` | 内置完整帮助 |
| `q` | 退出 |

播放列表面板中，`c` 新建、`a` 加入当前选中歌曲、`Enter` 查看内容、`x` 确认删除。列表内容中可按 `Enter` 从选中项开始播放，按 `d` 只从列表移除该项。

## 用户数据

程序不会在音乐库目录中写入文件：

| 数据 | Linux | Windows |
|---|---|---|
| 配置 | `$XDG_CONFIG_HOME/tui-music-player/config.toml` | `%APPDATA%\tui-music-player\config.toml` |
| 播放列表 | `$XDG_DATA_HOME/tui-music-player/playlists/*.json` | `%APPDATA%\tui-music-player\playlists\*.json` |
| 可删除缓存 | `$XDG_CACHE_HOME/tui-music-player/library.sqlite3` | `%LOCALAPPDATA%\tui-music-player\library.sqlite3` |

Linux 没有显式设置 XDG 基础目录时，通常对应 `~/.config`、`~/.local/share` 和 `~/.cache`。Windows Installer 和 Portable ZIP 使用相同的 AppData 目录；升级、删除 Portable 文件或卸载程序都不会删除这些用户数据。

频谱默认开启，退出程序时会把 `visualizer_enabled` 与音量、静音等设置一起保存。频谱从当前播放的 PCM 分析，不会采集麦克风或其他应用的声音。

支持的音频格式：MP3、FLAC、WAV、OGG/OGA Vorbis、M4A/AAC、AAC ADTS、AIFF。Opus 暂缓支持；APE 与 WMA 不再支持。

播放区域的图标表示按下 `Space` 后将执行的操作：播放中显示文本样式的 `⏸︎`，暂停时显示文本样式的 `▶︎`。

## 测试、RPM 与 Windows 发行包

构建 RPM 还需要 Fedora 的打包工具和桌面入口检查工具：

```sh
sudo dnf install rpm-build desktop-file-utils
```

以下命令必须在 `dev-fedora` 容器中运行：

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --locked

./packaging/build-rpm.sh
sudo dnf install ./packaging/rpmbuild/RPMS/x86_64/music-player-1.0.2-1.fc44.x86_64.rpm
```

RPM 构建脚本会生成离线 vendor 归档、二进制 RPM 和 SRPM。安装后的 RPM 同时提供命令行程序、man 手册、桌面菜单入口、可缩放 SVG 图标和 48 px 兼容图标。升级使用 `sudo dnf upgrade <RPM 路径>`，卸载使用 `sudo dnf remove music-player`。

Windows MSVC 构建、测试和打包由 GitHub Actions 的 Windows runner 完成，生成：

- `music-player-<版本>-windows-x86_64-setup.exe`
- `music-player-<版本>-windows-x86_64-portable.zip`

Installer 的“添加到当前用户 PATH”选项默认不勾选。启用后可以在新打开的终端中直接运行 `music-player`，卸载时只移除应用自己的 PATH 条目。

## 许可证

版权属名：HZ-TYZQ。项目采用 MIT 许可证，详见 `LICENSE`。
