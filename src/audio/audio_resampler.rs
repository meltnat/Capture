use std::ptr::null_mut;

use rusty_ffmpeg::ffi::{
    av_frame_alloc, av_frame_get_buffer, swr_alloc_set_opts2, swr_convert_frame, AVChannelLayout,
    AVFrame, SwrContext,
};

use crate::audio::audio_format::AudioFormat;

pub struct AudioResampler {
    swr: *mut SwrContext,
    channel_layout: AVChannelLayout,
    sample_rate: i32,
    format: AudioFormat,
}

impl AudioResampler {
    pub fn new(input: AudioFormat, output: AudioFormat) -> Self {
        let mut swr = null_mut();
        let channel_layout = input.channel_layout_default();
        unsafe {
            swr_alloc_set_opts2(
                &mut swr,
                &output.channel_layout_default(),
                output.sample_format(),
                output.sample_rate as i32,
                &channel_layout,
                input.sample_format(),
                input.sample_rate as i32,
                0,
                null_mut(),
            )
        };
        AudioResampler {
            swr,
            channel_layout,
            sample_rate: output.sample_rate,
            format: output,
        }
    }

    pub fn resample(&self, frame: &AVFrame) -> AVFrame {
        let mut resampled = unsafe { *av_frame_alloc() };
        resampled.ch_layout = self.channel_layout;
        resampled.sample_rate = self.sample_rate;
        resampled.format = self.format.sample_format();
        resampled.nb_samples = frame.nb_samples;
        resampled.pts = frame.pts;
        unsafe { av_frame_get_buffer(&mut resampled, 0) };
        unsafe { swr_convert_frame(self.swr, &mut resampled, frame) };
        resampled
    }
}
