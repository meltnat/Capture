use rusty_ffmpeg::ffi::AVPixelFormat;

#[derive(Eq, PartialEq, Clone)]
pub struct VideoFormat {
    pub width: i32,
    pub height: i32,
    pub bit_rate: i64,
    pub pix_fmt: AVPixelFormat,
    pub fps: i32,
}
