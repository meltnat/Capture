use std::collections::HashMap;

use clap::Parser;
#[derive(Parser, Debug)]
#[command[version, about, long_about = None]]
pub struct Args {
    /* General Settings */
    /// enable debug mode
    #[arg(long, default_value_t = false)]
    pub debug: bool,
    /// ffmpeg path
    #[arg(short, long, default_value = "ffmpeg")]
    pub ffmpeg: String,

    /* Audio Settings */
    /// mic device index
    #[arg(short, long)]
    pub microphone: Option<Option<usize>>,
    // mic volume
    #[arg(long, default_value_t = 1.0)]
    pub mic_volume: f64,
    /// audio device index
    #[arg(short, long)]
    pub audio: Option<Option<usize>>,
    /// audio volume
    #[arg(long, default_value_t = 1.0)]
    pub audio_volume: f64,

    /* Video Settings */
    /// display device index
    #[arg(short, long)]
    pub display: Option<Option<usize>>,

    /* Video Stream Settings */
    /// video encoder name
    #[arg(long, default_value_t = {"libx264".to_string()})]
    pub video_encoder: String,
    /// stream size
    #[arg(short, long, default_value_t = {"1920x1080".to_string()})]
    pub size: String,
    /// video bit rate
    #[arg(long, default_value_t = {"2500k".to_string()})]
    pub video_bit_rate: String,
    /// stream fps
    #[arg(long, default_value_t = 30)]
    pub fps: usize,
    /// additional options for the video encoder
    #[arg(long)]
    pub video_options: Vec<String>,

    /* Audio Stream Settings */
    // audio encoder name
    #[arg(long, default_value_t = {"aac".to_string()})]
    pub audio_encoder: String,
    /// audio bit rate
    #[arg(long, default_value_t = {"192k".to_string()})]
    pub audio_bit_rate: String,
    /// sample rate
    #[arg(long, default_value_t = 44100)]
    pub sample_rate: usize,
    /// number of audio channels
    #[arg(long, default_value_t = 2)]
    pub channels: usize,
    /// bytes per sample
    #[arg(long, default_value_t = 4)]
    pub bytes_per_sample: usize,

    /* Output Settings */
    /// target URL for the stream
    #[arg(short, long)]
    pub url: String,
}

impl Args {
    pub fn size(&self) -> Result<(usize, usize), String> {
        let norm = self
            .size
            .trim()
            .replace("×", "x")
            .replace("*", "x")
            .to_lowercase();
        let mut it = norm.split("x").map(str::trim);
        let width = it
            .next()
            .ok_or("Missing width")?
            .parse::<usize>()
            .map_err(|_| "Invalid width")?;
        let height = it
            .next()
            .ok_or("Missing height")?
            .parse::<usize>()
            .map_err(|_| "Invalid height")?;
        if it.next().is_some() {
            return Err("Too many dimensions".to_owned());
        }
        if width == 0 || height == 0 {
            return Err("Width and height must be greater than 0".to_owned());
        }
        Ok((width, height))
    }

    pub fn video_bit_rate(&self) -> Result<i64, String> {
        let norm = self
            .video_bit_rate
            .trim()
            .replace("k", "000")
            .replace("m", "000000")
            .replace("g", "000000000");
        norm.parse::<i64>()
            .map_err(|_| "Invalid bitrate".to_owned())
    }

    pub fn audio_bit_rate(&self) -> Result<i64, String> {
        let norm = self
            .audio_bit_rate
            .trim()
            .replace("k", "000")
            .replace("m", "000000")
            .replace("g", "000000000");
        norm.parse::<i64>()
            .map_err(|_| "Invalid audio bitrate".to_owned())
    }

    pub fn video_options(&self) -> HashMap<String, String> {
        self.video_options
            .iter()
            .map(|opt| {
                let mut parts = opt.splitn(2, '=');
                let key = parts.next().unwrap_or("").to_string();
                let value = parts.next().unwrap_or("").to_string();
                (key, value)
            })
            .collect()
    }
}
