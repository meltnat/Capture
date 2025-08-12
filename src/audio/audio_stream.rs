use std::{ffi::c_void, ptr::null_mut};

use rusty_ffmpeg::ffi::{
    av_init_packet, av_interleaved_write_frame, av_packet_rescale_ts, avcodec_alloc_context3,
    avcodec_find_encoder, avcodec_get_supported_config, avcodec_open2,
    avcodec_parameters_from_context, avcodec_receive_packet, avcodec_send_frame,
    avformat_new_stream, AVCodec, AVCodecContext, AVFormatContext, AVFrame, AVRational,
    AVSampleFormat, AVStream, AVERROR, AVERROR_EOF, AVFMT_GLOBALHEADER,
    AV_CODEC_CONFIG_SAMPLE_FORMAT, AV_CODEC_FLAG_GLOBAL_HEADER, AV_CODEC_ID_AAC, AV_NOPTS_VALUE,
    AV_SAMPLE_FMT_FLTP, AV_SAMPLE_FMT_NONE, EAGAIN,
};

use crate::audio::{audio_format::AudioFormat, audio_resampler::AudioResampler};

pub struct AudioStream {
    format_context: *mut AVFormatContext,
    stream: AVStream,
    codec_context: AVCodecContext,
    format: AudioFormat,
    resampler: Option<AudioResampler>,
    index: i64,
}

impl AudioStream {
    pub fn new(format_context: *mut AVFormatContext, format: AudioFormat) -> Self {
        let codec = unsafe { avcodec_find_encoder(AV_CODEC_ID_AAC) };
        let mut stream = unsafe { *avformat_new_stream(format_context, codec) };
        let mut codec_context = unsafe { *avcodec_alloc_context3(codec) };
        codec_context.codec_id = unsafe { *codec }.id;
        codec_context.bit_rate = format.bit_rate;
        codec_context.sample_rate = format.sample_rate;
        codec_context.ch_layout = format.channel_layout_default();
        codec_context.sample_fmt = select_sample_format(codec, &codec_context);
        codec_context.time_base = AVRational {
            num: 1,
            den: format.sample_rate,
        };
        codec_context.thread_count = 0;

        if unsafe { *(*format_context).oformat }.flags & AVFMT_GLOBALHEADER as i32 != 0 {
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
            resampler: None,
            index: 0,
        }
    }

    pub fn write(&mut self, frame: &mut AVFrame) {
        if frame.pts == AV_NOPTS_VALUE {
            frame.pts = self.index;
        }
        self.index = frame.pts + frame.nb_samples as i64;
        unsafe { avcodec_send_frame(&mut self.codec_context, frame) };
        loop {
            let packet = null_mut();
            unsafe { av_init_packet(packet) };
            let mut packet = unsafe { *packet };
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

    pub fn write_with_resample(&mut self, frame: &AVFrame) {
        if let Some(resampler) = &self.resampler {
            let mut resampled_frame = resampler.resample(frame);
            self.write(&mut resampled_frame);
        }
    }

    pub fn start(&self) {}
    pub fn stop(&self) {}

    pub fn set_resampler(&mut self, format: AudioFormat) -> &mut Self {
        self.resampler = Some(AudioResampler::new(format, self.format.clone()));
        self
    }
    pub fn format(&self) -> &AudioFormat {
        &self.format
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
