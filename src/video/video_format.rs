use rusty_ffmpeg::ffi::{av_frame_alloc, AVFrame, AVPixelFormat};

#[derive(Eq, PartialEq, Clone)]
pub struct VideoFormat {
    pub width: i32,
    pub height: i32,
    pub bit_rate: i64,
    pub pix_fmt: AVPixelFormat,
    pub fps: i32,
}

impl VideoFormat {
    pub fn from_bytes(&self, bytes: &[u8]) -> AVFrame {
        let mut frame = unsafe { *av_frame_alloc() };
        frame.width = self.width;
        frame.height = self.height;
        frame.format = self.pix_fmt as i32;
        frame.data[0] = bytes.as_ptr() as *mut u8;
        frame.linesize[0] = self.width * 4;
        frame
    }
}
