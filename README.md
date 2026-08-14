# Music Player

Music Player 是一个用 Rust 编写的终端音乐库播放器。它使用 GStreamer 播放和读取媒体信息，用 SQLite 保存可随时重建的增量索引，并把配置与播放列表保存在标准 XDG 用户目录中。

## 功能

- 后台递归扫描音乐库，界面不会因大型目录而停止响应
- 展示标题、歌手、专辑、格式、时长和实时播放进度
- 播放、平滑暂停、前后跳转、音量和静音
- 内置实时音频频谱，将 50–8000 Hz 对数映射为最多 32 根柱，并支持记忆开关状态
- 顺序、列表循环、单曲循环和随机四种播放模式
- Unicode 友好的实时模糊搜索，覆盖标题、歌手、专辑和相对路径
- 临时播放队列与持久命名播放列表
- 播放列表中的失效歌曲会被标记并跳过；删除列表绝不删除音乐文件

默认界面继承终端自身的背景和主要前景色，不绘制应用专属的全局背景。普通信息保持灰阶，只有音频频谱持续使用低饱和度的青色到洋红色渐变。

## 运行要求

- Linux 终端
- GStreamer 1.x 及常用音频解码插件
- SQLite 3

Fedora 44 的 RPM 会自动声明所需运行时插件。源码构建还需要：

```sh
sudo dnf install \
  rust cargo \
  gstreamer1-devel \
  gstreamer1-plugins-base-devel \
  gstreamer1-plugins-bad-free-devel \
  sqlite-devel
```

## 使用

```sh
# 首次运行默认使用系统的 XDG Music 目录
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

- 配置：`$XDG_CONFIG_HOME/tui-music-player/config.toml`
- 播放列表：`$XDG_DATA_HOME/tui-music-player/playlists/*.json`
- 可删除缓存：`$XDG_CACHE_HOME/tui-music-player/library.sqlite3`

没有显式设置 XDG 基础目录时，通常对应 `~/.config`、`~/.local/share` 和 `~/.cache`。

频谱默认开启，退出程序时会把 `visualizer_enabled` 与音量、静音等设置一起保存。频谱由 GStreamer `spectrum` 直接分析当前歌曲，不会采集麦克风或其他应用的声音。

播放区域的图标表示按下 `Space` 后将执行的操作：播放中显示 `⏸`，暂停时显示 `▶`。

## 测试与 RPM

构建 RPM 还需要 Fedora 的打包工具和桌面入口检查工具：

```sh
sudo dnf install rpm-build desktop-file-utils
```

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --locked

./packaging/build-rpm.sh
sudo dnf install ./packaging/rpmbuild/RPMS/x86_64/music-player-1.0.0-1.fc44.x86_64.rpm
```

RPM 构建脚本会生成离线 vendor 归档、二进制 RPM 和 SRPM。安装后的 RPM 同时提供命令行程序、man 手册、桌面菜单入口、可缩放 SVG 图标和 48 px 兼容图标。升级使用 `sudo dnf upgrade <RPM 路径>`，卸载使用 `sudo dnf remove music-player`。

## 许可证

版权属名：HZ-TYZQ。项目采用 MIT 许可证，详见 `LICENSE`。
