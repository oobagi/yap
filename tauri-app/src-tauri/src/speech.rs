//! On-device speech recognition via platform-native APIs.
//!
//! macOS: SFSpeechRecognizer (Speech framework)
//! Windows: not yet implemented (returns error)
//!
//! Two entry points:
//!   - `transcribe()` — full on-device transcription (used when provider = None)
//!   - `pre_check()` — quick check if audio contains speech (saves API costs)

use std::path::Path;

// ---------------------------------------------------------------------------
// macOS implementation — SFSpeechRecognizer via objc2 FFI
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::{c_char, CStr, CString};
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    use crate::orchestrator::log;

    /// Transcribe audio using on-device SFSpeechRecognizer.
    ///
    /// `locale` should be a BCP 47 string like "en-US".
    /// Returns the transcribed text, or an error.
    pub fn transcribe(audio_path: &Path, locale: &str) -> Result<String, String> {
        let path_str = audio_path
            .to_str()
            .ok_or_else(|| "invalid audio path".to_string())?;
        let path_cstr = CString::new(path_str).map_err(|e| format!("path encoding error: {e}"))?;
        let locale_cstr =
            CString::new(locale).map_err(|e| format!("locale encoding error: {e}"))?;

        let (tx, rx) = mpsc::channel::<Result<String, String>>();

        unsafe {
            // -- Create autorelease pool --
            let pool_cls =
                AnyClass::get(c"NSAutoreleasePool").ok_or("NSAutoreleasePool not found")?;
            let pool: *mut AnyObject = msg_send![pool_cls, new];

            // -- Create NSString helpers --
            let ns_str_cls = AnyClass::get(c"NSString").ok_or("NSString not found")?;
            let path_ns: *mut AnyObject =
                msg_send![ns_str_cls, stringWithUTF8String: path_cstr.as_ptr()];
            let locale_ns: *mut AnyObject =
                msg_send![ns_str_cls, stringWithUTF8String: locale_cstr.as_ptr()];

            // -- Create NSURL from file path --
            let url_cls = AnyClass::get(c"NSURL").ok_or("NSURL not found")?;
            let url: *mut AnyObject = msg_send![url_cls, fileURLWithPath: path_ns];

            // -- Create NSLocale --
            let locale_cls = AnyClass::get(c"NSLocale").ok_or("NSLocale not found")?;
            let ns_locale: *mut AnyObject =
                msg_send![locale_cls, localeWithLocaleIdentifier: locale_ns];

            // -- Create SFSpeechRecognizer with locale --
            let recognizer_cls = AnyClass::get(c"SFSpeechRecognizer")
                .ok_or("SFSpeechRecognizer not found — is the Speech framework available?")?;
            let recognizer: *mut AnyObject = msg_send![recognizer_cls, alloc];
            let recognizer: *mut AnyObject = msg_send![recognizer, initWithLocale: ns_locale];

            if recognizer.is_null() {
                let _: () = msg_send![pool, drain];
                return Err(format!(
                    "SFSpeechRecognizer not available for locale: {locale}"
                ));
            }

            // Check availability
            let available: bool = msg_send![recognizer, isAvailable];
            if !available {
                let _: () = msg_send![pool, drain];
                return Err("SFSpeechRecognizer is not available on this device".to_string());
            }

            // -- Create SFSpeechURLRecognitionRequest --
            let request_cls = AnyClass::get(c"SFSpeechURLRecognitionRequest")
                .ok_or("SFSpeechURLRecognitionRequest not found")?;
            let request: *mut AnyObject = msg_send![request_cls, alloc];
            let request: *mut AnyObject = msg_send![request, initWithURL: url];

            // -- Build the result handler block --
            // Called by the Speech framework with partial/final results.
            // We only care about the final result (isFinal == true).
            let block = RcBlock::new(move |result: *mut AnyObject, error: *mut AnyObject| {
                // Error with no result → terminal failure
                if !error.is_null() && result.is_null() {
                    let desc: *mut AnyObject = msg_send![error, localizedDescription];
                    if !desc.is_null() {
                        let cstr: *const c_char = msg_send![desc, UTF8String];
                        if !cstr.is_null() {
                            let err_str = CStr::from_ptr(cstr).to_string_lossy().to_string();
                            let _ = tx.send(Err(err_str));
                            return;
                        }
                    }
                    let _ = tx.send(Err("Speech recognition error".to_string()));
                    return;
                }

                if !result.is_null() {
                    let is_final: bool = msg_send![result, isFinal];
                    if is_final {
                        let transcription: *mut AnyObject = msg_send![result, bestTranscription];
                        if !transcription.is_null() {
                            let text: *mut AnyObject = msg_send![transcription, formattedString];
                            if !text.is_null() {
                                let cstr: *const c_char = msg_send![text, UTF8String];
                                if !cstr.is_null() {
                                    let s = CStr::from_ptr(cstr).to_string_lossy().to_string();
                                    let _ = tx.send(Ok(s));
                                    return;
                                }
                            }
                        }
                        let _ = tx.send(Ok(String::new()));
                    }
                }
            });

            // -- Start recognition task --
            let _task: *mut AnyObject = msg_send![
                recognizer,
                recognitionTaskWithRequest: request,
                resultHandler: &*block
            ];

            // Drain the autorelease pool (objects will be retained by the task)
            let _: () = msg_send![pool, drain];
        }

        // Wait for the final result with a 10-second timeout
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err("Speech recognition timed out".to_string()),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("Speech recognition channel closed unexpectedly".to_string())
            }
        }
    }

    /// Quick pre-check: does the audio contain speech?
    ///
    /// Runs on-device recognition with a short timeout. Returns `true` if
    /// any speech was detected, `false` if silent/noise-only.
    /// Used before expensive API calls to save costs.
    pub fn pre_check(audio_path: &Path) -> bool {
        log::info("Running Apple Speech pre-check");
        match transcribe(audio_path, "en-US") {
            Ok(text) => {
                let has = !text.trim().is_empty();
                log::info(&format!(
                    "Pre-check result: {} (text: {:?})",
                    if has { "speech detected" } else { "no speech" },
                    if text.len() > 60 {
                        format!("{}...", &text[..60])
                    } else {
                        text
                    }
                ));
                has
            }
            Err(e) => {
                log::info(&format!("Pre-check failed: {e} -- treating as no speech"));
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Non-macOS fallback
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::path::Path;

    pub fn transcribe(_audio_path: &Path, _locale: &str) -> Result<String, String> {
        Err("On-device speech recognition is only available on macOS".into())
    }

    pub fn pre_check(_audio_path: &Path) -> bool {
        // Can't verify — assume speech exists so the pipeline proceeds
        true
    }
}

// ---------------------------------------------------------------------------
// Public API (delegates to platform module)
// ---------------------------------------------------------------------------

/// Transcribe audio using on-device speech recognition.
pub fn transcribe(audio_path: &Path, locale: &str) -> Result<String, String> {
    platform::transcribe(audio_path, locale)
}

/// Quick pre-check: does the audio file contain speech?
/// Returns `true` if speech detected, `false` if silence/noise.
pub fn pre_check(audio_path: &Path) -> bool {
    platform::pre_check(audio_path)
}
