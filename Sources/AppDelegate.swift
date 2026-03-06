import Cocoa
import AVFoundation
import os.log

private let logger = OSLog(subsystem: "com.voicetype.app", category: "general")

func log(_ message: String) {
    os_log("%{public}@", log: logger, type: .default, message)
    // Also write to log file
    let logURL = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".config/voicetype/debug.log")
    let timestamp = ISO8601DateFormatter().string(from: Date())
    let line = "[\(timestamp)] \(message)\n"
    if let handle = try? FileHandle(forWritingTo: logURL) {
        handle.seekToEndOfFile()
        handle.write(line.data(using: .utf8)!)
        handle.closeFile()
    } else {
        try? line.data(using: .utf8)?.write(to: logURL)
    }
}

enum AppState {
    case idle, recording, processing
}

class AppDelegate: NSObject, NSApplicationDelegate, SettingsDelegate {
    private var statusItem: NSStatusItem!
    private var hotkeyManager: HotkeyManager!
    private var audioRecorder = AudioRecorder()
    private var transcriber = Transcriber()
    private var pasteManager = PasteManager()
    private lazy var overlayPanel = OverlayPanel()
    private var textFormatter: TextFormatter?
    private var settingsWindow: SettingsWindow?
    private var state: AppState = .idle
    private var recordingStart: Date?
    private var isEnabled = true
    private var enableMenuItem: NSMenuItem!
    private var peakAudioLevel: Float = 0
    
    // Config
    private var config: [String: Any] = [:]
    
    func applicationDidFinishLaunching(_ notification: Notification) {
        log("App launched")
        loadConfig()
        setupStatusItem()
        requestPermissions()
        setupHotkey()
        setupFormatter()
        log("Setup complete — ready")
    }
    
    // MARK: - Config
    
    private func loadConfig() {
        config = SettingsWindow.loadConfig()
    }
    
    // MARK: - Menu Bar
    
    private func setupStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        updateIcon(.idle)
        
        let menu = NSMenu()
        
        let titleItem = NSMenuItem(title: "VoiceType", action: nil, keyEquivalent: "")
        titleItem.isEnabled = false
        menu.addItem(titleItem)
        menu.addItem(NSMenuItem.separator())
        
