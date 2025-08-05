use std::time::Instant;

use ffmpeg_next::{
    self as ffmpeg, channel_layout, codec, encoder, format, packet, software, util, Packet,
};
use windows::core::{Interface, BOOL, PWSTR};
use windows::Graphics::Capture::Direct3D11CaptureFramePool;
use windows::Graphics::Capture::GraphicsCaptureAccess;
use windows::Graphics::Capture::GraphicsCaptureAccessKind;
use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::DisplayId;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::D3D11CreateDevice;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Direct3D11::D3D11_CPU_ACCESS_READ;
use windows::Win32::Graphics::Direct3D11::D3D11_CREATE_DEVICE_BGRA_SUPPORT;
use windows::Win32::Graphics::Direct3D11::D3D11_MAPPED_SUBRESOURCE;
use windows::Win32::Graphics::Direct3D11::D3D11_MAP_READ;
use windows::Win32::Graphics::Direct3D11::D3D11_SDK_VERSION;
use windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC;
use windows::Win32::Graphics::Direct3D11::D3D11_USAGE_STAGING;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient3, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;

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

fn get_monitors() -> Result<Vec<HMONITOR>, windows::core::Error> {
    let mut hmonitors: Vec<HMONITOR> = vec![];
    if !unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut hmonitors as *mut _ as isize),
        )
        .as_bool()
    } {
        return Err(windows::core::Error::from_win32());
    }
    Ok(hmonitors)
}

fn get_device(d3d_device: &ID3D11Device) -> Result<IDirect3DDevice, windows::core::Error> {
    let dxgi_device: IDXGIDevice = d3d_device.cast()?;
    let device: IDirect3DDevice =
        unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }?.cast()?;
    Ok(device)
}

