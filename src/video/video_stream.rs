use std::ptr::null_mut;

use rusty_ffmpeg::ffi::{
    av_init_packet, av_interleaved_write_frame, av_opt_set, av_packet_rescale_ts,
    avcodec_alloc_context3, avcodec_find_encoder, avcodec_open2, avcodec_parameters_from_context,
    avcodec_receive_packet, avcodec_send_frame, avformat_new_stream, AVCodecContext,
    AVFormatContext, AVFrame, AVPacket, AVRational, AVStream, AVERROR, AVERROR_EOF,
    AVFMT_GLOBALHEADER, AV_CODEC_FLAG_GLOBAL_HEADER, AV_CODEC_ID_H264, AV_NOPTS_VALUE, EAGAIN,
};

use crate::video::{video_encoder::VideoEncoder, video_format::VideoFormat};

pub struct VideoStream<'a> {
    format_context: &'a mut AVFormatContext,
    stream: AVStream,
    codec_context: AVCodecContext,
    format: VideoFormat,
    encoder: Option<VideoEncoder>,
    index: i64,
}

impl<'a> VideoStream<'a> {
    pub fn new(format_context: &'a mut AVFormatContext, format: VideoFormat) -> Self {
        let codec = unsafe { avcodec_find_encoder(AV_CODEC_ID_H264) };
        let mut stream = unsafe { *avformat_new_stream(format_context, codec) };
        let mut codec_context = unsafe { *avcodec_alloc_context3(codec) };
        codec_context.codec_id = unsafe { *codec }.id;
        codec_context.width = format.width;
        codec_context.height = format.height;
        codec_context.bit_rate = format.bit_rate;
        codec_context.pix_fmt = format.pix_fmt;
        codec_context.time_base = AVRational {
            num: 1,
            den: format.fps,
        };
        codec_context.framerate = AVRational {
            num: format.fps,
            den: 1,
        };
        codec_context.gop_size = 60;
        codec_context.max_b_frames = 0;
        codec_context.thread_count = 0;

        unsafe {
            av_opt_set(
                codec_context.priv_data,
                b"preset".as_ptr() as *const i8,
                b"veryfast".as_ptr() as *const i8,
                0,
            );
            av_opt_set(
                codec_context.priv_data,
                b"tune".as_ptr() as *const i8,
                b"zerolatency".as_ptr() as *const i8,
                0,
            );
            av_opt_set(
                codec_context.priv_data,
                b"crf".as_ptr() as *const i8,
                b"23".as_ptr() as *const i8,
                0,
            );
        }

        if unsafe { *format_context.oformat }.flags & AVFMT_GLOBALHEADER as i32 != 0 {
            codec_context.flags |= AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }

        unsafe { avcodec_open2(&mut codec_context, codec, null_mut()) };

        unsafe { avcodec_parameters_from_context(stream.codecpar, &codec_context) };
        stream.time_base = codec_context.time_base;

        Self {
            format_context,
            stream,
            codec_context,
            format,
            encoder: None,
            index: 0,
        }
    }

    pub fn write(&mut self, frame: &mut AVFrame) {
        if frame.pts == AV_NOPTS_VALUE {
            frame.pts = self.index;
            self.index += 1;
        }

        unsafe { avcodec_send_frame(&mut self.codec_context, frame) };

        loop {
            let packet = null_mut();
            unsafe { av_init_packet(packet) };
            let mut packet: AVPacket = unsafe { *packet };
            packet.data = null_mut();
            packet.size = 0;

            let err = unsafe { avcodec_receive_packet(&mut self.codec_context, &mut packet) };
            if err == AVERROR(EAGAIN) || err == AVERROR_EOF {
                break;
            }

            unsafe {
                av_packet_rescale_ts(
                    &mut packet,
                    self.codec_context.time_base,
                    self.stream.time_base,
                )
            };
            packet.stream_index = self.stream.index;
            unsafe { av_interleaved_write_frame(self.format_context, &mut packet) };
        }
    }

    pub fn write_with_encode(&mut self, frame: &AVFrame) {
        if let Some(encoder) = &mut self.encoder {
            let mut encoded_frame = encoder.encode(frame);
            self.write(&mut encoded_frame);
        } else {
            panic!("Encoder not set for VideoStream");
        }
    }

    pub fn set_encoder(&mut self, format: VideoFormat) -> &mut Self {
        self.encoder = Some(VideoEncoder::new(format, self.format.clone()));
        self
    }

    pub fn start(&mut self) {
        // Initialize the format context for writing
    }

    pub fn stop(&mut self) {
        unsafe {
            avcodec_send_frame(&mut self.codec_context, null_mut());
            let mut packet: AVPacket = std::mem::zeroed();
            while avcodec_receive_packet(&mut self.codec_context, &mut packet) >= 0 {
                av_interleaved_write_frame(self.format_context, &mut packet);
            }
        }
    }
}
