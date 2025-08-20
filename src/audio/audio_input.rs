use crate::audio::audio_device::AudioDevice;

pub trait AudioInput {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn capture(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
    fn stop(&self) -> Result<(), Box<dyn std::error::Error>>;
}

impl AudioInput for AudioDevice {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.start().is_err() {
            return Err("Failed to start audio device".into());
        }
        Ok(())
    }

    fn capture(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.get_wave()
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.stop().is_err() {
            return Err("Failed to stop audio device".into());
        }
        Ok(())
    }
}
