use std::{os::raw::c_void, ptr::null_mut};

use rusty_ffmpeg::ffi::{
    av_frame_get_buffer, av_frame_make_writable, av_samples_get_buffer_size, memcpy, AVFrame,
};

use crate::audio::audio_device::AudioDevice;

pub trait AudioInput {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn capture(&self) -> Result<rusty_ffmpeg::ffi::AVFrame, Box<dyn std::error::Error>>;
    fn stop(&self) -> Result<(), Box<dyn std::error::Error>>;
}

impl AudioInput for AudioDevice {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.start().is_err() {
            return Err("Failed to start audio device".into());
        }
        Ok(())
    }

    fn capture(&self) -> Result<AVFrame, Box<dyn std::error::Error>> {
        let data = self.get_wave()?;
        let mut frame = self.format().frame(data.len() as i32);
        unsafe { av_frame_get_buffer(&mut frame, 0) };
        let required = unsafe {
            av_samples_get_buffer_size(
                null_mut(),
                self.format().channels,
                frame.nb_samples,
                self.format().sample_format(),
                1,
            )
        };
        if required < 0 {
            return Err("Failed to get buffer size".into());
        }
        if required as usize != data.len() {
            return Err("Data length does not match required buffer size".into());
        }
        unsafe { av_frame_make_writable(&mut frame) };
        unsafe {
            memcpy(
                frame.data[0] as *mut c_void,
                data.as_ptr() as *const c_void,
                data.len() as u64,
            )
        };
        Ok(frame)
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.stop().is_err() {
            return Err("Failed to stop audio device".into());
        }
        Ok(())
    }
}
