use rusty_ffmpeg::ffi::AV_PIX_FMT_BGRA;
use rusty_ffmpeg::ffi::AV_PIX_FMT_YUV420P;
use windows::Win32::Media::Audio::eCapture;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

mod args;
mod audio;
mod stream;
mod video;

use crate::args::Args;
use crate::audio::audio_device::AudioDevice;
use crate::audio::audio_format::AudioFormat;
use crate::audio::audio_input::AudioInput;
use crate::audio::audio_mixer::AudioMixer;
use crate::audio::audio_stream::AudioStream;
use crate::stream::Stream;
use crate::video::video_desktop::VideoDesktop;
use crate::video::video_format::VideoFormat;
use crate::video::video_input::VideoInput;
use crate::video::video_stream::VideoStream;

use clap::Parser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.audio.is_none() && args.microphone.is_none() && args.display.is_none() {
        println!("No inputs to stream, exiting...");
        return Ok(());
    }

    // Stream setup
    let mut stream = Stream::new(&args.url);

    // Audio input setup
    let mut audio_input: Option<(Box<dyn AudioInput>, AudioStream)> =
        get_audio_input(&args, &mut stream)?;

    // Video input setup
    let mut video: Option<(Box<dyn VideoInput>, VideoStream)> = if let Some(v) = args.display {
        let index = v.unwrap_or(0);
        let desktop = VideoDesktop::with_index(index, None)?;
        println!("Video input: {}", desktop);
        let (width, height) = args.size()?;
        let mut video_stream: VideoStream = VideoStream::new(
            stream.context(),
            VideoFormat {
                width: width as i32,
                height: height as i32,
                fps: args.fps as i32,
                bit_rate: args.video_bit_rate()?,
                pix_fmt: AV_PIX_FMT_YUV420P,
            },
        );
        video_stream.set_encoder(VideoFormat {
            width: desktop.width() as i32,
            height: desktop.height() as i32,
            bit_rate: desktop.bit_rate(args.fps) as i64,
            pix_fmt: AV_PIX_FMT_BGRA,
            fps: args.fps as i32,
        });
        Some((Box::new(desktop), video_stream))
    } else {
        None
    };

    // Ctrl+C handling
    let runnning = Arc::new(AtomicBool::new(true));
    let runnning_clone = runnning.clone();

    ctrlc::set_handler(move || {
        runnning_clone.clone().store(false, Ordering::SeqCst);
    })?;

    println!("Press Ctrl+C to stop the capture...");

    // Start
    if let Some((input, s)) = &mut audio_input {
        input.start()?;
        s.start();
    }
    if let Some((input, s)) = &mut video {
        input.start()?;
        s.start();
    }

    // let duration = Duration::from_millis(500ms * buffer_size / sample_rate);
    let duration = Duration::from_millis(500);

    // Starting Capture
    if let (Some((a, a_s)), Some((v, v_s))) = (&mut audio_input, &mut video) {
        while runnning.load(Ordering::SeqCst) {
            v_s.write_with_encode(&mut v.capture()?);
            a_s.write_with_resample(&a.capture()?);
            thread::sleep(duration);
        }
    } else if let Some((a, a_s)) = &mut audio_input {
        while runnning.load(Ordering::SeqCst) {
            a_s.write_with_resample(&a.capture()?);
            thread::sleep(duration);
        }
    } else if let Some((v, v_s)) = &mut video {
        while runnning.load(Ordering::SeqCst) {
            match v.capture() {
                Ok(frame) => v_s.write_with_encode(&frame),
                Err(e) => continue,
            }
            thread::sleep(duration);
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

    Ok(())
}

fn get_audio_input(
    args: &Args,
    stream: &mut Stream,
) -> Result<Option<(Box<dyn AudioInput>, AudioStream)>, Box<dyn std::error::Error>> {
    let mut render: Option<AudioDevice> = None;
    let mut mic: Option<AudioDevice> = None;
    if let Some(index) = args.audio {
        let mut r = if let Some(i) = index {
            AudioDevice::with_index(i, eCapture, None)?
        } else {
            AudioDevice::default_render(None)?
        };
        r.set_volume(args.audio_volume);
        println!("Render: {}", r);
        render = Some(r);
    }
    if let Some(index) = args.microphone {
        let mut m = if let Some(i) = index {
            AudioDevice::with_index(i, eCapture, None)?
        } else {
            AudioDevice::default_capture(None)?
        };
        m.set_volume(args.mic_volume);
        println!("Microphone: {}", m);
        mic = Some(m);
    }
    Ok(if render.is_some() || mic.is_some() {
        let mut audio_stream = AudioStream::new(
            stream.context(),
            AudioFormat::new(
                args.audio_bit_rate()? as i64,
                args.sample_rate as i32,
                args.channels as i32,
                args.bytes_per_sample as i32,
            ),
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
                audio_stream.set_resampler(mixer.format().clone());
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
