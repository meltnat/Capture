use rusty_ffmpeg::ffi::{AV_PIX_FMT_BGRA, AV_PIX_FMT_YUV420P};
use tokio::select;
use tokio::signal::ctrl_c;
use tokio::sync::mpsc::channel;
use windows::Win32::Media::Audio::eCapture;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use std::ffi::CString;
use std::str::FromStr;

mod args;
mod audio;
mod input;
mod stream;
mod video;
mod encoder;

use crate::args::Args;
use crate::audio::audio_device::AudioDevice;
use crate::audio::audio_input::AudioInput;
use crate::audio::audio_mixer::AudioMixer;
use crate::audio::audio_stream::AudioStream;
use crate::input::Input;
use crate::stream::Stream;
use crate::video::video_desktop::VideoDesktop;
use crate::video::video_format::VideoFormat;
use crate::video::video_stream::VideoStream;

use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.audio.is_none() && args.microphone.is_none() && args.display.is_none() {
        println!("No inputs to stream, exiting...");
        return Ok(());
    }

    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

    // Stream setup
    let mut stream = Stream::new(CString::from_str(&args.url)?);

    // Audio input setup
    let mut audio_input: Option<(Box<dyn AudioInput>, AudioStream)> =
        get_audio_input(&args, &mut stream)?;

    // Video input setup
    let mut video: Option<(Box<dyn Input>, VideoStream)> = if let Some(v) = args.display {
        let index = v.unwrap_or(0);
        let mut desktop = VideoDesktop::with_index(index, None)?;
        println!("Video input: {}", desktop);
        let (width, height) = args.size()?;
        let mut video_stream: VideoStream = VideoStream::new(
            stream.context().clone(),
            VideoFormat {
                width: width as i32,
                height: height as i32,
                fps: args.fps as i32,
                bit_rate: args.video_bit_rate()?,
                pix_fmt: AV_PIX_FMT_YUV420P,
            },
            CString::from_str(&args.video_encoder)?,
            args.video_options().into_iter().map(|(k, v)| (CString::from_str(&k).unwrap(), CString::from_str(&v).unwrap())).collect()
        );
        video_stream.set_encoder(VideoFormat {
            width: desktop.width() as i32,
            height: desktop.height() as i32,
            bit_rate: desktop.bit_rate(args.fps) as i64,
            pix_fmt: AV_PIX_FMT_BGRA,
            fps: args.fps as i32,
        });
        desktop.set_fps_limit(args.fps as u64);
        Some((Box::new(desktop), video_stream))
    } else {
        None
    };

    let (sender, mut receiver) = channel::<Vec<u8>>(32);

    // Start
    stream.start()?;
    if let Some((input, _s)) = &mut audio_input {
        input.start()?;
    }
    if let Some((input, _s)) = &mut video {
        input.start(&sender)?;
    }

    // Starting Capture
    println!("Press Ctrl+C to stop the capture...");
    if let (Some((a, a_s)), Some((_v, v_s))) = (&mut audio_input, &mut video) {
        println!("Starting audio and video capture...");
        select! {
            _ = ctrl_c() => {
                println!("Ctrl+C pressed, stopping capture...");
            }
            r = a_s.stream(a) => {
                r?;
            }
            r = v_s.stream(&mut receiver) => {
                r?;
            }
        }
    } else if let Some((a, a_s)) = &mut audio_input {
        println!("Starting audio capture...");
        select! {
            _ = ctrl_c()=>{
                println!("Ctrl+C pressed, stopping capture...");
            }
            r = a_s.stream(a) => {
                r?;
            }
        }
    } else if let Some((_v, v_s)) = &mut video {
        println!("Starting video capture...");
        select! {
            _ = ctrl_c()=>{
                println!("Ctrl+C pressed, stopping capture...");
            }
            r = v_s.stream(&mut receiver) => {
                r?;
            }
        }
    }

    //cleanup
    if let Some((input, s)) = &mut audio_input {
        input.stop()?;
        s.stop();
    }
    if let Some((input, s)) = &mut video {
        input.stop()?;
        s.stop();
    }
    stream.stop();
    println!("Capture stopped");

    unsafe { CoUninitialize() };

    Ok(())
}

fn get_audio_input(
    args: &Args,
    stream: &mut Stream,
) -> Result<Option<(Box<dyn AudioInput>, AudioStream)>, Box<dyn std::error::Error>> {
    let mut render: Option<AudioDevice> = None;
    let mut mic: Option<AudioDevice> = None;
    if let Some(index) = args.audio {
        let r = if let Some(i) = index {
            AudioDevice::with_index(i, eCapture, None)?
        } else {
            AudioDevice::default_render(None)?
        };
        println!("Render: {}", r);
        render = Some(r);
    }
    if let Some(index) = args.microphone {
        let m = if let Some(i) = index {
            AudioDevice::with_index(i, eCapture, None)?
        } else {
            AudioDevice::default_capture(None)?
        };
        println!("Microphone: {}", m);
        mic = Some(m);
    }
    Ok(if render.is_some() || mic.is_some() {
        let mut audio_stream = AudioStream::new(
            stream.context().clone(),
            CString::from_str(&args.audio_encoder)?,
            args.audio_bit_rate()? as i64,
            args.sample_rate as i32,
            args.channels as i32,
        );
        if render.is_some() && mic.is_some() {
            if let (Some(render), Some(mic)) = (render, mic) {
                let (a, b) = (render.format().clone(), mic.format().clone());
                let mut mixer = AudioMixer::new(
                    Box::new(render),
                    Box::new(mic),
                    audio_stream.format().clone(),
                );
                mixer.set_resampler(a, b);
                println!("{}", mixer);
                Some((Box::new(mixer), audio_stream))
            } else {
                panic!("Both render and microphone are required for mixing audio inputs. Please provide both or none.");
            }
        } else if let Some(r) = render {
            audio_stream.set_resampler(r.format().clone());
            Some((Box::new(r), audio_stream))
        } else if let Some(m) = mic {
            audio_stream.set_resampler(m.format().clone());
            Some((Box::new(m), audio_stream))
        } else {
            None
        }
    } else {
        println!("No audio input specified, skipping audio capture.");
        None
    })
}
