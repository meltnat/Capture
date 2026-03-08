use std::ptr::{null_mut, NonNull};

use rusty_ffmpeg::ffi::{
    av_frame_alloc, av_frame_get_buffer, sws_getContext, sws_scale, AVFrame, SwsContext,
    SWS_BILINEAR,
};

use crate::video::video_format::VideoFormat;

pub struct VideoEncoder {
    input: VideoFormat,
    output: VideoFormat,
    sws: NonNull<SwsContext>,
}

impl VideoEncoder {
    pub fn new(input: VideoFormat, output: VideoFormat) -> Self {
        let sws = unsafe {
            sws_getContext(
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
            input,
            output,
            sws: NonNull::new(sws).unwrap(),
        }
    }

    pub fn from_bytes(&mut self, bytes: &[u8]) -> AVFrame {
        let mut frame = unsafe { *av_frame_alloc() };
        frame.width = self.output.width;
        frame.height = self.output.height;
        frame.format = self.output.pix_fmt as i32;
        unsafe { av_frame_get_buffer(&mut frame, 0) };

        let src = [bytes.as_ptr(), null_mut(), null_mut(), null_mut()];
        let stride = [self.input.width * 4, 0, 0, 0];
        unsafe {
            sws_scale(
                self.sws.as_mut(),
                src.as_ptr(),
                stride.as_ptr(),
                0,
                self.input.height,
                frame.data.as_mut_ptr(),
                frame.linesize.as_mut_ptr(),
            )
        };
        frame
    }
}
