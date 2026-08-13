# Music Player

Music Player 是一个用 Rust 编写的终端音乐播放器。它递归扫描指定目录中的音频文件，在终端界面中提供浏览、播放、暂停、停止和自动连播功能。

## 运行要求

- Linux 终端
- `/usr/bin/ffplay`（程序实际播放音频所必需）

在 Fedora 44 上，`ffplay` 通常由 RPM Fusion 仓库的 FFmpeg 软件包提供。

## 从源码运行

```sh
cargo run --locked -- /path/to/music
```

省略目录时，程序扫描当前工作目录。支持 `mp3`、`flac`、`wav`、`ogg`、`opus`、`m4a` 和 `aac` 文件，最多递归扫描八层子目录。

## 按键

| 按键 | 操作 |
|---|---|
| `↑` / `↓` 或 `k` / `j` | 选择曲目 |
| `Enter` | 播放选中曲目 |
| `Space` | 暂停或继续 |
| `s` | 停止 |
| `n` / `p` | 下一曲 / 上一曲 |
| `q` 或 `Esc` | 退出 |

## 构建和安装 RPM

在已安装 Fedora RPM 构建工具和 Rust RPM 构建依赖的环境中运行：

```sh
./packaging/build-rpm.sh
sudo dnf install ./packaging/rpmbuild/RPMS/x86_64/music-player-0.1.0-1.fc44.x86_64.rpm
```

升级时再次运行 `sudo dnf upgrade <RPM 路径>`；卸载使用 `sudo dnf remove music-player`。RPM 明确依赖 `/usr/bin/ffplay`，并同时生成可供重新构建的 SRPM。

## 许可证

本项目采用 MIT 许可证，详见 `LICENSE`。
