# MusicPlayer RPM 打包实施计划

状态：已完成
目标版本：`music-player 0.1.0`
目标平台：Fedora 44 x86_64

## 1. 目标

把当前 Rust 终端音乐播放器制作成一个能够由 Fedora 包管理器安装、查询和卸载的 RPM 包。

首版完成后应当满足：

- 生成可安装的 `music-player-0.1.0-1.fc44.x86_64.rpm`。
- 程序安装到 `/usr/bin/music-player`。
- RPM 明确要求系统提供 `/usr/bin/ffplay`，避免安装成功后完全无法播放。
- RPM 包含许可证、基础说明和命令手册。
- 构建过程使用锁定的 Rust 依赖，并可在无网络的 RPM 构建阶段完成。
- 保留源代码 RPM（SRPM），方便以后重新构建和扩展到其他 Fedora 版本。

“SRPM”是 source RPM，即包含源码和打包规则、用于重新构建二进制 RPM 的包。

## 2. 已由用户决定的事项

1. 第一阶段只面向 Fedora 44 x86_64。
2. 项目采用 MIT 许可证。
3. 继续使用现有 `ffplay` 播放后端。
4. `/usr/bin/ffplay` 是 RPM 的必要运行依赖。
5. 计划文件保存在 `/home/tyzq/Projects/MusicPlayer/plan`。

## 3. 由助手补充的技术方案

### 3.1 使用原生 RPM spec

新增 `packaging/music-player.spec`。spec 是 RPM 的构建说明书，负责描述包名、版本、依赖、构建命令、安装位置和包含的文件。

不把 `cargo-rpm` 之类的第三方 Cargo 插件作为项目依赖。首版采用 Fedora 原生工具和 Rust RPM 构建宏，使打包过程更接近 Fedora 的常规维护方式。

### 3.2 离线构建 Rust 依赖

程序依赖 crates.io 上的 Rust 库。构建辅助脚本会依据 `Cargo.lock` 收集并固定这些依赖，制作只用于 RPM 构建的源码归档。RPM 的正式构建阶段使用 `--locked` 和 `--offline`：

- `--locked` 防止构建时悄悄改变依赖版本。
- `--offline` 防止构建时临时访问网络。

生成的依赖和 RPM 产物属于构建产物，不直接提交到源码目录的长期维护文件中。

### 3.3 RPM 内容

二进制 RPM 计划包含：

- `/usr/bin/music-player`
- `/usr/share/licenses/music-player/LICENSE`
- `/usr/share/doc/music-player/README.md`
- `/usr/share/doc/music-player/changelog.md`
- `/usr/share/man/man1/music-player.1.gz`（压缩由 RPM 自动处理）

RPM 运行依赖使用文件依赖 `Requires: /usr/bin/ffplay`。这样依赖描述的是程序真正需要的命令，而不是绑定到某个特定的 FFmpeg 包名。

### 3.4 版本管理

`Cargo.toml` 中的 `0.1.0` 作为项目版本来源。spec 仍需声明 RPM 版本，构建脚本会检查二者一致；不一致时立即停止并给出明确错误，避免生成名称和程序版本不一致的包。

## 4. 实施阶段

### 阶段一：整理不改变功能的项目基础

计划修改：

- 新增 `LICENSE`，写入标准 MIT 许可证文本。
- 新增 `README.md`，说明功能、运行方式、按键、支持格式、`ffplay` 依赖及 RPM 安装方式。
- 补全 `Cargo.toml` 中的 `description`、`license` 和 `readme` 元数据。
- 运行 `cargo fmt` 修复现有格式差异。
- 修复当前两个 Clippy 警告：简化自然播放结束判断，并为 `Player` 实现 `Default`。

这些代码整理不改变播放器的可观察行为。

### 阶段二：加入打包文件

计划新增：

- `packaging/music-player.spec`：RPM 构建规则。
- `packaging/build-rpm.sh`：可重复执行的本地构建入口。
- `packaging/music-player.1`：终端命令手册。

计划修改：

- `.gitignore`：忽略生成的源码归档、依赖归档和 RPM 构建目录。
- `changelog.md`：记录新增的打包工具链和 RPM 支持；不会把系统工具错误地登记成程序运行库。

构建脚本只操作项目内明确的构建输出目录，不清理用户目录，不覆盖源码文件。

### 阶段三：准备 Fedora 打包工具

先查询 Fedora 44 仓库中工具的准确包名和版本，再安装缺少的工具。预计需要：

- Fedora 提供的 Rust/Cargo 构建包或 Rust RPM 宏。
- `rpmlint`，用于检查 RPM 元数据和常见打包错误。
- 必要时使用 `mock` 做隔离构建；第一阶段不把它作为成功交付的硬性条件。