        enableMenuItem = NSMenuItem(title: "Enabled", action: #selector(toggleEnabled(_:)), keyEquivalent: "e")
        enableMenuItem.target = self
        enableMenuItem.state = .on
        menu.addItem(enableMenuItem)
        
        let settingsItem = NSMenuItem(title: "Settings...", action: #selector(openSettings(_:)), keyEquivalent: ",")
        settingsItem.target = self
        menu.addItem(settingsItem)
        
        menu.addItem(NSMenuItem.separator())
        
        let quitItem = NSMenuItem(title: "Quit", action: #selector(quit(_:)), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)
        
        statusItem.menu = menu
    }
    
    private func updateIcon(_ state: AppState) {
        guard let button = statusItem.button else { return }
        let symbolName: String
        switch state {
        case .idle:
            symbolName = "mic"
        case .recording:
            symbolName = "mic.fill"
        case .processing:
            symbolName = "ellipsis.circle"
        }
        button.image = NSImage(systemSymbolName: symbolName, accessibilityDescription: "VoiceType")
    }
    
    @objc private func toggleEnabled(_ sender: NSMenuItem) {
        isEnabled.toggle()
        sender.state = isEnabled ? .on : .off
        updateIcon(.idle)
    }
    
    @objc private func openSettings(_ sender: Any) {
        if settingsWindow == nil {
            settingsWindow = SettingsWindow()
            settingsWindow?.settingsDelegate = self
        }
        settingsWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
    
    @objc private func quit(_ sender: Any) {
        NSApp.terminate(nil)
    }
    
    // MARK: - Settings Delegate
    
    func settingsDidChange() {
        log("Settings changed, reloading...")
        loadConfig()
        
        // Restart hotkey manager with new config
        hotkeyManager?.stop()
        setupHotkey()
        setupFormatter()
    }
    
    // MARK: - Permissions
    
    private func requestPermissions() {
        // Microphone
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            log("Microphone permission: \(granted ? "✅" : "❌")")
            if !granted {
                DispatchQueue.main.async {
                    self.showNotification(
                        title: "VoiceType",
                        body: "Microphone access required. Open System Settings → Privacy & Security → Microphone."
                    )
                }
            }
        }
        
        // Speech recognition
        Transcriber.requestAuthorization { granted in
            log("Speech recognition permission: \(granted ? "✅" : "❌")")
            if !granted {
                self.showNotification(
                    title: "VoiceType",
                    body: "Speech recognition access required. Open System Settings → Privacy & Security → Speech Recognition."
                )
            }
        }
    }
    
    // MARK: - Hotkey
    
    private func setupHotkey() {
        let hotkeyType = config["hotkey"] as? String ?? "fn"
        let mask: UInt64
        switch hotkeyType {
        case "option":
            mask = CGEventFlags.maskAlternate.rawValue
        default: // "fn"
            mask = 0x00800000 // NX_SECONDARYFNMASK
        }
        
        hotkeyManager = HotkeyManager(
            modifierMask: mask,
            onKeyDown: { [weak self] in self?.startRecording() },
            onKeyUp: { [weak self] in self?.stopAndTranscribe() }
        )
        
        let hotkeyStarted = hotkeyManager.start()
        log("Hotkey (\(hotkeyType)) event tap: \(hotkeyStarted ? "✅" : "❌ FAILED — no Accessibility permission?")")
        if !hotkeyStarted {
            showNotification(
                title: "VoiceType — Accessibility Required",
                body: "Open System Settings → Privacy & Security → Accessibility → add VoiceType"
            )
        }
    }
    
    // MARK: - Formatter
    
    private func setupFormatter() {
        let formatting = config["formatting"] as? [String: Any] ?? [:]
        let providerName = formatting["provider"] as? String ?? "none"
        let styleName = formatting["style"] as? String ?? "verbatim"
        let apiKey = formatting["api_key"] as? String ?? ""
        let model = formatting["model"] as? String
        
        let provider = APIProvider.allCases.first { $0.rawValue == providerName } ?? .gemini
        let style = FormattingStyle.allCases.first { $0.rawValue == styleName } ?? .verbatim
        
        if !apiKey.isEmpty {
            // Gemini works with any style (it handles transcription)
            // OpenAI/Anthropic only make sense with non-verbatim styles
            if provider.handlesTranscription || style != .verbatim {
                textFormatter = TextFormatter(
                    provider: provider,
                    apiKey: apiKey,
                    model: model?.isEmpty == true ? nil : model,
                    style: style
                )
                log("Formatter: \(provider.rawValue) / \(style.rawValue)")
            } else {
                textFormatter = nil
                log("Formatter: disabled (verbatim + text-only provider)")
            }
        } else {
            textFormatter = nil
            log("No API key — using Apple Speech only")
        }
    }
    
    // MARK: - Recording Flow
    
    private func startRecording() {
        log("Key down — starting recording")
        guard isEnabled, state == .idle else {
            log("Skipped: enabled=\(isEnabled) state=\(state)")
            return
        }
        state = .recording
        recordingStart = Date()
        updateIcon(.recording)
        overlayPanel.showRecording()
        peakAudioLevel = 0
        NSSound(named: "Blow")?.play()
        
        // Wire up audio level updates to the overlay
        audioRecorder.onLevelUpdate = { [weak self] level in
            self?.overlayPanel.updateLevel(level)
            if level > (self?.peakAudioLevel ?? 0) {
                self?.peakAudioLevel = level
            }
        }
        
        do {
            try audioRecorder.start()
        } catch {
            log("Recording failed: \(error)")
            state = .idle
            updateIcon(.idle)
            overlayPanel.dismiss()
        }
    }
    
    private func stopAndTranscribe() {
        log("Key up — stopping recording")
        guard state == .recording else { return }
        
        // Minimum 0.4s to avoid accidental taps
        let duration = Date().timeIntervalSince(recordingStart ?? Date())
        log("Recording duration: \(String(format: "%.1f", duration))s")
        guard duration >= 0.4 else {
            audioRecorder.cancel()
            state = .idle
            updateIcon(.idle)
            overlayPanel.dismiss()
            return
        }
        
        state = .processing
        updateIcon(.processing)
        overlayPanel.showProcessing()
        NSSound(named: "Pop")?.play()
        
        guard let audioURL = audioRecorder.stop() else {
            state = .idle
            updateIcon(.idle)
            overlayPanel.dismiss()
            return
        }
        
        // Skip if no meaningful audio was detected (prevents hallucination)
        log("Peak audio level: \(peakAudioLevel)")
        if peakAudioLevel < 0.05 {
            log("Silence detected (peak \(peakAudioLevel) < 0.05) — skipping transcription")
            finishProcessing()
            return
        }
        
        // If provider handles transcription (Gemini), skip Apple Speech entirely
        if let formatter = textFormatter, formatter.handlesTranscription {
            log("Using \(formatter) for transcription + formatting")
            formatter.transcribeAndFormat(audioURL: audioURL) { [weak self] result in
                DispatchQueue.main.async {
                    switch result {
                    case .success(let text):
                        log("Transcribe+format result: \"\(text)\"")
                        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
                        if !trimmed.isEmpty {
                            self?.pasteManager.paste(trimmed)
                        }
                    case .failure(let error):
                        log("❌ Transcribe+format failed: \(error)")
                        self?.showNotification(title: "VoiceType", body: "Transcription failed: \(error.localizedDescription)")
                    }
                    self?.finishProcessing()
                }
            }
        } else {
            // Apple Speech → optional AI formatting (existing flow)
            transcriber.transcribe(audioURL: audioURL) { [weak self] result in
                DispatchQueue.main.async {
                    switch result {
                    case .success(let text):
                        log("Transcription result: \"\(text)\"")
                        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            self?.finishProcessing()
                            return
                        }
                        if let formatter = self?.textFormatter {
                            formatter.format(text) { formatResult in
                                DispatchQueue.main.async {
                                    switch formatResult {
                                    case .success(let formatted):
                                        log("Formatted: \"\(formatted)\"")
                                        self?.pasteManager.paste(formatted)
                                    case .failure(let error):
                                        log("Formatting failed, using raw: \(error)")
                                        self?.pasteManager.paste(text)
                                    }
                                    self?.finishProcessing()
                                }
                            }
                        } else {
                            self?.pasteManager.paste(text)
                            self?.finishProcessing()
                        }
                    case .failure(let error):
                        log("❌ Transcription failed: \(error)")
                        self?.showNotification(title: "VoiceType", body: "Transcription failed: \(error.localizedDescription)")
                        self?.finishProcessing()
                    }
                }
            }
        }
    }
    
    private func finishProcessing() {
        state = .idle
        updateIcon(.idle)
        overlayPanel.dismiss()
    }
    
    // MARK: - Helpers
    
    private func showNotification(title: String, body: String) {
        let notification = NSUserNotification()
        notification.title = title
        notification.informativeText = body
        NSUserNotificationCenter.default.deliver(notification)
    }
}
