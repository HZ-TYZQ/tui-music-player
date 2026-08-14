# 桌面入口与应用图标实施计划

状态：已实施（待系统安装人工验收与版本决定）
目标分支：当前开发分支
版本策略：本轮不修改 `1.0.0`，正式发布时再决定后续版本

## 1. 目标

为终端音乐播放器增加能够由 Linux 桌面环境识别的应用入口和正式图标。

完成后应满足：

- RPM 安装后，应用菜单中显示 `Music Player`；简体中文环境显示“音乐播放器”。
- 点击菜单项会由桌面环境打开用户首选终端，并在其中运行 `music-player`。
- 图标延续用户草图中的“终端窗口 + 六根频谱柱”构图。
- 图标在 32、48、64、128、256 像素等常见尺寸下保持可辨认。
- 图标和程序继续统一采用 MIT 许可证，版权属名为 HZ-TYZQ。
- 不新增 Rust 运行依赖，也不绑定 GNOME Terminal、Konsole 等特定终端。

`.desktop` 是 Linux 桌面入口文件，作用类似应用菜单中的启动快捷方式；它不是新的图形界面。

## 2. 用户已经决定的事项

### 2.1 图标构图

1. 保留终端提示符、三个窗口点、分隔线和六根频谱柱。
2. 将手绘草图整理为规则的矢量几何图形。
3. 统一线宽、圆角和间距，但不改变原有识别概念。
4. 图形外包一层深色圆角背景。

### 2.2 图标配色

1. 背景使用深石墨色。
2. 前景沿用程序频谱现有的青色 `#14D2DC` 到洋红 `#DC46B4`。
3. 不增加霓虹外发光、玻璃拟态或额外装饰。

### 2.3 设计确认方式

1. 第一轮先制作三个方向相近的 SVG 预览。
2. 三版只比较留白、线宽、圆角和颜色分配，不重新发明构图。
3. 用户选定其中一版后，才制作正式资产并集成 RPM。

### 2.4 桌面入口与发布

1. 英文名称使用 `Music Player`，简体中文名称使用“音乐播放器”。
2. 图标与项目代码一样采用 MIT 许可证。
3. 本轮不立即提升版本号，也不创建新标签或发行版。

### 2.5 正式 Logo 选择

1. 用户选定方案 C 的留白和布局。
2. 终端提示符、三个圆点和分隔线使用冷灰单色。
3. 只有六根频谱柱保留青色到洋红色渐变。

## 3. 设计系统

### 3.1 使用场景四问

- 叙事角色：应用启动器和项目识别标志。
- 观看距离：重点照顾桌面菜单中的 32–64 px，同时支持大尺寸展示。
- 视觉温度：深色、冷静、技术感，并通过频谱渐变加入适量活力。
- 内容容量：只保留终端和六柱频谱，不加入音符、播放按钮、文字或阴影装饰。

### 3.2 共用视觉规则

- 画布使用正方形 `viewBox`，所有关键坐标使用整数或简单比例，便于维护。
- 深色圆角底板四周保留透明安全区，避免桌面环境裁切。
- 提示符、圆点、分隔线和柱体使用统一的端点与圆角语言。
- 分隔线降低对比度，避免它和频谱争夺视觉焦点。
- 渐变只服务于项目现有频谱身份，不添加光晕。
- 在小尺寸预览中优先保证提示符、三个圆点和频谱节奏仍能被辨认。

## 4. 第一检查点：三个 SVG 预览

第一阶段只创建设计预览，不修改 `.desktop`、RPM spec 或版本号。

### 4.1 方案 A：均衡

- 中等安全边距。
- 中等线宽和圆角。
- 青色从提示符开始，频谱向右自然过渡到洋红。
- 目标：最接近原稿，同时兼顾 48 px 可读性。

### 4.2 方案 B：紧凑

- 前景略大，线条略粗。
- 频谱区域占比更高、对比更强。
- 目标：在 32–48 px 菜单图标中更醒目。

### 4.3 方案 C：留白

- 前景略小，四周留白更多。
- 圆角背景更柔和，分隔线更克制。
- 目标：在现代桌面图标网格中更安静、协调。

### 4.4 第一检查点验收

将三版并排展示，同时提供 256 px、64 px、48 px 和 32 px 预览。用户可以：

- 直接选择 A、B 或 C；
- 指定混合，例如“A 的比例 + B 的线宽”；
- 要求一次局部修订。

在用户明确选定前，不进入正式集成。

## 5. 第二阶段：正式图标资产

用户选定方案后：

1. 把原始 Procreate PNG 保存为设计来源，避免未来失去原稿依据。
2. 将选定方案整理为规范的主 SVG，文件名使用 `music-player.svg`。
3. 生成至少一份 48×48 PNG 兼容图标；SVG 支持良好的桌面环境继续使用可缩放版本。
4. 检查 SVG XML 合法性、透明安全区、渐变引用和缩放结果。
5. 在亮色、深色以及透明背景预览中检查边缘和对比度。

计划资产结构：

