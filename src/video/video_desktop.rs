use core::fmt;
use windows::{
    core::{factory, Interface, BOOL},
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
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIDevice,
            },
            Gdi::{EnumDisplayMonitors, HDC, HMONITOR},
        },
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
            WinRT::{
                Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
                Graphics::Capture::IGraphicsCaptureItemInterop,
            },
        },
    },
};

pub struct VideoDesktop {
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    item: GraphicsCaptureItem,
    width: usize,
    height: usize,
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
        // Initialize COM library
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // Request access to graphics capture
        GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)?.get()?;
        // Create device
        let (device, context) = unsafe {
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            (device.unwrap(), context.unwrap())
        };
        let d3d_device: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>()?) }?
                .cast()?;

        // Create item
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(hmonitor) }?;
        let size = item.Size()?;

        // Create capture frame pool and session
        let pool = Direct3D11CaptureFramePool::Create(
            &d3d_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        session.SetIsBorderRequired(false)?;
        session.SetIsCursorCaptureEnabled(true)?;
        Ok(Self {
            session,
            pool,
            device,
            context,
            item,
            width: size.Width as usize,
            height: size.Height as usize,
        })
    }

    pub fn start(&self) -> windows::core::Result<()> {
        self.session.StartCapture()
    }

    pub fn stop(&self) -> windows::core::Result<()> {
        self.session.Close()?;
        self.pool.Close()?;
        Ok(unsafe { CoUninitialize() })
    }

    pub fn get_texture(&self) -> Result<ID3D11Texture2D, Box<dyn std::error::Error>> {
        let frame = self.pool.TryGetNextFrame();
        let surface = frame?.Surface()?;
        let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
        Ok(unsafe { access.GetInterface::<ID3D11Texture2D>() }?)
    }

    pub fn staging(
        &self,
        texture: &ID3D11Texture2D,
    ) -> Result<ID3D11Texture2D, Box<dyn std::error::Error>> {
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: self.width as u32,
            Height: self.height as u32,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging_texture: Option<ID3D11Texture2D> = None;
        unsafe {
            self.device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging_texture))
        }?;
        let staging_texture = staging_texture.unwrap();
        unsafe { self.context.CopyResource(&staging_texture, texture) };
        Ok(staging_texture)
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
    pub fn context(&self) -> &ID3D11DeviceContext {
        &self.context
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
