use once_cell::sync::Lazy;
use std::sync::Mutex;

use crate::orchestrator::log;

static ACTIVE_SESSION: Lazy<Mutex<Option<platform::Session>>> = Lazy::new(|| Mutex::new(None));

pub fn begin(enabled: bool) {
    if !enabled {
        end();
        return;
    }

    let Ok(mut guard) = ACTIVE_SESSION.lock() else {
        log::info("Audio ducking unavailable: session lock poisoned");
        return;
    };

    if guard.is_some() {
        return;
    }

    match platform::Session::begin() {
        Ok(session) => {
            *guard = Some(session);
        }
        Err(e) => {
            log::info(&format!("Audio ducking unavailable: {e}"));
        }
    }
}

pub fn end() {
    let session = ACTIVE_SESSION
        .lock()
        .ok()
        .and_then(|mut guard| guard.take());
    if let Some(session) = session {
        if let Err(e) = session.end() {
            log::info(&format!("Audio ducking restore failed: {e}"));
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::log;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr;

    type AudioObjectID = u32;
    type AudioObjectPropertySelector = u32;
    type AudioObjectPropertyScope = u32;
    type AudioObjectPropertyElement = u32;
    type AudioObjectPropertyElementName = AudioObjectPropertyElement;
    type OSStatus = i32;

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        m_selector: AudioObjectPropertySelector,
        m_scope: AudioObjectPropertyScope,
        m_element: AudioObjectPropertyElementName,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectHasProperty(
            inObjectID: AudioObjectID,
            inAddress: *const AudioObjectPropertyAddress,
        ) -> u8;
        fn AudioObjectGetPropertyData(
            inObjectID: AudioObjectID,
            inAddress: *const AudioObjectPropertyAddress,
            inQualifierDataSize: u32,
            inQualifierData: *const c_void,
            ioDataSize: *mut u32,
            outData: *mut c_void,
        ) -> OSStatus;
        fn AudioObjectSetPropertyData(
            inObjectID: AudioObjectID,
            inAddress: *const AudioObjectPropertyAddress,
            inQualifierDataSize: u32,
            inQualifierData: *const c_void,
            inDataSize: u32,
            inData: *const c_void,
        ) -> OSStatus;
    }

    const K_AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectID = 1;
    const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: AudioObjectPropertyElement = 0;
    const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = fourcc(*b"dOut");
    const K_AUDIO_DEVICE_PROPERTY_MUTE: u32 = fourcc(*b"mute");
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = fourcc(*b"glob");
    const K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: u32 = fourcc(*b"outp");

    pub struct Session {
        device_id: AudioObjectID,
        previous_muted: bool,
        changed: bool,
    }

    impl Session {
        pub fn begin() -> Result<Self, String> {
            let device_id = default_output_device()?;
            let previous_muted = get_device_mute(device_id)?;

            if previous_muted {
                log::info("Audio ducking: default output device is already muted");
                return Ok(Self {
                    device_id,
                    previous_muted,
                    changed: false,
                });
            }

            set_device_mute(device_id, true)?;
            log::info(&format!(
                "Audio ducking: muted default output device {device_id}"
            ));

            Ok(Self {
                device_id,
                previous_muted,
                changed: true,
            })
        }

        pub fn end(self) -> Result<(), String> {
            if !self.changed {
                return Ok(());
            }

            set_device_mute(self.device_id, self.previous_muted)?;
            log::info(&format!(
                "Audio ducking: restored output device {} mute state to {}",
                self.device_id, self.previous_muted
            ));
            Ok(())
        }
    }

    const fn fourcc(bytes: [u8; 4]) -> u32 {
        ((bytes[0] as u32) << 24)
            | ((bytes[1] as u32) << 16)
            | ((bytes[2] as u32) << 8)
            | bytes[3] as u32
    }

    fn default_output_device() -> Result<AudioObjectID, String> {
        let address = AudioObjectPropertyAddress {
            m_selector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut device_id: AudioObjectID = 0;
        let mut size = size_of::<AudioObjectID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                K_AUDIO_OBJECT_SYSTEM_OBJECT,
                &address,
                0,
                ptr::null(),
                &mut size,
                (&mut device_id as *mut AudioObjectID).cast(),
            )
        };

        if status != 0 {
            return Err(format!(
                "failed to read default output device, OSStatus {status}"
            ));
        }
        if device_id == 0 {
            return Err("default output device is unknown".to_string());
        }

        Ok(device_id)
    }

    fn mute_address() -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            m_selector: K_AUDIO_DEVICE_PROPERTY_MUTE,
            m_scope: K_AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        }
    }

    fn get_device_mute(device_id: AudioObjectID) -> Result<bool, String> {
        let address = mute_address();
        let has_property = unsafe { AudioObjectHasProperty(device_id, &address) } != 0;
        if !has_property {
            return Err(format!(
                "output device {device_id} does not expose a mute property"
            ));
        }

        let mut muted: u32 = 0;
        let mut size = size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                &address,
                0,
                ptr::null(),
                &mut size,
                (&mut muted as *mut u32).cast(),
            )
        };

        if status != 0 {
            return Err(format!(
                "failed to read output device {device_id} mute state, OSStatus {status}"
            ));
        }

        Ok(muted != 0)
    }

    fn set_device_mute(device_id: AudioObjectID, muted: bool) -> Result<(), String> {
        let address = mute_address();
        let value: u32 = u32::from(muted);
        let status = unsafe {
            AudioObjectSetPropertyData(
                device_id,
                &address,
                0,
                ptr::null(),
                size_of::<u32>() as u32,
                (&value as *const u32).cast(),
            )
        };

        if status != 0 {
            return Err(format!(
                "failed to set output device {device_id} mute state to {muted}, OSStatus {status}"
            ));
        }

        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::log;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use windows::core::Interface;
    use windows::Win32::Foundation::{BOOL, RPC_E_CHANGED_MODE, S_FALSE, S_OK};
    use windows::Win32::Media::Audio::{
        eCapture, eCommunications, eConsole, AudioCategory_Communications, AudioClientProperties,
        IAudioCaptureClient, IAudioClient, IAudioClient2, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMOPTIONS_NONE, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    pub struct Session {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Session {
        pub fn begin() -> Result<Self, String> {
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

            let thread = std::thread::Builder::new()
                .name("yap-audio-ducking".into())
                .spawn(move || {
                    let result = run_communications_capture(thread_stop, ready_tx);
                    if let Err(e) = result {
                        log::info(&format!("Audio ducking worker stopped: {e}"));
                    }
                })
                .map_err(|e| format!("failed to spawn audio ducking worker: {e}"))?;

            match ready_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Ok(())) => {
                    log::info("Audio ducking: started Windows communications session");
                    Ok(Self {
                        stop,
                        thread: Some(thread),
                    })
                }
                Ok(Err(e)) => {
                    stop.store(true, Ordering::SeqCst);
                    let _ = thread.join();
                    Err(e)
                }
                Err(e) => {
                    stop.store(true, Ordering::SeqCst);
                    let _ = thread.join();
                    Err(format!("audio ducking worker did not start: {e}"))
                }
            }
        }

        pub fn end(mut self) -> Result<(), String> {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            log::info("Audio ducking: stopped Windows communications session");
            Ok(())
        }
    }

    fn run_communications_capture(
        stop: Arc<AtomicBool>,
        ready_tx: mpsc::Sender<Result<(), String>>,
    ) -> Result<(), String> {
        let coinit = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let should_uninitialize = coinit == S_OK || coinit == S_FALSE;
        if coinit == RPC_E_CHANGED_MODE {
            let _ = ready_tx.send(Err(
                "COM is already initialized with an incompatible apartment".to_string(),
            ));
            return Err("COM apartment mismatch".to_string());
        }
        if let Err(e) = coinit.ok() {
            let _ = ready_tx.send(Err(format!("failed to initialize COM: {e}")));
            return Err(format!("failed to initialize COM: {e}"));
        }

        let startup_tx = ready_tx.clone();
        let result = unsafe { run_communications_capture_inner(stop, startup_tx) };
        if let Err(e) = &result {
            let _ = ready_tx.send(Err(e.clone()));
        }

        if should_uninitialize {
            unsafe { CoUninitialize() };
        }

        result
    }

    unsafe fn run_communications_capture_inner(
        stop: Arc<AtomicBool>,
        ready_tx: mpsc::Sender<Result<(), String>>,
    ) -> Result<(), String> {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("failed to create audio device enumerator: {e}"))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eCapture, eCommunications)
            .or_else(|_| enumerator.GetDefaultAudioEndpoint(eCapture, eConsole))
            .map_err(|e| format!("failed to get default capture device: {e}"))?;
        let state = device
            .GetState()
            .map_err(|e| format!("failed to read communications capture device state: {e}"))?;
        if state != DEVICE_STATE_ACTIVE {
            return Err("default communications capture device is not active".to_string());
        }

        let client: IAudioClient2 = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("failed to activate communications audio client: {e}"))?;
        let properties = AudioClientProperties {
            cbSize: std::mem::size_of::<AudioClientProperties>() as u32,
            bIsOffload: BOOL(0),
            eCategory: AudioCategory_Communications,
            Options: AUDCLNT_STREAMOPTIONS_NONE,
        };
        client
            .SetClientProperties(&properties)
            .map_err(|e| format!("failed to mark stream as communications audio: {e}"))?;

        let base_client: IAudioClient = client.cast().map_err(|e| {
            format!("failed to cast communications audio client to IAudioClient: {e}")
        })?;
        let mix_format = base_client
            .GetMixFormat()
            .map_err(|e| format!("failed to read communications mix format: {e}"))?;

        let init_result =
            base_client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 1_000_000, 0, mix_format, None);
        CoTaskMemFree(Some(mix_format.cast()));
        init_result.map_err(|e| format!("failed to initialize communications stream: {e}"))?;

        let capture: IAudioCaptureClient = base_client
            .GetService()
            .map_err(|e| format!("failed to get communications capture service: {e}"))?;
        base_client
            .Start()
            .map_err(|e| format!("failed to start communications stream: {e}"))?;

        let _ = ready_tx.send(Ok(()));

        while !stop.load(Ordering::SeqCst) {
            drain_capture_packets(&capture)?;
            std::thread::sleep(Duration::from_millis(10));
        }

        let _ = base_client.Stop();
        Ok(())
    }

    unsafe fn drain_capture_packets(capture: &IAudioCaptureClient) -> Result<(), String> {
        let mut next_packet = capture
            .GetNextPacketSize()
            .map_err(|e| format!("failed to read communications packet size: {e}"))?;

        while next_packet > 0 {
            let mut data = ptr::null_mut();
            let mut frames = 0;
            let mut flags = 0;
            capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .map_err(|e| format!("failed to read communications packet: {e}"))?;
            capture
                .ReleaseBuffer(frames)
                .map_err(|e| format!("failed to release communications packet: {e}"))?;
            next_packet = capture
                .GetNextPacketSize()
                .map_err(|e| format!("failed to read communications packet size: {e}"))?;
        }

        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub struct Session;

    impl Session {
        pub fn begin() -> Result<Self, String> {
            Err("audio ducking is not supported on this platform".to_string())
        }

        pub fn end(self) -> Result<(), String> {
            Ok(())
        }
    }
}
