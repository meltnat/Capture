use std::ptr::null_mut;

use rusty_ffmpeg::ffi::{
    av_frame_alloc, sws_getContext, sws_scale, AVFrame, SwsContext, SWS_BILINEAR,
};

use crate::video::video_format::VideoFormat;

pub struct VideoEncoder {
    format: VideoFormat,
    sws: SwsContext,
}

impl VideoEncoder {
    pub fn new(input: VideoFormat, output: VideoFormat) -> Self {
        let sws = unsafe {
            *sws_getContext(
                input.width,
                input.height,
                input.pix_fmt,
                output.width,
                output.height,
                output.pix_fmt,
                SWS_BILINEAR as i32,
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        VideoEncoder {
            format: output,
            sws,
        }
    }

    pub fn encode(&mut self, input: &AVFrame) -> AVFrame {
        let mut output = unsafe { *av_frame_alloc() };
        output.format = self.format.pix_fmt;
        output.width = self.format.width;
        output.height = self.format.height;
        output.pts = input.pts;
        unsafe {
            sws_scale(
                &mut self.sws,
                input.data.as_ptr() as *const *const u8,
                input.linesize.as_ptr(),
                0,
                input.height,
                output.data.as_mut_ptr(),
                output.linesize.as_mut_ptr(),
            )
        };
        output
    }
}
