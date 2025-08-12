use core::fmt;
use rusty_ffmpeg::ffi::{
    av_channel_layout_default, av_frame_alloc, AVChannelLayout, AVFrame, AVSampleFormat,
    AV_SAMPLE_FMT_NONE, AV_SAMPLE_FMT_S16, AV_SAMPLE_FMT_S32, AV_SAMPLE_FMT_U8,
};
use std::ptr::null_mut;
use windows::Win32::Media::Audio::WAVEFORMATEX;

#[derive(PartialEq, Clone)]
pub struct AudioFormat {
    pub bit_rate: i64,
    pub sample_rate: i32,
    pub channels: i32,
    pub bytes_per_sample: i32,
    pub volume: f64,
}

impl AudioFormat {
    pub fn new(bit_rate: i64, sample_rate: i32, channels: i32, bytes_per_sample: i32) -> Self {
        Self {
            bit_rate,
            sample_rate,
            channels,
            bytes_per_sample,
            volume: 1.0,
        }
    }
    pub fn sample_format(&self) -> AVSampleFormat {
        match self.bytes_per_sample {
            1 => AV_SAMPLE_FMT_U8,
            2 => AV_SAMPLE_FMT_S16,
            3 => AV_SAMPLE_FMT_S32,
            4 => AV_SAMPLE_FMT_S32,
            _ => AV_SAMPLE_FMT_NONE,
        }
    }
    pub fn channel_layout_default(&self) -> AVChannelLayout {
        let ch_layout = null_mut();
        unsafe { av_channel_layout_default(ch_layout, self.channels) };
        unsafe { *ch_layout }
    }
    pub fn frame(&self, length: i32) -> AVFrame {
        let mut frame = unsafe { *av_frame_alloc() };
        frame.ch_layout = self.channel_layout_default();
        frame.sample_rate = self.sample_rate;
        frame.format = self.sample_format() as i32;
        frame.nb_samples = self.nb_samples(length);
        frame
    }

    pub fn nb_samples(&self, length: i32) -> i32 {
        length / self.channels * self.bytes_per_sample
    }
}

impl From<WAVEFORMATEX> for AudioFormat {
    fn from(wfx: WAVEFORMATEX) -> Self {
        Self::new(
            (wfx.nAvgBytesPerSec * 8) as i64,
            wfx.nSamplesPerSec as i32,
            wfx.nChannels as i32,
            wfx.wBitsPerSample as i32 / 8,
        )
    }
}

impl fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} bps, {} Hz, {} ch, {} bytes/sample, {:.1}%",
            self.bit_rate,
            self.sample_rate,
            self.channels,
            self.bytes_per_sample,
            self.volume * 100.0
        )
    }
}
