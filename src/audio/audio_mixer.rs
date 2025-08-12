use std::{fmt, slice::from_raw_parts};

use rusty_ffmpeg::ffi::{av_frame_alloc, AVFrame};

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

    fn mix(&self, a: &AVFrame, b: &AVFrame) -> Result<AVFrame, Box<dyn std::error::Error>> {
        let mut frame = unsafe { *av_frame_alloc() };
        frame.format = self.format.sample_format();
        frame.sample_rate = self.format.sample_rate;
        frame.ch_layout = self.format.channel_layout_default();
        frame.nb_samples = a.nb_samples.min(b.nb_samples);

        type DataType = i64;

        for i in 0..self.format.channels as usize {
            let a_channel = a.data[i];
            let b_channel = b.data[i];

            let a_samples = unsafe { from_raw_parts(a_channel, a.nb_samples as usize) };
            let b_samples = unsafe { from_raw_parts(b_channel, b.nb_samples as usize) };

            let mixed_samples: Vec<u8> =
                mixing::<DataType>(&from_le_bytes(a_samples), &from_le_bytes(b_samples))
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect();

            unsafe {
                std::ptr::copy_nonoverlapping(
                    mixed_samples.as_ptr(),
                    frame.data[i],
                    mixed_samples.len(),
                );
            }
        }

        frame.pts = a.pts.min(b.pts);
        Ok(frame)
    }

    pub fn mix_with_resample(&self) -> Result<AVFrame, Box<dyn std::error::Error>> {
        if let (Some(a_resampler), Some(b_resampler)) = (&self.a_resampler, &self.b_resampler) {
            self.mix(
                &a_resampler.resample(&self.a.capture()?),
                &b_resampler.resample(&self.b.capture()?),
            )
        } else {
            Err("Resampling not set up for one or both inputs".into())
        }
    }

    pub fn format(&self) -> &AudioFormat {
        &self.format
    }

    pub fn set_resampler(&mut self, a: AudioFormat, b: AudioFormat) -> &mut Self {
        self.a_resampler = Some(AudioResampler::new(a, self.format.clone()));
        self.b_resampler = Some(AudioResampler::new(b, self.format.clone()));
        self
    }
}

trait FromLeBytes {
    fn from(bytes: &[u8]) -> Self;
}

macro_rules! impl_from_le_bytes {
    ($t:ty,$n:expr) => {
        impl FromLeBytes for $t {
            fn from(bytes: &[u8]) -> Self {
                let arr: [u8; $n] = bytes[..$n].try_into().expect("Slice with incorrect length");
                <$t>::from_le_bytes(arr)
            }
        }
    };
}

impl_from_le_bytes!(i8, 1);
impl_from_le_bytes!(i16, 2);
impl_from_le_bytes!(i32, 4);
impl_from_le_bytes!(i64, 8);
impl_from_le_bytes!(i128, 16);
impl_from_le_bytes!(u8, 1);
impl_from_le_bytes!(u16, 2);
impl_from_le_bytes!(u32, 4);
impl_from_le_bytes!(u64, 8);
impl_from_le_bytes!(u128, 16);

fn from_le_bytes<T: FromLeBytes>(bytes: &[u8]) -> Vec<T> {
    let mut result = Vec::with_capacity(bytes.len() / std::mem::size_of::<T>());
    for chunk in bytes.chunks_exact(std::mem::size_of::<T>()) {
        let value = T::from(chunk);
        result.push(value);
    }
    result
}

fn mixing<T: Into<i128> + TryFrom<i128> + Copy>(a: &[T], b: &[T]) -> Vec<T>
where
    <T as TryFrom<i128>>::Error: std::fmt::Debug,
{
    let len = a.len().min(b.len());
    let mut result: Vec<T> = Vec::with_capacity(len);
    for i in 0..len {
        result.push(T::try_from((a[i].into() + b[i].into()) / 2).expect("Conversion failed"));
    }
    result
}

impl AudioInput for AudioMixer {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.a.start()?;
        self.b.start()
    }

    fn capture(&self) -> Result<AVFrame, Box<dyn std::error::Error>> {
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