当前 Rust 工具来自用户环境，而不是 RPM 数据库。标准 `rpmbuild` 会根据 RPM 数据库检查 `BuildRequires`，所以不能仅靠当前 `rustup` 工具链假装系统构建依赖已满足。

安装系统软件会改变机器状态。实施到此阶段时，应先向用户展示准确的 DNF 命令并取得许可，然后执行；不使用 `--nodeps` 绕过依赖检查。

### 阶段四：构建二进制 RPM 和 SRPM

执行流程：

1. 检查源码版本、锁文件和必要工具。
2. 根据 `Cargo.lock` 准备离线依赖。
3. 生成 `music-player-0.1.0` 源码归档。
4. 在项目专用的 RPM 工作目录中运行 `rpmbuild -ba`。
5. 收集生成的 x86_64 RPM 和 SRPM，并打印它们的完整路径。

不会在这一阶段创建 Git 提交、推送远程仓库或发布到公共软件源。

### 阶段五：验证

源码级验证：

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features --locked -- -D warnings`
- `cargo test --locked`

RPM 级验证：

- `rpmlint` 检查 spec、SRPM 和二进制 RPM。
- `rpm -qpi` 检查名称、版本、架构、许可证和描述。
- `rpm -qpl` 检查包内文件及安装路径。
- `rpm -qpR` 确认存在 `/usr/bin/ffplay` 依赖。
- 检查 RPM 签名/摘要和文件校验信息；首版本地构建允许“未签名”，但必须明确报告。

安装验证：

- 经用户允许后，用 DNF 安装生成的本地 RPM，让 DNF 正常处理依赖。
- 确认 `/usr/bin/music-player` 来自该 RPM。
- 在伪终端或真实终端中启动已安装的程序，检查界面和正常退出。
- 验证卸载不会删除用户音乐，也不会遗留项目文件。

系统安装和卸载会在执行前再次明确告知用户。

## 5. 验收标准

只有同时满足下列条件才视为完成：

1. 源码测试、格式检查和严格 Clippy 检查全部通过。
2. `rpmbuild` 成功生成 x86_64 二进制 RPM 和 SRPM。
3. RPM 元数据声明 MIT 许可证和 `/usr/bin/ffplay` 依赖。
4. 包内文件路径符合计划，没有把 `target`、测试音乐或临时文件打入包中。
5. `rpmlint` 没有未解释的错误；无法消除的警告必须逐项记录原因。
6. 安装后的 `/usr/bin/music-player` 能启动并正常退出。
7. 最终交付说明包含重新构建、安装、升级和卸载命令。

## 6. 当前假设

- 第一版仍是终端程序，不新增桌面菜单项或图标。
- 命令名保持 `music-player`。
- RPM Release 从 `1` 开始。
- 包不会包含 `ffplay` 或 FFmpeg 本体，只声明外部依赖。
- 使用 RPM Fusion 是首版运行环境的一部分，因为当前 `/usr/bin/ffplay` 由其 `ffmpeg` 包提供。
- 不给 RPM 做 GPG 发布签名；本地未签名 RPM 可以安装，但不适合作为正式公共软件仓库的最终发布物。
- 当前仓库没有 Git 提交，本计划不会擅自创建初始提交。

## 7. 推迟到后续版本的决定

- 支持其他 Fedora 版本或其他 CPU 架构。
- 使用 `mock`/COPR 自动为多个 Fedora 版本构建。
- 提交到 Fedora 官方仓库。
- 替换 `ffplay` 播放后端。
- 添加桌面入口、应用图标或图形界面。
- RPM GPG 签名和公共仓库发布。
- 为命令增加正式的 `--help`、`--version` 和更完整的参数解析。

这些项目不会被第一阶段“顺便”实现，避免扩大范围。

## 8. 预计改动文件

| 文件 | 处理方式 | 用途 |
|---|---|---|
| `LICENSE` | 新增 | MIT 许可证 |
| `README.md` | 新增 | 使用和安装说明 |
| `Cargo.toml` | 修改 | 项目包元数据 |
| `src/app.rs` | 小幅修改 | 消除 Clippy 警告，不改变行为 |
| `src/player.rs` | 小幅修改 | 格式整理和 `Default` 实现 |
| `src/main.rs` | 格式整理 | 通过 rustfmt |
| `.gitignore` | 修改 | 忽略 RPM 构建产物 |
| `changelog.md` | 修改 | 记录 RPM 支持 |
| `packaging/music-player.spec` | 新增 | RPM 构建规则 |
| `packaging/build-rpm.sh` | 新增 | 本地构建入口 |
| `packaging/music-player.1` | 新增 | man 手册 |

## 9. 批准方式

用户明确回复批准本计划后，才进入实施阶段。如果需要调整，可以指出具体章节或行为；计划更新后再次等待批准。
