Music Player for Windows 11 x86_64
=================================

启动
----

安装版可以从开始菜单启动。Portable ZIP 解压后运行 music-player.cmd。
在 Windows Terminal 中也可以执行：

    .\music-player.cmd

使用 --set-library 可以永久设置音乐库：

    .\music-player.cmd --set-library "D:\Music"

用户数据
--------

配置和播放列表保存在 %APPDATA%\tui-music-player，缓存保存在
%LOCALAPPDATA%\tui-music-player。Portable ZIP 也使用这些标准目录。
卸载应用不会删除这些用户数据。

运行依赖
--------

本发行包已经私有携带 GStreamer runtime，SQLite 也已编入程序。
用户不需要另外安装 GStreamer 或 SQLite，也不应把包内的 DLL 单独移动出去。

许可与源码
----------

包内附带 GStreamer 1.28.6 官方许可文本与 LGPL 2.1 全文（见
third-party-licenses 目录），以及 HZ-TYZQ 提供对应源码的书面承诺，详见
THIRD-PARTY-NOTICES.txt 与 SOURCE-CODE-OFFER.txt。

安全提示
--------

第一版 Windows 发行包尚未使用 Authenticode 签名。Windows Defender
SmartScreen 可能显示“Windows 已保护你的电脑”。请只从项目的 GitHub Release
下载，并用同一 Release 中的 SHA256SUMS.txt 核对文件。

项目主页：https://github.com/HZ-TYZQ/tui-music-player
版权属名：HZ-TYZQ
