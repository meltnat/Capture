@echo off
cd /d %~dp0

set RUSTFLAGS=-Ctarget-feature=+crt-static
set FFMPEG_LIBS_DIR=%USERPROFILE%\Workspace\vcpkg\installed\x64-windows-static\lib
set FFMPEG_INCLUDE_DIR=%USERPROFILE%\Workspace\vcpkg\installed\x64-windows-static\include

cargo build --release --bin=Capture

pause

exit /b