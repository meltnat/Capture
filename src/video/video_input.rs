use std::{os::raw::c_void, ptr::null_mut};

use rusty_ffmpeg::ffi::{av_frame_alloc, memcpy, AVFrame, AV_PIX_FMT_BGRA};
use windows::Win32::Graphics::Direct3D11::D3D11_MAP_READ;

use crate::video::video_desktop::VideoDesktop;

pub trait VideoInput {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn capture(&self) -> Result<AVFrame, Box<dyn std::error::Error>>;
    fn stop(&self) -> Result<(), Box<dyn std::error::Error>>;
}

impl VideoInput for VideoDesktop {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.start()?;
        Ok(())
    }

    fn capture(&self) -> Result<AVFrame, Box<dyn std::error::Error>> {
        let texture = self.staging(&self.get_texture()?)?;

        let mut frame = unsafe { *av_frame_alloc() };
        frame.width = self.width() as i32;
        frame.height = self.height() as i32;
        frame.format = AV_PIX_FMT_BGRA;

        let map = null_mut();
        unsafe {
            self.context()
                .Map(&texture, 0, D3D11_MAP_READ, 0, Some(map))
        }?;
        let map = unsafe { *map };

        let mut source = map.pData;
        let src_pitch = map.RowPitch as usize;
        let mut d = frame.data[0] as *mut c_void;
        let copy = self.width() as u64 * 4;
        let linesize = frame.linesize[0] as usize;

        for _ in 0..self.height() {
            unsafe { memcpy(d, source, copy) };
            d = unsafe { d.add(linesize) };
            source = unsafe { source.add(src_pitch) };
        }

        unsafe { self.context().Unmap(&texture, 0) };

        Ok(frame)
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.stop()?;
        Ok(())
    }
}
