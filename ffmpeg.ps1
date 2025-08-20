$target = "x64-windows-static"
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
    "ssh"
) -join ","

# Uninstall FFmpeg with the specified features
vcpkg remove "ffmpeg:$target"

# Install FFmpeg with the specified features
vcpkg install "ffmpeg[$features]:$target"



Pause

exit 0