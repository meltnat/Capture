$target = "x64-windows-static-md"
$features = @(
    "core"
    "avcodec"
    "avformat"
    "avfilter"
    "avdevice"
    "swresample"
    "swscale"
    "x264"
    "nvcodec"
) -join ","

# Uninstall FFmpeg with the specified features
vcpkg remove "ffmpeg:$target"

# Install FFmpeg with the specified features
vcpkg install "ffmpeg[$features]:$target"

Pause

exit 0