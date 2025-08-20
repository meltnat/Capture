use std::{fmt, slice::from_raw_parts};

use rusty_ffmpeg::ffi::AVFrame;

use crate::audio::{
    audio_format::AudioFormat, audio_input::AudioInput, audio_resampler::AudioResampler,
};

pub struct AudioMixer {
    a: Box<dyn AudioInput>,
    b: Box<dyn AudioInput>,
    format: AudioFormat,
    a_resampler: Option<AudioResampler>,
    b_resampler: Option<AudioResampler>,
}

impl AudioMixer {
    pub fn new(a: Box<dyn AudioInput>, b: Box<dyn AudioInput>, format: AudioFormat) -> Self {
        Self {
            a,
            b,
            format,
            a_resampler: None,
            b_resampler: None,
        }
    }

    fn mix(&self, a: &AVFrame, b: &AVFrame) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let channels = self.format.channels as usize;
        let nb_samples = std::cmp::min(a.nb_samples, b.nb_samples) as usize;
        let mut out: Vec<f32> = Vec::with_capacity(nb_samples * channels);

        for i in 0..nb_samples {
            for ch in 0..channels {
                let a_plane = a.data[ch];
                let b_plane = b.data[ch];
                if a_plane.is_null() || b_plane.is_null() {
                    return Err(format!("Null plane pointer at channel {}", ch).into());
                }
                let a_slice = unsafe { from_raw_parts(a_plane as *const f32, nb_samples) };
                let b_slice = unsafe { from_raw_parts(b_plane as *const f32, nb_samples) };
                let mixed = (a_slice[i] + b_slice[i]) * 0.5;
                out.push(mixed);
            }
        }

        let byte_len = out.len() * std::mem::size_of::<f32>();
        let mut bytes = Vec::<u8>::with_capacity(byte_len);
        unsafe { bytes.set_len(byte_len) };
        unsafe {
            std::ptr::copy_nonoverlapping(out.as_ptr() as *const u8, bytes.as_mut_ptr(), byte_len)
        };
        Ok(bytes)
    }

    pub fn mix_with_resample(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        if self.a_resampler.is_none() || self.b_resampler.is_none() {
            return Err("Resampler not set up for one or both inputs".into());
        }
        let a = self.a.capture()?;
        if a.is_empty() {
            return Ok(Vec::new());
        }
        let a = &self.a_resampler.as_mut().unwrap().from_bytes(&a);
        let b = self.b.capture()?;
        if b.is_empty() {
            return Ok(Vec::new());
        }
        let b = &self.b_resampler.as_mut().unwrap().from_bytes(&b);
        self.mix(a, b)
    }

    pub fn set_resampler(&mut self, a: AudioFormat, b: AudioFormat) -> &mut Self {
        self.a_resampler = Some(AudioResampler::new(a, self.format.clone()));
        self.b_resampler = Some(AudioResampler::new(b, self.format.clone()));
        self
    }
}

impl AudioInput for AudioMixer {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.a.start()?;
        self.b.start()
    }

    fn capture(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.mix_with_resample()
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.a.stop()?;
        self.b.stop()
    }
}

impl fmt::Display for AudioMixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AudioMixer: {}Hz, {} channels, {}bps",
            self.format.sample_rate, self.format.channels, self.format.bit_rate
        )
    }
}
