use std::{
    ffi::{c_void, CString},
    ptr::{copy_nonoverlapping, null_mut, NonNull},
    slice::from_raw_parts,
    time::{SystemTime, UNIX_EPOCH},
};

use rusty_ffmpeg::ffi::{
    av_frame_alloc, av_frame_get_buffer, av_frame_make_writable, av_interleaved_write_frame,
    av_packet_rescale_ts, av_packet_unref, avcodec_alloc_context3, avcodec_get_supported_config,
    avcodec_open2, avcodec_parameters_from_context, avcodec_receive_packet, avcodec_send_frame,
    avformat_new_stream, AVCodec, AVCodecContext, AVFormatContext, AVFrame, AVPacket, AVRational,
    AVSampleFormat, AVStream, AVERROR, AVERROR_EOF, AVFMT_GLOBALHEADER,
    AV_CODEC_CONFIG_SAMPLE_FORMAT, AV_CODEC_FLAG_GLOBAL_HEADER, AV_SAMPLE_FMT_FLTP,
    AV_SAMPLE_FMT_NONE, EAGAIN,
};

use crate::{
    audio::{audio_format::AudioFormat, audio_input::AudioInput, audio_resampler::AudioResampler},
    encoder::find_codec,
};
use tokio::task::yield_now; // ループ内で他タスクに制御を戻すため

pub struct AudioStream {
    format_context: NonNull<AVFormatContext>,
    stream: NonNull<AVStream>,
    codec_context: NonNull<AVCodecContext>,
    format: AudioFormat,
    resampler: Option<AudioResampler>,
    start: u128,
    next: u128,
    buffer: Vec<Vec<u8>>,
}

impl AudioStream {
    pub fn new(
        mut format_context: NonNull<AVFormatContext>,
        encoder: CString,
        bit_rate: i64,
        sample_rate: i32,
        channels: i32,
    ) -> Self {
        let codec = find_codec(&encoder, true);
        if codec.is_none() {
            panic!("Failed to find encoder: {}", encoder.to_string_lossy());
        }
        let codec = codec.unwrap();
        let mut codec_context =
            NonNull::new(unsafe { avcodec_alloc_context3(codec.as_ptr()) }).unwrap();
        unsafe { codec_context.as_mut() }.codec_id = unsafe { codec.as_ref() }.id;
        unsafe { codec_context.as_mut() }.bit_rate = bit_rate;
        unsafe { codec_context.as_mut() }.sample_rate = sample_rate;
        unsafe { codec_context.as_mut() }.ch_layout = AudioFormat::channel_layout_default(channels);
        unsafe { codec_context.as_mut() }.sample_fmt =
            select_sample_format(codec.as_ptr(), unsafe { codec_context.as_ref() });
        unsafe { codec_context.as_mut() }.time_base = AVRational {
            num: 1,
            den: sample_rate,
        };
        unsafe { codec_context.as_mut() }.thread_count = 0;

        if unsafe { *format_context.as_ref().oformat }.flags & AVFMT_GLOBALHEADER as i32 != 0 {
            unsafe { codec_context.as_mut() }.flags |= AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }

        let err = unsafe { avcodec_open2(codec_context.as_mut(), codec.as_ptr(), null_mut()) };
        if err < 0 {
            panic!("Failed to open codec: {}", err);
        }

        let mut stream =
            NonNull::new(unsafe { avformat_new_stream(format_context.as_mut(), codec.as_ptr()) })
                .unwrap();
        unsafe {
            avcodec_parameters_from_context(stream.as_ref().codecpar, codec_context.as_ref())
        };
        unsafe { stream.as_mut() }.time_base = unsafe { codec_context.as_ref() }.time_base;
        Self {
            format_context,
            stream,
            codec_context,
            format: AudioFormat::from(unsafe { codec_context.as_ref() }),
            resampler: None,
            start: 0,
            next: 0,
            buffer: (0..channels as usize).map(|_| Vec::new()).collect(),
        }
    }

    pub fn push(&mut self, frame: &AVFrame) -> Option<AVFrame> {
        let input_plane_bytes = (frame.nb_samples * self.format.bytes_per_sample) as usize;
        self.buffer.iter_mut().enumerate().for_each(|(ch, buf)| {
            let src = unsafe { from_raw_parts(frame.data[ch], input_plane_bytes) };
            buf.extend_from_slice(src);
        });
        let required_plane_bytes = (unsafe { self.codec_context.as_ref() }.frame_size
            * self.format.bytes_per_sample) as usize;
        if self.buffer[0].len() < required_plane_bytes {
            return None;
        }
        let mut frame = unsafe { *av_frame_alloc() };
        frame.format = self.format.sample_format as i32;
        frame.sample_rate = self.format.sample_rate;
        frame.ch_layout = self.format.get_channel_layout_default();
        frame.nb_samples = unsafe { self.codec_context.as_ref() }.frame_size;
        if unsafe { av_frame_get_buffer(&mut frame, 0) } < 0 {
            panic!("Failed to get frame buffer");
        }
        if unsafe { av_frame_make_writable(&mut frame) } < 0 {
            panic!("Failed to make frame writable");
        }
        self.buffer.iter_mut().enumerate().for_each(|(ch, buf)| {
            let src = &buf[..required_plane_bytes];
            unsafe { copy_nonoverlapping(src.as_ptr(), frame.data[ch], required_plane_bytes) };
            buf.drain(..required_plane_bytes);
        });
        Some(frame)
    }

