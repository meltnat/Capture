use std::{
    collections::HashMap,
    ffi::CString,
    ptr::{null_mut, NonNull},
};

use rusty_ffmpeg::ffi::{
    av_frame_unref, av_interleaved_write_frame, av_opt_set, av_packet_rescale_ts, av_packet_unref,
    avcodec_alloc_context3, avcodec_open2, avcodec_parameters_from_context, avcodec_receive_packet,
    avcodec_send_frame, avformat_new_stream, AVCodecContext, AVFormatContext, AVFrame, AVPacket,
    AVRational, AVStream, AVERROR, AVERROR_EOF, AVFMT_GLOBALHEADER, AV_CODEC_FLAG_GLOBAL_HEADER,
    EAGAIN,
};

use tokio::sync::mpsc::Receiver;

use crate::{
    encoder::find_codec,
    video::{video_encoder::VideoEncoder, video_format::VideoFormat},
};

pub struct VideoStream {
    format_context: NonNull<AVFormatContext>,
    stream: NonNull<AVStream>,
    codec_context: NonNull<AVCodecContext>,
    format: VideoFormat,
    encoder: Option<VideoEncoder>,
    start: u128,
}

impl VideoStream {
    pub fn new(
        mut format_context: NonNull<AVFormatContext>,
        format: VideoFormat,
        encoder: CString,
        options: HashMap<CString, CString>,
    ) -> Self {
        let codec = find_codec(&encoder, true);
        if codec.is_none() {
            panic!("Failed to find encoder: {}", encoder.to_string_lossy());
        }
        let codec = codec.unwrap();
        let mut codec_context =
            NonNull::new(unsafe { avcodec_alloc_context3(codec.as_ptr()) }).unwrap();
        unsafe { codec_context.as_mut() }.codec_id = unsafe { codec.as_ref() }.id;
        unsafe { codec_context.as_mut() }.width = format.width;
        unsafe { codec_context.as_mut() }.height = format.height;
        unsafe { codec_context.as_mut() }.bit_rate = format.bit_rate;
        unsafe { codec_context.as_mut() }.pix_fmt = format.pix_fmt;
        unsafe { codec_context.as_mut() }.time_base = AVRational {
            num: 1,
            den: format.fps,
        };
        unsafe { codec_context.as_mut() }.framerate = AVRational {
            num: format.fps,
            den: 1,
        };
        unsafe { codec_context.as_mut() }.gop_size = format.fps;
        unsafe { codec_context.as_mut() }.max_b_frames = 0;
        unsafe { codec_context.as_mut() }.thread_count = 0;

        options.iter().for_each(|(key, value)| unsafe {
            av_opt_set(
                codec_context.as_mut().priv_data,
                key.as_ptr() as *const i8,
                value.as_ptr() as *const i8,
                0,
            );
        });

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
            format,
            encoder: None,
            start: 0,
        }
    }

    pub fn write(&mut self, ts: u128, frame: &mut AVFrame) {
        if self.start == 0 {
            self.start = ts;
        }
        frame.time_base = unsafe { self.stream.as_ref() }.time_base;
        let pts = (ts - self.start) * self.format.fps as u128 / 1_000_000_000;
        frame.pts = pts as i64;

        frame.extended_data = frame.data.as_mut_ptr();

        let err = unsafe { avcodec_send_frame(self.codec_context.as_mut(), frame) };
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
        unsafe { av_frame_unref(frame) };
    }

    pub fn set_encoder(&mut self, format: VideoFormat) -> &mut Self {
        self.encoder = Some(VideoEncoder::new(format, self.format.clone()));
        self
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

    pub async fn stream(&mut self, receiver: &mut Receiver<Vec<u8>>) -> Result<(), String> {
        let mut encoder = self.encoder.take();
        if let Some(encoder) = &mut encoder {
            while let Some(data) = receiver.recv().await {
                let (ts, frame_data) = split_time(&data);
                let mut frame = encoder.from_bytes(frame_data);
                self.write(ts, &mut frame);
            }
        } else {
            while let Some(data) = receiver.recv().await {
                let (ts, frame_data) = split_time(&data);
                self.write(ts, &mut self.format.from_bytes(frame_data));
            }
        }
        Ok(())
    }
}

fn split_time(data: &[u8]) -> (u128, &[u8]) {
    (
        u128::from_le_bytes(data[..16].try_into().unwrap()),
        &data[16..],
    )
}
