use std::ptr::{null_mut, NonNull};

use rusty_ffmpeg::ffi::{
    av_frame_alloc, av_frame_get_buffer, av_rescale_rnd, swr_alloc_set_opts2, swr_convert,
    swr_get_delay, swr_init, AVFrame, SwrContext, AV_ROUND_UP,
};

use crate::audio::audio_format::AudioFormat;

pub struct AudioResampler {
    swr: NonNull<SwrContext>,
    input: AudioFormat,
    output: AudioFormat,
}

impl AudioResampler {
    pub fn new(input: AudioFormat, output: AudioFormat) -> Self {
        let mut swr = null_mut();
        let channel_layout = input.get_channel_layout_default();
        let err = unsafe {
            swr_alloc_set_opts2(
                &mut swr,
                &output.get_channel_layout_default(),
                output.sample_format,
                output.sample_rate as i32,
                &channel_layout,
                input.sample_format,
                input.sample_rate as i32,
                0,
                null_mut(),
            )
        };
        if err < 0 {
            panic!("Failed to allocate resampler: {}", err);
        }
        unsafe { swr_init(swr) };
        AudioResampler {
            swr: NonNull::new(swr).unwrap(),
            input,
            output,
        }
    }

    pub fn from_bytes(&mut self, bytes: &[u8]) -> AVFrame {
        let mut frame = unsafe { *av_frame_alloc() };
        frame.ch_layout = self.output.get_channel_layout_default();
        frame.sample_rate = self.output.sample_rate as i32;
        frame.format = self.output.sample_format;

        let src_nb_samples =
            bytes.len() as i32 / (self.input.channels * self.input.bytes_per_sample);
        frame.nb_samples = unsafe {
            av_rescale_rnd(
                swr_get_delay(self.swr.as_ptr(), self.input.sample_rate as i64)
                    + src_nb_samples as i64,
                self.output.sample_rate as i64,
                self.input.sample_rate as i64,
                AV_ROUND_UP,
            )
        } as i32;
        if unsafe { av_frame_get_buffer(&mut frame, 0) } < 0 {
            panic!("Failed to get frame buffer");
        }

        let ret = unsafe {
            swr_convert(
                self.swr.as_mut(),
                frame.data.as_mut_ptr(),
                frame.nb_samples,
                [bytes.as_ptr()].as_ptr(),
                src_nb_samples,
            )
        };
        if ret < 0 {
            panic!("Failed to convert audio");
        }
        frame.nb_samples = ret;
        frame
    }
}
