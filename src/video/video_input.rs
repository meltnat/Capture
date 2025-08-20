use tokio::sync::mpsc::Sender;

use crate::{input::Input, video::video_desktop::VideoDesktop};
impl Input for VideoDesktop {
    fn start(&mut self, sender: &Sender<Vec<u8>>) -> Result<(), Box<dyn std::error::Error>> {
        let sender = sender.clone();
        self.start(move |pool| {
            let texture = Self::get_texture(pool);
            if let Err(err) = texture {
                eprintln!("Failed to get texture: {}", err);
                return Err(err.into());
            }
            let texture = Self::staging(texture?);
            if let Err(err) = texture {
                eprintln!("Failed to stage texture: {}", err);
                return Err(err.into());
            }
            let bytes = Self::to_bytes(texture?);
            if let Err(err) = bytes {
                eprintln!("Failed to convert texture to bytes: {}", err);
                return Err(err.into());
            }
            let bytes = bytes?;
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let mut data = Vec::with_capacity(16 + bytes.len());
            data.extend_from_slice(&ts.to_le_bytes());
            data.extend_from_slice(&bytes);

            if let Err(err) = sender.blocking_send(data) {
                eprintln!("Failed to send video frame: {}", err);
            }
            Ok(())
        })?;
        Ok(())
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.stop()?;
        Ok(())
    }
}