fn get_image(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    pool: &Direct3D11CaptureFramePool,
) -> Result<Vec<u8>, windows::core::Error> {
    let frame = pool.TryGetNextFrame()?;
    let surface = frame.Surface()?;
    let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
    let texture = unsafe { access.GetInterface::<ID3D11Texture2D>() }?;
    let mut desc: D3D11_TEXTURE2D_DESC = unsafe { std::mem::zeroed() };
    unsafe { texture.GetDesc(&mut desc) };

    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: desc.Width,
        Height: desc.Height,
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
    let mut _staging_texture: Option<ID3D11Texture2D> = None;
    unsafe { device.CreateTexture2D(&staging_desc, None, Some(&mut _staging_texture)) }?;
    let staging_texture = _staging_texture.unwrap();
    unsafe { context.CopyResource(&staging_texture, &texture) };

    let mut mapped: D3D11_MAPPED_SUBRESOURCE = unsafe { std::mem::zeroed() };

    let mut image_data = vec![0u8; (desc.Width * desc.Height * 4) as usize];

    unsafe {
        context.Map(
            &staging_texture,
            0,
            D3D11_MAP_READ,
            0,
            Some(&mut mapped as *mut D3D11_MAPPED_SUBRESOURCE),
        )
    }?;

    for y in 0..desc.Height {
        let src = unsafe {
            std::slice::from_raw_parts(
                (mapped.pData as *const u8).add((y * mapped.RowPitch) as usize),
                (desc.Width * 4) as usize,
            )
        };
        let dst_start = (y * desc.Width * 4) as usize;
        image_data[dst_start..(dst_start + (desc.Width * 4) as usize)].copy_from_slice(src);
    }

    unsafe { context.Unmap(&staging_texture, 0) };
    Ok(image_data)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

    // もにーたしゅとく
    let hmonitors: Vec<HMONITOR> = get_monitors()?;
    println!("Monitors found: {}", hmonitors.len());

    let hmonitor = hmonitors[0];

    // Capture access request
    GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless)?.get()?;

    // Create a capture item for the monitor
    let item = GraphicsCaptureItem::TryCreateFromDisplayId(DisplayId {
        Value: hmonitor.0 as u64,
    })?;
    println!(
        "Capture item created for monitor: {:?} {}x{}",
        item.DisplayName(),
        item.Size()?.Width,
        item.Size()?.Height
    );

    // Create a Direct3D11 device
    let mut _device: Option<ID3D11Device> = None;
    let mut _context: Option<ID3D11DeviceContext> = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut _device),
            None,
            Some(&mut _context),
        )
    }?;
    let d11_device = _device.unwrap();
    let d11_context = _context.unwrap();

    let pool = Direct3D11CaptureFramePool::Create(
        &get_device(&d11_device)?,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        item.Size()?,
    )?;
    let session = pool.CreateCaptureSession(&item)?;
    session.SetIsBorderRequired(false)?;

    let imm_devices: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
    let imm_device = unsafe { imm_devices.GetDefaultAudioEndpoint(eRender, eConsole) }?;

    println!(
        "Default audio endpoint: {:?} > {}",
        unsafe { imm_device.GetId() }?,
        unsafe {
            PWSTR(
                imm_device
                    .OpenPropertyStore(windows::Win32::System::Com::STGM(0))?
                    .GetValue(&PKEY_Device_FriendlyName)?
                    .Anonymous
                    .Anonymous
                    .Anonymous
                    .pwszVal
                    .0,
            )
            .to_string()
        }?
    );

    let audio_client: IAudioClient3 = unsafe { imm_device.Activate(CLSCTX_ALL, None) }?;
    let mix_format = unsafe { *audio_client.GetMixFormat()? };
    let wave_format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: mix_format.nChannels,
        nSamplesPerSec: mix_format.nSamplesPerSec,
        nAvgBytesPerSec: mix_format.nAvgBytesPerSec,
        nBlockAlign: mix_format.nBlockAlign,
        wBitsPerSample: mix_format.wBitsPerSample,
        cbSize: 0,
    };

    let sample_rate: i32 = wave_format.nSamplesPerSec as i32;

    unsafe {
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            5000000,
            0,
            &wave_format,
            None,
        )
    }?;

    const OUT_WIDTH: u32 = 1920;
    const OUT_HEIGHT: u32 = 1080;

    let capture_client: IAudioCaptureClient = unsafe { audio_client.GetService() }?;

    ffmpeg::init()?;
    println!("FFmpeg initialized");
    let mut output_ctx = format::output_as(&"rtmp://localhost/live/test", "flv")?;
    let global_header = output_ctx
        .format()
        .flags()
        .contains(format::flag::Flags::GLOBAL_HEADER);

    let codec = codec::encoder::find_by_name("h264_nvenc").unwrap();
    let mut video_stream = output_ctx.add_stream(codec)?;
    let video_context = codec::context::Context::from_parameters(video_stream.parameters())?;
    let mut video = video_context.encoder().video()?;
    video_stream.set_parameters(&video);
    video.set_height(OUT_HEIGHT);
    video.set_width(OUT_WIDTH);
    video.set_aspect_ratio((16, 9));
    video.set_format(format::Pixel::YUV420P);
    video.set_frame_rate(Some((30, 1)));
    video.set_time_base((1, 30));
    video_stream.set_time_base((1, 30));
    if global_header {
        video.set_flags(codec::flag::Flags::GLOBAL_HEADER);
    }
    let mut video_encoder = video.open_as(codec)?;
    video_stream.set_parameters(&video_encoder);

    // let codec = encoder::find(codec::Id::AAC).unwrap();
    // let mut audio_stream = output_ctx.add_stream(codec)?;
    // let audio_context = codec::context::Context::from_parameters(audio_stream.parameters())?;
    // let mut audio = audio_context.encoder().audio()?;
    // audio_stream.set_parameters(&audio);
    // audio.set_channel_layout(channel_layout::ChannelLayout::STEREO);
    // audio.set_format(format::Sample::F32(format::sample::Type::Planar));
    // audio.set_rate(sample_rate);
    // let mut audio_encoder = audio.open_as(codec)?;
    // audio_stream.set_parameters(&audio_encoder);

    // let mut raw_audio = util::frame::Audio::empty();
    // raw_audio.set_format(audio_encoder.format());
    // raw_audio.set_channel_layout(audio_encoder.channel_layout());
    // raw_audio.set_rate(audio_encoder.rate());

    let start = Instant::now();

    // Start the capture session
    output_ctx.write_header()?;
    session.StartCapture()?;
    //unsafe { audio_client.Start() }?;
    println!("Capture session started");
    loop {
        let image_data = get_image(&d11_device, &d11_context, &pool);
        if image_data.is_ok() {
            let mut raw_frame = util::frame::Video::empty();
            unsafe {
                raw_frame.alloc(
                    format::Pixel::BGRA,
                    item.Size()?.Width as u32,
                    item.Size()?.Height as u32,
                )
            };
            let mut frame = util::frame::Video::empty();
            unsafe { frame.alloc(format::Pixel::YUV420P, OUT_WIDTH, OUT_HEIGHT) };
            let mut scaler = software::scaling::Context::get(
                raw_frame.format(),
                raw_frame.width(),
                raw_frame.height(),
                frame.format(),
                frame.width(),
                frame.height(),
                software::scaling::Flags::BILINEAR,
            )?;
            raw_frame.data_mut(0).copy_from_slice(&image_data?);
            scaler.run(&raw_frame, &mut frame)?;
            let pts = start.elapsed().as_micros() as i64 * 30 / 1_000_000;
            frame.set_pts(Some(pts));
            video_encoder.send_frame(&frame)?;

            //unsafe { capture_client.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }?;
            //unsafe {
            //    std::ptr::copy_nonoverlapping(&mut data, &mut raw_audio_data, frames as usize * 2)
            //};
            //unsafe { capture_client.ReleaseBuffer(frames) }?;

            //let pts = start.elapsed().as_micros() as i64 * 30 / 1_000_000;
            //raw_audio.set_pts(Some(pts));
            //audio_encoder.send_frame(&raw_audio)?;

            let mut packet = Packet::empty();
            //while video_encoder.receive_packet(&mut packet).is_ok() {}
        }
    }
    Ok(())
}
