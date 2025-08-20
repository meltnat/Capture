use std::fmt;

use windows::{
    core::PWSTR,
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Media::Audio::{
            eCapture, eConsole, eRender, EDataFlow, IAudioCaptureClient, IAudioClient3, IMMDevice,
            IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE,
        },
        System::Com::{CoCreateInstance, CLSCTX_ALL},
    },
};

use crate::audio::audio_format::AudioFormat;

pub(crate) const HNS_BUFFER_DURATION: i64 = 10_000_000;

pub struct AudioDevice {
    audio_client: IAudioClient3,
    capture_client: IAudioCaptureClient,
    imm_device_enumerator: Option<IMMDeviceEnumerator>,
    imm_device: IMMDevice,
    format: AudioFormat,
}

impl AudioDevice {
    pub fn get_devices() -> Result<IMMDeviceEnumerator, windows::core::Error> {
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
    }

    pub fn with_index(
        index: usize,
        e_data_flow: EDataFlow,
        imm_device_enumerator: Option<IMMDeviceEnumerator>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let devices = imm_device_enumerator.unwrap_or(Self::get_devices()?);
        let device = unsafe { devices.EnumAudioEndpoints(e_data_flow, DEVICE_STATE_ACTIVE)? };
        Self::new(unsafe { device.Item(index as u32) }?, e_data_flow)
    }

    pub fn default(
        e_data_flow: EDataFlow,
        imm_device_enumerator: Option<IMMDeviceEnumerator>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let devices = imm_device_enumerator.unwrap_or(Self::get_devices()?);
        let device = unsafe { devices.GetDefaultAudioEndpoint(e_data_flow, eConsole) }?;
        let mut audio = Self::new(device, e_data_flow)?;
        audio.imm_device_enumerator = Some(devices);
        Ok(audio)
    }

    #[allow(non_upper_case_globals)]
    pub fn new(
        imm_device: IMMDevice,
        e_data_flow: EDataFlow,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let audio_client: IAudioClient3 = unsafe { imm_device.Activate(CLSCTX_ALL, None) }?;
        let mut format = unsafe { *audio_client.GetMixFormat()? };
        format.wFormatTag = 3;
        format.cbSize = 0;

        unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                match e_data_flow {
                    eCapture => 0,
                    eRender => AUDCLNT_STREAMFLAGS_LOOPBACK,
                    _ => return Err("Unsupported data flow".into()),
                },
                HNS_BUFFER_DURATION,
                0,
                &format,
                None,
            )
        }?;
        let capture_client: IAudioCaptureClient = unsafe { audio_client.GetService() }?;
        Ok(Self {
            audio_client,
            capture_client,
            imm_device_enumerator: None,
            imm_device,
            format: AudioFormat::from(format),
        })
    }
    pub fn start(&self) -> Result<(), windows::core::Error> {
        unsafe { self.audio_client.Start() }
    }
    pub fn stop(&self) -> Result<(), windows::core::Error> {
        unsafe { self.audio_client.Stop() }
    }
    pub fn get_wave(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut buffer = Vec::new();
        let mut data_ptr = std::ptr::null_mut();
        let mut frames = 0;
        let mut flags = 0;
        while unsafe { self.capture_client.GetNextPacketSize() }? > 0 {
            unsafe {
                self.capture_client
                    .GetBuffer(&mut data_ptr, &mut frames, &mut flags, None, None)
            }?;
            let samples = unsafe {
                std::slice::from_raw_parts(
                    data_ptr,
                    frames as usize
                        * self.format().channels as usize
                        * self.format().bytes_per_sample as usize,
                )
            };
            buffer.extend_from_slice(samples);
            unsafe { self.capture_client.ReleaseBuffer(frames) }?;
        }
        Ok(buffer)
    }
    pub fn get_name(&self) -> Result<String, Box<dyn std::error::Error>> {
        unsafe {
            PWSTR(
                self.imm_device
                    .OpenPropertyStore(windows::Win32::System::Com::STGM(0))?
                    .GetValue(&PKEY_Device_FriendlyName)?
                    .Anonymous
                    .Anonymous
                    .Anonymous
                    .pwszVal
                    .0,
            )
            .to_string()
            .map_err(|_| "Failed to convert name to string".into())
        }
    }
    pub fn default_render(
        imm_device_enumerator: Option<IMMDeviceEnumerator>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::default(eRender, imm_device_enumerator)
    }
    pub fn default_capture(
        imm_device_enumerator: Option<IMMDeviceEnumerator>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::default(eCapture, imm_device_enumerator)
    }
    pub fn format(&self) -> &AudioFormat {
        &self.format
    }
}

impl fmt::Display for AudioDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<{:?}>{}, {}",
            unsafe { self.imm_device.GetId() }.unwrap(),
            self.get_name().unwrap_or("Unknown".to_string()),
            self.format(),
        )
    }
}
