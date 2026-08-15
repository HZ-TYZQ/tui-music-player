# Windows 11 x86_64 MSVC 支持计划

状态：Linux/RPM 与 Windows MSVC CI 已通过，等待带 tag 的发行打包验证
日期：2026-08-15

## 目标

- 保持现有 Linux、GStreamer、SQLite 和 Fedora RPM 行为不变。
- 新增 `x86_64-pc-windows-msvc` 构建与测试。
- 提供携带完整官方 GStreamer runtime 的 Windows Installer 和 Portable ZIP。
- 用户无需另行安装 GStreamer 或 SQLite。

## 用户决定

- Installer 使用 Inno Setup `Setup.exe`，默认当前用户安装。
- 第一版携带完整的官方 GStreamer runtime，不提前裁剪插件。
- Portable 表示免安装，配置、播放列表和缓存仍使用 Windows 标准用户目录。
- 第一版不做 Authenticode 签名，Release 提供 SHA-256。
- Installer 提供“加入用户 PATH”的可选任务，默认不勾选。
- 只有 Windows CI 或 Windows 11 真机出现实际失败后，才增加最小范围的
  `#[cfg(windows)]`；不预先增加平台代码。

## 技术方案

1. 使用 `rusqlite` 的 `bundled-windows` 特性，让 Windows EXE 内置 SQLite；
   Linux 继续链接系统 SQLite。
2. Windows CI 固定并校验官方 GStreamer MSVC x86_64 安装包，使用 devel
   组件编译和测试。
3. 发行目录保留 GStreamer 官方结构，把 `music-player.exe` 放入 `bin`；包根
   提供 `music-player.cmd`，Installer 的可选 PATH 任务只加入包根。
4. Installer 和 Portable ZIP 从同一个 staging 目录生成，包含应用许可证、
   Windows 使用说明及 GStreamer 自带的第三方许可证。
5. Linux CI 保留；Release 改为 RPM 与 Windows 并行构建、全部成功后统一发布。

## 实施阶段

### 1. 平台中性代码与测试

- 将 XDG 专属用户提示改为“系统 Music 目录”等平台中性表述。
- 增加配置重复覆盖、播放列表重复保存、Windows 风格路径序列化、中文及空格
  媒体路径测试。
- 先让现有实现直接跑 Windows CI；只有测试证明失败才引入平台分支。

### 2. Windows CI

- 在 `windows-latest` 上安装固定版本 GStreamer MSVC x86_64 devel 环境。
- 执行 Clippy 和全部测试，包括无声 GStreamer 集成测试。
- 所有下载校验仓库中固定的 SHA-256。

### 3. Windows 发行包

- PowerShell 脚本构造完整 runtime staging、校验关键插件并生成 Portable ZIP。
- Inno Setup 生成当前用户 Installer、开始菜单入口和安全的可选 PATH 任务。
- 自动测试 Portable 启动、Installer 静默安装/卸载和 PATH 清理。

### 4. Release 整合

- 校验 tag、Cargo 版本和 RPM spec 版本一致。
- Fedora 44 container 继续调用现有 `packaging/build-rpm.sh`。
- Windows job 生成 Installer 与 Portable ZIP。
- 最终 job 汇总所有产物、生成 `SHA256SUMS.txt` 并只创建一次 Release。

## 验收

- Linux fmt、Clippy、全部测试与 RPM 构建通过。
- Windows MSVC Clippy、全部测试和 staging smoke test 通过。
- 干净 Windows 11 x64 上无需系统 GStreamer/SQLite即可播放和显示频谱。
- Installer 默认不修改 PATH；勾选后可直接运行 `music-player`，卸载只移除
  自己的 PATH 项并保留用户数据。
- Portable ZIP 解压即用，配置仍保存在 AppData。

## 延期事项

- Authenticode、MSI、MSIX/Microsoft Store、ARM64、完全自包含 portable 数据、
  runtime 裁剪和自动更新。
