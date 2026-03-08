@echo off

setlocal
pushd %~dp0

set RUST_BACKTRACE=1
set RUSTFLAGS=-Ctarget-feature=+crt-static
set FFMPEG_INCLUDE_DIR=%USERPROFILE%\Workspace\vcpkg\installed\x64-windows-static\include
set FFMPEG_LIBS_DIR=%USERPROFILE%\Workspace\vcpkg\installed\x64-windows-static\lib

rem cargo run -- -a -m -d --video-encoder h264_nvenc -u rtmp://localhost/live/test
cargo run -- -a -m -d --video-encoder h264_nvenc -u rtmp://localhost/live/test

popd
endlocal

exit /b
