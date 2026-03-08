use tokio::sync::mpsc::Sender;

use crate::{input::Input, overlay::draw_keylog, video::video_desktop::VideoDesktop};
impl Input for VideoDesktop {
    fn start(&mut self, sender: &Sender<Vec<u8>>) -> Result<(), Box<dyn std::error::Error>> {
        let sender = sender.clone();
        let keylog = self.keylog.clone();
        let width = self.width();
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
            let mut bytes = bytes?;

            // キーログオーバーレイを BGRA フレームに描画
            if let Some(log) = &keylog {
                if let Ok(guard) = log.lock() {
                    let keys: Vec<&str> = guard.current_keys();
                    if !keys.is_empty() {
                        draw_keylog(&mut bytes, width, &keys);
                    }
                }
            }

            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let mut data = Vec::with_capacity(16 + bytes.len());
            data.extend_from_slice(&ts.to_le_bytes());
            data.extend_from_slice(&bytes);

            if let Err(_) = sender.try_send(data) {
                // channel full or closed: drop frame to avoid blocking the capture callback
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
