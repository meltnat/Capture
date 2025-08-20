use std::{
    ffi::{CStr, CString},
    ptr::NonNull,
};

use rusty_ffmpeg::ffi::{
    av_codec_is_decoder, av_codec_is_encoder, av_codec_iterate, avcodec_descriptor_get_by_name,
    avcodec_find_decoder, avcodec_find_decoder_by_name, avcodec_find_encoder,
    avcodec_find_encoder_by_name, AVCodec,
};

pub fn find_codec(name: &CString, encoder: bool) -> Option<NonNull<AVCodec>> {
    let mut codec = if encoder {
        unsafe { avcodec_find_encoder_by_name(name.as_ptr()) }
    } else {
        unsafe { avcodec_find_decoder_by_name(name.as_ptr()) }
    };
    if codec.is_null() {
        let desc = unsafe { avcodec_descriptor_get_by_name(name.as_ptr()) };
        if !desc.is_null() {
            codec = if encoder {
                unsafe { avcodec_find_encoder((*desc).id) }
            } else {
                unsafe { avcodec_find_decoder((*desc).id) }
            };
        }
    }
    if codec.is_null() {
        eprintln!("Codec not found: {}", name.to_string_lossy());
        eprintln!("Available codecs:");
        let mut opaque = std::ptr::null_mut();
        while let Some(codec) =
            NonNull::new(unsafe { av_codec_iterate(&mut opaque) } as *mut AVCodec)
        {
            if (unsafe { av_codec_is_encoder(codec.as_ptr()) } != 0 && encoder)
                || (unsafe { av_codec_is_decoder(codec.as_ptr()) } != 0 && !encoder)
            {
                eprintln!(
                    " - {}",
                    unsafe { CStr::from_ptr(codec.as_ref().name) }.to_string_lossy()
                );
            }
        }
        return None;
    }
    NonNull::new(codec as *mut AVCodec)
}