    pub fn write(&mut self, ts: u128, frame: &AVFrame) {
        let frame = self.push(frame);
        if frame.is_none() {
            return;
        }
        let mut frame = frame.unwrap();
        if self.start == 0 {
            self.start = ts;
        }
        frame.time_base = unsafe { self.stream.as_ref() }.time_base;
        let pts = (ts - self.start) * self.format.sample_rate as u128 / 1_000_000_000;
        if pts > self.next {
            self.next = pts;
        }
        frame.pts = self.next as i64;
        self.next += frame.nb_samples as u128;

        let err = unsafe { avcodec_send_frame(self.codec_context.as_mut(), &mut frame) };
        if err < 0 {
            panic!("Failed to send frame: {}", err);
        }
        loop {
            let mut packet: AVPacket = unsafe { std::mem::zeroed() };

            let err = unsafe { avcodec_receive_packet(self.codec_context.as_mut(), &mut packet) };
            if err == AVERROR(EAGAIN) || err == AVERROR_EOF {
                break;
            }

            packet.stream_index = unsafe { self.stream.as_ref() }.index;
            unsafe {
                av_packet_rescale_ts(
                    &mut packet,
                    self.codec_context.as_ref().time_base,
                    self.stream.as_ref().time_base,
                )
            };
            unsafe { av_interleaved_write_frame(self.format_context.as_mut(), &mut packet) };
            unsafe { av_packet_unref(&mut packet) };
        }
    }

    pub fn stop(&mut self) {
        unsafe { avcodec_send_frame(self.codec_context.as_mut(), null_mut()) };
        loop {
            let mut packet: AVPacket = unsafe { std::mem::zeroed() };

            let err = unsafe { avcodec_receive_packet(self.codec_context.as_mut(), &mut packet) };
            if err == AVERROR(EAGAIN) || err == AVERROR_EOF {
                break;
            }

            packet.stream_index = unsafe { self.stream.as_ref() }.index;
            unsafe {
                av_packet_rescale_ts(
                    &mut packet,
                    self.codec_context.as_ref().time_base,
                    self.stream.as_ref().time_base,
                )
            };
            unsafe { av_interleaved_write_frame(self.format_context.as_mut(), &mut packet) };
            unsafe { av_packet_unref(&mut packet) };
        }
    }

    pub fn set_resampler(&mut self, format: AudioFormat) -> &mut Self {
        self.resampler = Some(AudioResampler::new(format, self.format.clone()));
        self
    }
    pub fn format(&self) -> &AudioFormat {
        &self.format
    }

    pub async fn stream(
        &mut self,
        input: &mut Box<dyn AudioInput>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut resampler = self.resampler.take();
        if let Some(resampler) = &mut resampler {
            loop {
                let data = input.capture();
                if let Err(e) = data {
                    eprintln!("Failed to capture audio: {}", e);
                    continue;
                }
                let data = data?;
                if data.is_empty() {
                    yield_now().await;
                    continue;
                }
                let frame = resampler.from_bytes(&data);
                self.write(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos(),
                    &frame,
                );
                yield_now().await;
            }
        } else {
            loop {
                let data = input.capture();
                if let Err(e) = data {
                    eprintln!("Failed to capture audio: {}", e);
                    continue;
                }
                let data = data?;
                if data.is_empty() {
                    yield_now().await;
                    continue;
                }
                self.write(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos(),
                    &self.format.from_bytes(&data),
                );
                yield_now().await;
            }
        }
    }
}

fn select_sample_format(
    codec: *const AVCodec,
    codec_context: *const AVCodecContext,
) -> AVSampleFormat {
    let mut out_format: *const c_void = null_mut();
    unsafe {
        avcodec_get_supported_config(
            codec_context,
            codec,
            AV_CODEC_CONFIG_SAMPLE_FORMAT,
            0,
            &mut out_format,
            null_mut(),
        )
    };
    if out_format.is_null() {
        return AV_SAMPLE_FMT_FLTP;
    }
    let sample_format: AVSampleFormat = unsafe { *(out_format as *const AVSampleFormat) };
    if sample_format == AV_SAMPLE_FMT_NONE {
        return AV_SAMPLE_FMT_FLTP;
    }
    sample_format
}
