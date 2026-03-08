use core::fmt;
use std::sync::{Arc, Mutex};
use windows::{
    core::{factory, Error, Interface, Ref, BOOL},
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind,
            GraphicsCaptureItem, GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    Win32::{
        Foundation::{HMODULE, LPARAM, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::IDXGIDevice,
            Gdi::{EnumDisplayMonitors, HDC, HMONITOR},
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    },
};

use crate::keylog::KeyLog;

pub struct VideoDesktop {
    session: Option<GraphicsCaptureSession>,
    device: Option<ID3D11Device>,
    pool: Option<Direct3D11CaptureFramePool>,
    item: GraphicsCaptureItem,
    width: usize,
    height: usize,
    target_fps: u64,
    last_instant: Arc<Mutex<std::time::Instant>>,
    pub keylog: Option<Arc<Mutex<KeyLog>>>,
}

impl VideoDesktop {
    pub fn get_monitors() -> Result<Vec<HMONITOR>, Box<dyn std::error::Error>> {
        let mut hmonitors: Vec<HMONITOR> = vec![];
        if unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(monitor_enum_proc),
                LPARAM(&mut hmonitors as *mut _ as isize),
            )
        }
        .as_bool()
        {
            Ok(hmonitors)
        } else {
            Err("Failed to enumerate monitors".into())
        }
    }

    pub fn with_index(
        index: usize,
        hmoniros: Option<Vec<HMONITOR>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let monitors = hmoniros.unwrap_or(Self::get_monitors()?);
        if index >= monitors.len() {
            return Err("Monitor index out of bounds".into());
        }
        Ok(Self::new(monitors[index])?)
    }

    pub fn new(hmonitor: HMONITOR) -> Result<Self, Box<dyn std::error::Error>> {
        // Request access to graphics capture
        GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)?.get()?;
        // Create item
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(hmonitor) }?;
        let size = item.Size()?;
        Ok(Self {
            device: None,
            pool: None,
            session: None,
            item,
            width: size.Width as usize,
            height: size.Height as usize,
            target_fps: 0,
            last_instant: Arc::new(Mutex::new(std::time::Instant::now())),
            keylog: None,
        })
    }

    pub fn set_fps_limit(&mut self, fps: u64) -> &mut Self {
        self.target_fps = fps;
        self
    }

    pub fn set_keylog(&mut self, keylog: Arc<Mutex<KeyLog>>) -> &mut Self {
        self.keylog = Some(keylog);
        self
    }

    pub fn start<F>(&mut self, mut callback: F) -> windows::core::Result<()>
    where
        F: FnMut(&Direct3D11CaptureFramePool) -> Result<(), Error> + Send + 'static,
    {
        let mut d3d_device: Option<ID3D11Device> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                None,
            )
        }?;
        let device = unsafe {
            CreateDirect3D11DeviceFromDXGIDevice(
                &d3d_device.as_ref().unwrap().cast::<IDXGIDevice>()?,
            )
        }?
        .cast::<IDirect3DDevice>()?;
        // Create capture frame pool and session
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            self.item.Size()?,
        )?;
        // FPS制限付きフレーム到着ハンドラを生成
        let fps = self.target_fps;
        let last_instant = Arc::clone(&self.last_instant);
        let handler =
            TypedEventHandler::new(move |pool: Ref<'_, Direct3D11CaptureFramePool>, _| {
                let pool = pool.as_ref().unwrap();
                if fps > 0 {
                    let desired_interval =
                        std::time::Duration::from_nanos(1_000_000_000u64 / fps as u64);
                    let now = std::time::Instant::now();
                    if let Ok(mut guard) = last_instant.lock() {
                        let last = *guard;
                        if now.duration_since(last) < desired_interval {
                            let _ = pool.TryGetNextFrame();
                            return Ok(());
                        }
                        *guard = now;
                    } else {
                        panic!("Failed to lock last_instant mutex");
                    }
                }
                if let Err(err) = callback(pool) {
                    eprintln!("Error occurred while processing frame: {}", err);
                }
                Ok(())
            });
        pool.FrameArrived(&handler)?;

        let session = pool.CreateCaptureSession(&self.item)?;
        session.SetIsBorderRequired(false)?;
        session.SetIsCursorCaptureEnabled(true)?;
        session.StartCapture()?;
        self.device = d3d_device;
        self.session = Some(session);
        self.pool = Some(pool);
        Ok(())
    }

    pub fn get_texture(
        pool: &Direct3D11CaptureFramePool,
    ) -> windows::core::Result<ID3D11Texture2D> {
        let frame = pool.TryGetNextFrame()?;
        let surface = frame.Surface()?;
        let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
        unsafe { access.GetInterface::<ID3D11Texture2D>() }
    }

    pub fn staging(texture: ID3D11Texture2D) -> windows::core::Result<ID3D11Texture2D> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;

        let device = unsafe { texture.GetDevice() }?;
        let mut staging = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut staging)) }?;
        let context = unsafe { device.GetImmediateContext() }?;
        let staging = staging.ok_or_else(|| windows::core::Error::from_win32())?;
        unsafe { context.CopyResource(&staging, &texture) };
        Ok(staging)
    }

    pub fn to_bytes(texture: ID3D11Texture2D) -> windows::core::Result<Vec<u8>> {
        let context = unsafe { texture.GetDevice()?.GetImmediateContext() }?;

        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        let height = desc.Height as usize;
        let row = desc.Width as usize * 4;
        let mut data = vec![0u8; height * row];
        let mut dst = data.as_mut_ptr();

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { context.Map(&texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }?;

        let pitch = mapped.RowPitch as usize;
        let mut src = mapped.pData as *const u8;

        for _ in 0..height {
            unsafe { dst.copy_from_nonoverlapping(src, row) };
            src = unsafe { src.add(pitch) };
            dst = unsafe { dst.add(row) };
        }

        unsafe { context.Unmap(&texture, 0) };
        Ok(data)
    }

    pub fn stop(&self) -> windows::core::Result<()> {
        if let Some(session) = &self.session {
            session.Close()?;
        }
        if let Some(pool) = &self.pool {
            pool.Close()?;
        }
        Ok(())
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn bit_rate(&self, fps: usize) -> usize {
        self.width() * self.height() * 32 * fps
    }
}

unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprect: *mut RECT,
    _lparam: LPARAM,
) -> BOOL {
    let monitors = unsafe { &mut *(_lparam.0 as *mut Vec<HMONITOR>) };
    monitors.push(hmonitor);
    BOOL::from(true)
}

impl fmt::Display for VideoDesktop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "VideoDesktop {}x{} - {}",
            self.width,
            self.height,
            self.item.DisplayName().unwrap(),
        )?;
        Ok(())
    }
}
