import Cocoa
import AVFoundation
import os.log

private let logger = OSLog(subsystem: "com.yap.app", category: "general")

func log(_ message: String) {
    os_log("%{public}@", log: logger, type: .default, message)
    let logURL = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".config/yap/debug.log")
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
    private var settingsWindow: SettingsWindow?
    private var state: AppState = .idle
    private var recordingStart: Date?
    private var isEnabled = true
    private var enableMenuItem: NSMenuItem!
    private var peakAudioLevel: Float = 0
    private var chimeWorkItem: DispatchWorkItem?
    private var chimeSound: NSSound?
    
    // Separate transcription and formatting engines
    private var audioTranscriber: AudioTranscriber?
    private var textFormatter: TextFormatter?
    private var formattingStyle: FormattingStyle = .formatted
    
    // Config
    private var config: [String: Any] = [:]
    
    func applicationDidFinishLaunching(_ notification: Notification) {
        log("App launched")
        loadConfig()
        setupStatusItem()
        requestPermissions()
        setupHotkey()
        setupEngines()
        overlayPanel.orderFront(nil)
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
        
        let titleItem = NSMenuItem(title: "Yap", action: nil, keyEquivalent: "")
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
        switch state {
        case .idle:
            if let customIcon = loadMenuIcon() {
                button.image = customIcon
            } else {
                button.image = NSImage(systemSymbolName: "mic", accessibilityDescription: "Yap")
            }
        case .recording:
            button.image = NSImage(systemSymbolName: "mic.fill", accessibilityDescription: "Yap")
        case .processing:
            button.image = NSImage(systemSymbolName: "ellipsis.circle", accessibilityDescription: "Yap")
        }
    }
    
    private func loadMenuIcon() -> NSImage? {
        let bundle = Bundle.main
        // Try @2x first for retina
        if let url = bundle.url(forResource: "MenuIconTemplate@2x", withExtension: "png"),
           let img = NSImage(contentsOf: url) {
            img.isTemplate = true
            img.size = NSSize(width: 14, height: 14) // point size, @2x handles retina
            return img
        }
        if let url = bundle.url(forResource: "MenuIconTemplate", withExtension: "png"),
           let img = NSImage(contentsOf: url) {
            img.isTemplate = true
            img.size = NSSize(width: 14, height: 14)
            return img
        }
        return nil
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
        hotkeyManager?.stop()
        setupHotkey()
        setupEngines()
    }
    
    // MARK: - Permissions
    
    private func requestPermissions() {
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            log("Microphone permission: \(granted ? "✅" : "❌")")
            if !granted {
                DispatchQueue.main.async {
                    self.showNotification(title: "Yap", body: "Microphone access required.")
                }
            }
        }
        Transcriber.requestAuthorization { granted in
            log("Speech recognition permission: \(granted ? "✅" : "❌")")
        }
    }
    
    // MARK: - Hotkey
    
    private func setupHotkey() {
        let hotkeyType = config["hotkey"] as? String ?? "fn"
        let mask: UInt64 = hotkeyType == "option" ? CGEventFlags.maskAlternate.rawValue : 0x00800000
        
        hotkeyManager = HotkeyManager(
            modifierMask: mask,
            onKeyDown: { [weak self] in self?.startRecording() },
            onKeyUp: { [weak self] in self?.stopAndTranscribe() }
        )
        
        let started = hotkeyManager.start()
        log("Hotkey (\(hotkeyType)): \(started ? "✅" : "❌ — no Accessibility?")")
        if !started {
            showNotification(title: "Yap", body: "Accessibility permission required.")
        }
    }
    
    // MARK: - Engine Setup
    
    private func setupEngines() {
        let txConfig = config["transcription"] as? [String: Any] ?? [:]
        let fmtConfig = config["formatting"] as? [String: Any] ?? [:]
        
        // Transcription
        let txProviderName = txConfig["provider"] as? String ?? "none"
        let txKey = txConfig["api_key"] as? String ?? ""
        let txModel = txConfig["model"] as? String
        let txProvider = TranscriptionProvider.allCases.first { $0.rawValue == txProviderName } ?? .none
        
        if txProvider != .none && !txKey.isEmpty {
            audioTranscriber = AudioTranscriber(provider: txProvider, apiKey: txKey, model: txModel?.isEmpty == true ? nil : txModel)
            log("Transcription: \(txProvider.rawValue)")
        } else {
            audioTranscriber = nil
            log("Transcription: Apple Speech")
        }
        
        // Formatting
        let fmtProviderName = fmtConfig["provider"] as? String ?? "none"
        let fmtStyleName = fmtConfig["style"] as? String ?? "formatted"
        let fmtModel = fmtConfig["model"] as? String
        let fmtProvider = FormattingProvider.allCases.first { $0.rawValue == fmtProviderName } ?? .none
        formattingStyle = FormattingStyle.allCases.first { $0.rawValue == fmtStyleName } ?? .formatted
        
        // Resolve formatting API key: use its own, or fall back to transcription key if same provider
        var fmtKey = fmtConfig["api_key"] as? String ?? ""
        if fmtKey.isEmpty && fmtProviderName == txProviderName {
            fmtKey = txKey
        }
        
        if fmtProvider != .none && !fmtKey.isEmpty {
            textFormatter = TextFormatter(provider: fmtProvider, apiKey: fmtKey, model: fmtModel?.isEmpty == true ? nil : fmtModel, style: formattingStyle)
            log("Formatting: \(fmtProvider.rawValue) / \(formattingStyle.rawValue)")
        } else {
            textFormatter = nil
            log("Formatting: disabled")
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
        peakAudioLevel = 0
        updateIcon(.recording)
        overlayPanel.showRecording()

        audioRecorder.onLevelUpdate = { [weak self] level in
            self?.overlayPanel.updateLevel(level)
            if level > (self?.peakAudioLevel ?? 0) {
                self?.peakAudioLevel = level
            }
        }
        audioRecorder.onBandLevels = { [weak self] bands in
            self?.overlayPanel.updateBandLevels(bands)
        }

        do {
            try audioRecorder.start()
            // Delay chime so hardware has settled after engine.start()
            let workItem = DispatchWorkItem { [weak self] in
                let sound = NSSound(named: "Blow")
                self?.chimeSound = sound
                sound?.play()
            }
            chimeWorkItem = workItem
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1, execute: workItem)
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
        
        let duration = Date().timeIntervalSince(recordingStart ?? Date())
        log("Duration: \(String(format: "%.1f", duration))s, peak: \(peakAudioLevel)")
        
        // Too short = accidental tap
        guard duration >= 0.4 else {
            chimeWorkItem?.cancel()
            chimeWorkItem = nil
            chimeSound?.stop()
            chimeSound = nil
            audioRecorder.cancel()
            state = .idle
            updateIcon(.idle)
            overlayPanel.dismiss()
            return
        }

        chimeWorkItem = nil
        chimeSound = nil
        
        state = .processing
        updateIcon(.processing)
        overlayPanel.showProcessing()
        NSSound(named: "Pop")?.play()
        
        guard let audioURL = audioRecorder.stop() else {
            finishProcessing()
            return
        }
        
        // Silence check — levels are RMS * 5, so 0.15 ≈ actual quiet speech threshold
        if peakAudioLevel < 0.15 {
            log("Silence detected (peak \(peakAudioLevel)) — skipping")
            finishProcessing()
            return
        }
        
        // Determine flow based on configured engines
        if let apiTranscriber = audioTranscriber {
            // Quick Apple Speech pre-check: if no words detected, skip API call
            log("Pre-check: running Apple Speech to detect speech...")
            transcriber.transcribe(audioURL: audioURL) { [weak self] preResult in
                DispatchQueue.main.async {
                    let hasSpeech: Bool
                    if case .success(let preText) = preResult {
                        hasSpeech = !preText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    } else {
                        hasSpeech = false
                    }
                    
                    guard hasSpeech else {
                        log("Pre-check: no speech detected — skipping API call")
                        self?.finishProcessing()
                        return
                    }
                    
                    log("Pre-check: speech detected, proceeding with API")
                    self?.sendToAPI(apiTranscriber: apiTranscriber, audioURL: audioURL)
                }
            }
        } else {
            // Apple Speech → optional format
            log("Apple Speech transcription")
            transcriber.transcribe(audioURL: audioURL) { [weak self] result in
                DispatchQueue.main.async {
                    switch result {
                    case .success(let text):
                        self?.maybeFormat(text)
                    case .failure(let error):
                        log("❌ Apple Speech failed: \(error)")
                        self?.showError(error)
                    }
                }
            }
        }
    }
    
    /// Send audio to the configured API transcription provider
    private func sendToAPI(apiTranscriber: AudioTranscriber, audioURL: URL) {
        let canOneShot = apiTranscriber.provider.canAlsoFormat
            && textFormatter != nil
            && apiTranscriber.provider.rawValue == textFormatter?.provider.rawValue
        
        if canOneShot {
            log("One-shot: \(apiTranscriber.provider.rawValue) transcribe+format")
            apiTranscriber.transcribe(audioURL: audioURL, style: formattingStyle) { [weak self] result in
                DispatchQueue.main.async {
                    self?.handleResult(result)
                }
            }
        } else {
            log("Two-step: \(apiTranscriber.provider.rawValue) transcribe → \(textFormatter?.provider.rawValue ?? "none") format")
            apiTranscriber.transcribe(audioURL: audioURL) { [weak self] result in
                DispatchQueue.main.async {
                    switch result {
                    case .success(let text):
                        self?.maybeFormat(text)
                    case .failure(let error):
                        log("❌ Transcription failed: \(error)")
                        self?.showError(error)
                    }
                }
            }
        }
    }
    
    /// Format text if a formatter is configured, otherwise paste raw
    private func maybeFormat(_ text: String) {
        log("Transcription: \"\(text)\"")
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            finishProcessing()
            return
        }
        
        // Discard prompt regurgitation
        let lower = trimmed.lowercased()
        if lower.contains("transcribe this audio") || lower.contains("respond with only a json") || lower.contains("dictation commands") {
            log("⚠️ Discarded — model regurgitated prompt")
            finishProcessing()
            return
        }
        
        if let formatter = textFormatter {
            formatter.format(text) { [weak self] result in
                DispatchQueue.main.async {
                    switch result {
                    case .success(let formatted):
                        log("Formatted: \"\(formatted)\"")
                        self?.pasteManager.paste(formatted)
                    case .failure(let error):
                        log("Format failed, using raw: \(error)")
                        self?.pasteManager.paste(text)
                    }
                    self?.finishProcessing()
                }
            }
        } else {
            pasteManager.paste(text)
            finishProcessing()
        }
    }
    
    /// Handle a final result (from one-shot transcription+formatting)
    private func handleResult(_ result: Result<String, Error>) {
        switch result {
        case .success(let text):
            log("Result: \"\(text)\"")
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            // Sanity check: if result contains prompt fragments, discard it
            let lower = trimmed.lowercased()
            if lower.contains("transcribe this audio") || lower.contains("respond with only a json") || lower.contains("dictation commands") {
                log("⚠️ Discarded — model regurgitated prompt")
                finishProcessing()
                return
            }
            if !trimmed.isEmpty {
                pasteManager.paste(trimmed)
            }
            finishProcessing()
        case .failure(let error):
            log("❌ Failed: \(error)")
            showError(error)
        }
    }
    
    private func finishProcessing() {
        state = .idle
        updateIcon(.idle)
        overlayPanel.dismiss()
    }
    
    private func showNotification(title: String, body: String) {
        let notification = NSUserNotification()
        notification.title = title
        notification.informativeText = body
        NSUserNotificationCenter.default.deliver(notification)
    }
    
    /// Show a brief error message in the overlay pill, then auto-dismiss
    private func showError(_ error: Error) {
        state = .idle
        updateIcon(.idle)
        
        // Build a short user-friendly message
        let message: String
        if let fmtError = error as? FormatterError {
            switch fmtError {
            case .apiError(let msg):
                if msg.contains("quota") || msg.contains("rate") || msg.contains("429") {
                    message = "Rate limited — try again"
                } else if msg.contains("auth") || msg.contains("key") || msg.contains("401") || msg.contains("403") {
                    message = "Invalid API key"
                } else {
                    message = "API error"
                }
            case .truncatedResponse:
                message = "Response cut off — try again"
            default:
                message = fmtError.localizedDescription ?? "Something went wrong"
            }
        } else {
            let desc = error.localizedDescription
            if desc.contains("timed out") || desc.contains("timeout") {
                message = "Request timed out"
            } else if desc.contains("offline") || desc.contains("network") || desc.contains("Internet") {
                message = "No internet connection"
            } else {
                message = "Something went wrong"
            }
        }
        
        overlayPanel.showError(message)
    }
}
