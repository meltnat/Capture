use std::{
    ffi::{c_char, CString},
    ptr::{null_mut, NonNull},
};

use rusty_ffmpeg::ffi::{
    av_write_trailer, avformat_alloc_output_context2, avformat_write_header, avio_close, avio_open,
    AVFormatContext, AVFMT_NOFILE, AVIO_FLAG_WRITE,
};

pub struct Stream {
    format_context: NonNull<AVFormatContext>,
    url: CString,
}

impl Stream {
    pub fn new(url: CString) -> Self {
        let mut format_context = null_mut();
        unsafe {
            avformat_alloc_output_context2(
                &mut format_context,
                null_mut(),
                b"flv\0".as_ptr() as *const c_char,
                url.as_ptr(),
            )
        };
        Stream {
            format_context: NonNull::new(format_context).unwrap(),
            url,
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if (unsafe { *self.format_context.as_ref().oformat }.flags & AVFMT_NOFILE as i32) == 0 {
            let err = unsafe {
                avio_open(
                    &mut self.format_context.as_mut().pb,
                    self.url.as_ptr(),
                    AVIO_FLAG_WRITE as i32,
                )
            };
            if err < 0 {
                return Err(format!(
                    "Failed to open output URL: {}",
                    self.url.to_string_lossy()
                ));
            }
        }
        let err = unsafe { avformat_write_header(self.format_context.as_ptr(), null_mut()) };
        if err < 0 {
            return Err(format!("Failed to write stream header: {}", err));
        }
        Ok(())
    }

    pub fn context(&mut self) -> &NonNull<AVFormatContext> {
        &self.format_context
    }

    pub fn stop(&mut self) {
        unsafe { av_write_trailer(self.format_context.as_ptr()) };
        unsafe { avio_close(self.format_context.as_ref().pb) };
    }
}