```text
assets/
├── brand-spec.md
├── source/
│   └── icon-concept.png
└── icons/
    ├── music-player.svg
    └── music-player-48.png
```

`brand-spec.md` 会记录原稿来源、版权、色值、构图规则和禁用方式，防止以后修改图标时逐渐偏离。

## 6. 第三阶段：桌面入口

新增 `packaging/music-player.desktop`，核心字段计划为：

```ini
[Desktop Entry]
Version=1.0
Type=Application
Name=Music Player
Name[zh_CN]=音乐播放器
GenericName=Terminal Music Player
GenericName[zh_CN]=终端音乐播放器
Comment=Play and manage a local music library in the terminal
Comment[zh_CN]=在终端中播放和管理本地音乐库
TryExec=music-player
Exec=music-player
Icon=music-player
Terminal=true
Categories=AudioVideo;Audio;Player;
Keywords=Music;Player;Audio;Terminal;TUI;
Keywords[zh_CN]=音乐;播放器;音频;终端;
StartupNotify=false
```

说明：

- `Terminal=true` 让桌面环境选择用户的默认终端，不硬编码某个终端程序。
- `Icon=music-player` 不写路径和扩展名，使桌面图标主题可以正常查找或替换图标。
- `Version=1.0` 表示桌面入口规范版本，不是应用版本，因此不会和项目的 `1.0.0` 冲突。
- 不声明文件关联；当前程序接收的是音乐库目录，不是“打开单首歌曲”的桌面应用。

## 7. 第四阶段：RPM 集成

计划修改：

- `packaging/music-player.spec`
  - 增加构建依赖 `desktop-file-utils`。
  - 把桌面入口安装到 `%{_datadir}/applications/music-player.desktop`。
  - 把 SVG 安装到 `%{_datadir}/icons/hicolor/scalable/apps/music-player.svg`。
  - 把 48 px PNG 安装到 `%{_datadir}/icons/hicolor/48x48/apps/music-player.png`。
  - 使用 `desktop-file-validate` 验证入口文件。
  - 在 `%files` 中登记新增文件。
- `packaging/build-rpm.sh`
  - 把 `assets` 和桌面入口加入源码归档。
- `README.md`
  - 说明 RPM 安装后可从应用菜单启动。
- `changelog.md`
  - 记录桌面入口和正式图标。

不修改 Cargo 依赖。`desktop-file-utils` 只用于 RPM 构建期规范检查，不是程序运行依赖。

## 8. 验证

### 8.1 设计与文件验证

- 用 XML 工具检查 SVG 语法。
- 渲染并人工查看 256、64、48、32 px 图标。
- 在浅色和深色背景上查看透明边缘。
- 运行 `desktop-file-validate packaging/music-player.desktop`。

### 8.2 源码验证

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --locked -- -D warnings`
- `cargo test --all-targets --locked`
- `bash -n packaging/build-rpm.sh`

### 8.3 RPM 验证

- 构建二进制 RPM 和 SRPM。
- 用 `rpm -qpl` 确认桌面入口、SVG 和 48 px PNG 已进入包中。
- 用 `rpmlint` 检查 spec 与 RPM。
- 经用户允许安装后，在应用菜单中检查名称、图标和终端启动行为。

## 9. 当前假设

- 用户提供的 PNG 是 HZ-TYZQ 的原创作品，可以按项目 MIT 许可证纳入仓库。
- 第一轮预览可以基于原稿重新构建矢量几何，而不是自动描摹所有手绘像素。
- 应用仍然是终端程序，因此桌面入口只负责启动终端，不新增 GUI。
- 桌面环境负责选择默认终端；不同桌面打开终端窗口的动画和标题可能不同。
- 首轮只为简体中文提供本地化字段。

## 10. 推迟决定

- 后续版本号以及是否发布 `v1.1.0`。
- AppStream/软件中心元数据和截图。
- 主题专用图标、单色 symbolic 图标和高对比度变体。
- 图标商标注册或独立品牌使用政策。
- 让桌面入口直接打开指定目录或单首音频文件。

## 11. 实施检查点

1. 批准本计划后，只制作三个 SVG 预览。
2. 用户选定或混合方案后，制作正式资产。
3. 正式资产再次人工确认后，才集成 `.desktop` 和 RPM。
4. 完成验证后再由用户决定是否暂存、提交、推送或准备新版本。

## 12. 实施结果

- 正式 SVG、48 px PNG、原始概念稿和品牌规范已固化到 `assets/`。
- 桌面入口已新增并通过 `desktop-file-validate`。
- RPM spec 已安装桌面入口和两种图标；发布工作流已补齐构建期检查依赖。
- 源码格式、Clippy 和全部 33 项测试通过。
- Fedora 44 RPM 与 SRPM 构建成功，包内路径检查通过。
- rpmlint 为 0 个错误、4 个已知警告；警告来自本地 `Source0`/`Source1` 归档没有 URL，与本次桌面集成无关。
- 尚未修改版本号，也尚未把新 RPM 安装到系统；当前生成的 RPM 与已发布版本具有相同的 `1.0.0-1` 标识，只用于构建验证。
