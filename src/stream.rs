use std::ptr::null_mut;

use rusty_ffmpeg::ffi::{
    avformat_alloc_context, avformat_alloc_output_context2, avformat_write_header, avio_close,
    avio_open, AVFormatContext, AVFMT_NOFILE, AVIO_FLAG_WRITE,
};

pub struct Stream {
    format_context: AVFormatContext,
}

impl Stream {
    pub fn new(url: &str) -> Self {
        let mut _format_context = unsafe { avformat_alloc_context() };
        unsafe {
            avformat_alloc_output_context2(
                &mut _format_context,
                null_mut(),
                b"flv\0".as_ptr() as *const i8,
                url.as_ptr() as *const i8,
            )
        };
        let mut format_context = unsafe { *_format_context };
        if (unsafe { *format_context.oformat }.flags & AVFMT_NOFILE as i32) != 0 {
            unsafe {
                avio_open(
                    &mut format_context.pb,
                    url.as_ptr() as *const i8,
                    AVIO_FLAG_WRITE as i32,
                )
            };
        }
        unsafe { avformat_write_header(_format_context, null_mut()) };
        Stream { format_context }
    }

    pub fn context(&mut self) -> &mut AVFormatContext {
        &mut self.format_context
    }

    pub fn stop(&mut self) {
        unsafe { avio_close(self.format_context.pb) };
    }
}
