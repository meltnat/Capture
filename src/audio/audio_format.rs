use core::fmt;
use rusty_ffmpeg::ffi::{
    av_channel_layout_default, av_frame_alloc, av_frame_get_buffer, av_get_bytes_per_sample,
    av_sample_fmt_is_planar, AVChannelLayout, AVCodecContext, AVFrame, AVSampleFormat,
    AV_SAMPLE_FMT_FLT, AV_SAMPLE_FMT_NONE, AV_SAMPLE_FMT_S16, AV_SAMPLE_FMT_S32, AV_SAMPLE_FMT_U8,
};
use std::mem::zeroed;
use windows::Win32::Media::Audio::WAVEFORMATEX;

#[derive(PartialEq, Clone)]
pub struct AudioFormat {
    pub bit_rate: i64,
    pub sample_rate: i32,
    pub channels: i32,
    pub bytes_per_sample: i32,
    pub sample_format: AVSampleFormat,
}

impl AudioFormat {
    pub fn new(
        bit_rate: i64,
        sample_rate: i32,
        channels: i32,
        bytes_per_sample: i32,
        sample_format: AVSampleFormat,
    ) -> Self {
        Self {
            bit_rate,
            sample_rate,
            channels,
            bytes_per_sample,
            sample_format,
        }
    }
    pub fn sample_format(bits_per_sample: i32, w_format_tag: u16) -> AVSampleFormat {
        const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
        match (bits_per_sample, w_format_tag) {
            (32, WAVE_FORMAT_IEEE_FLOAT) => AV_SAMPLE_FMT_FLT,
            (8, _) => AV_SAMPLE_FMT_U8,
            (16, _) => AV_SAMPLE_FMT_S16,
            (24, _) => AV_SAMPLE_FMT_S32,
            (32, _) => AV_SAMPLE_FMT_S32,
            _ => AV_SAMPLE_FMT_NONE,
        }
    }
    pub fn channel_layout_default(channels: i32) -> AVChannelLayout {
        let mut ch_layout = unsafe { zeroed() };
        unsafe { av_channel_layout_default(&mut ch_layout, channels) };
        ch_layout
    }

    pub fn get_channel_layout_default(&self) -> AVChannelLayout {
        AudioFormat::channel_layout_default(self.channels)
    }

    pub fn nb_samples(&self, length: i32) -> i32 {
        length / (self.channels * self.bytes_per_sample)
    }

    pub fn from_bytes(&self, bytes: &[u8]) -> AVFrame {
        let mut frame = unsafe { *av_frame_alloc() };
        frame.sample_rate = self.sample_rate;
        frame.ch_layout = self.get_channel_layout_default();
        frame.format = self.sample_format;
        frame.nb_samples = self.nb_samples(bytes.len() as i32);
        let err = unsafe { av_frame_get_buffer(&mut frame, 0) };
        if err < 0 {
            panic!("Failed to get frame buffer: {}", err);
        }

        let is_planar = unsafe { av_sample_fmt_is_planar(self.sample_format) != 0 };
        let channels = self.channels as usize;
        let bps = self.bytes_per_sample as usize;
        if is_planar {
            let samples_per_ch = frame.nb_samples as usize;
            let required = samples_per_ch * channels * bps;
            if bytes.len() < required {
                panic!(
                    "Insufficient input bytes: have {}, need {} ({} samples * {} ch * {} B)",
                    bytes.len(), required, samples_per_ch, channels, bps
                );
            }
            for ch in 0..channels {
                let dst_plane = frame.data[ch];
                if dst_plane.is_null() { continue; }
                for i in 0..samples_per_ch {
                    let src_index = (i * channels + ch) * bps;
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            bytes.as_ptr().add(src_index),
                            dst_plane.add(i * bps),
                            bps,
                        );
                    }
                }
            }
        } else {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), frame.data[0], bytes.len());
            }
        }
        frame
    }
}

impl From<WAVEFORMATEX> for AudioFormat {
    fn from(wfx: WAVEFORMATEX) -> Self {
        let sample_format = Self::sample_format(wfx.wBitsPerSample as i32, wfx.wFormatTag);
        Self::new(
            (wfx.nAvgBytesPerSec * 8) as i64,
            wfx.nSamplesPerSec as i32,
            wfx.nChannels as i32,
            (wfx.wBitsPerSample / 8) as i32,
            sample_format,
        )
    }
}

impl From<&AVCodecContext> for AudioFormat {
    fn from(codec_context: &AVCodecContext) -> Self {
        let bytes_per_sample = unsafe { av_get_bytes_per_sample(codec_context.sample_fmt) };
        Self::new(
            codec_context.bit_rate,
            codec_context.sample_rate,
            codec_context.ch_layout.nb_channels,
            bytes_per_sample,
            codec_context.sample_fmt,
        )
    }
}

impl fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} bps, {} Hz, {} ch, {} bytes/sample",
            self.bit_rate, self.sample_rate, self.channels, self.bytes_per_sample,
        )
    }
}
