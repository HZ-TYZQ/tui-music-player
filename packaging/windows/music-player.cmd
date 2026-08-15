@echo off
set "PATH=%~dp0bin;%PATH%"
set "GSTREAMER_1_0_ROOT_MSVC_X86_64=%~dp0"
set "GST_PLUGIN_SYSTEM_PATH_1_0=%~dp0lib\gstreamer-1.0"
set "GST_PLUGIN_PATH_1_0="
"%~dp0bin\music-player.exe" %*
