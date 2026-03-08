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
    case idle, recording, handsFreeRecording, handsFreePaused, processing
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
    private var handsFreeEntryTime: Date?
    private var shortTapCleanupWork: DispatchWorkItem?
    private var chimeWorkItem: DispatchWorkItem?
    private var onboardingHoldWork: DispatchWorkItem?
    private var soundPlayers: [String: AVAudioPlayer] = [:]

    private func preloadSounds() {
        for name in ["Pop", "Blow", "Submarine"] {
            guard let url = Bundle.main.url(forResource: name, withExtension: "aiff") else { continue }
            guard let player = try? AVAudioPlayer(contentsOf: url) else { continue }
            player.prepareToPlay()
            soundPlayers[name] = player
        }
    }

    private func playSound(_ name: String) {
        guard let player = soundPlayers[name] else { return }
        player.currentTime = 0
        player.play()
    }
    private var tipDismissWork: DispatchWorkItem?
    
    // Separate transcription and formatting engines
    private var audioTranscriber: AudioTranscriber?
    private var textFormatter: TextFormatter?
    private var formattingStyle: FormattingStyle = .formatted

    func applicationDidFinishLaunching(_ notification: Notification) {
        log("App launched")
        setupStatusItem()
        requestPermissions()
        setupHotkey()
        setupEngines()
        preloadSounds()
        overlayPanel.orderFront(nil)
        startOnboardingIfNeeded()
        log("Setup complete — ready")
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
        case .recording, .handsFreeRecording, .handsFreePaused:
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
        hotkeyManager?.stop()
        setupHotkey()
        setupEngines()
        if overlayPanel.currentOnboardingStep != nil {
            let hotkeyType = UserDefaults.standard.string(forKey: SettingsKey.hotkey) ?? "fn"
            overlayPanel.setHotkeyLabel(hotkeyType == "option" ? "option" : "fn")
        }
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
        let hotkeyType = UserDefaults.standard.string(forKey: SettingsKey.hotkey) ?? "fn"
        let mask: UInt64 = hotkeyType == "option" ? CGEventFlags.maskAlternate.rawValue : 0x00800000
        
        hotkeyManager = HotkeyManager(
            modifierMask: mask,
            onKeyDown: { [weak self] in self?.startRecording() },
            onKeyUp: { [weak self] in self?.stopAndTranscribe() }
        )
        hotkeyManager.onDoubleTap = { [weak self] in self?.startHandsFreeRecording() }
        
        let started = hotkeyManager.start()
        log("Hotkey (\(hotkeyType)): \(started ? "✅" : "❌ — no Accessibility?")")
        if !started {
            showNotification(title: "Yap", body: "Accessibility permission required.")
        }
    }
    
    // MARK: - Engine Setup
    
    private func setupEngines() {
        let d = UserDefaults.standard

        // Transcription
        let txProviderName = d.string(forKey: SettingsKey.txProvider) ?? "none"
        let txKey = d.string(forKey: SettingsKey.txApiKey) ?? ""
        let txModel = d.string(forKey: SettingsKey.txModel)
        let txProvider = TranscriptionProvider.allCases.first { $0.rawValue == txProviderName } ?? .none

        if txProvider != .none && !txKey.isEmpty {
            audioTranscriber = AudioTranscriber(provider: txProvider, apiKey: txKey, model: txModel?.isEmpty == true ? nil : txModel)
            log("Transcription: \(txProvider.rawValue)")
        } else {
            audioTranscriber = nil
            log("Transcription: Apple Speech")
        }

        // Formatting
        let fmtProviderName = d.string(forKey: SettingsKey.fmtProvider) ?? "none"
        let fmtStyleName = d.string(forKey: SettingsKey.fmtStyle) ?? "formatted"
        let fmtModel = d.string(forKey: SettingsKey.fmtModel)
        let fmtProvider = FormattingProvider.allCases.first { $0.rawValue == fmtProviderName } ?? .none
        formattingStyle = FormattingStyle.allCases.first { $0.rawValue == fmtStyleName } ?? .formatted

        // Resolve formatting API key: use its own, or fall back to transcription key if same provider
        var fmtKey = d.string(forKey: SettingsKey.fmtApiKey) ?? ""
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
    
    // MARK: - Onboarding

    private func startOnboardingIfNeeded() {
        guard !UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete) else { return }
        let hotkeyType = UserDefaults.standard.string(forKey: SettingsKey.hotkey) ?? "fn"
        overlayPanel.setHotkeyLabel(hotkeyType == "option" ? "option" : "fn")
        overlayPanel.advanceOnboarding(to: .tryIt)
    }

    private func advanceOnboardingStep() {
        let step = overlayPanel.currentOnboardingStep
        log("Advancing onboarding from: \(String(describing: step))")
        switch step {
        case .success:
            overlayPanel.advanceOnboarding(to: .apiTip)
        case .apiTip:
            overlayPanel.advanceOnboarding(to: .formattingTip)
        case .formattingTip:
            overlayPanel.advanceOnboarding(to: .welcome)
        case .welcome:
            finalizeOnboarding()
        default:
            finalizeOnboarding()
        }
    }

    private func finalizeOnboarding() {
        UserDefaults.standard.set(true, forKey: SettingsKey.onboardingComplete)
        overlayPanel.completeOnboarding()
        log("Onboarding finalized")
    }

    // MARK: - Recording Flow
    
    private func startRecording() {
        log("Key down — starting recording")

        // Hold-to-confirm only for actual onboarding confirmation screens
        if let step = overlayPanel.currentOnboardingStep {
            switch step {
            case .success, .apiTip, .formattingTip, .welcome:
                log("Hold-to-confirm for: \(step)")
                overlayPanel.pressDown()
                let workItem = DispatchWorkItem { [weak self] in
                    guard let self else { return }
                    self.onboardingHoldWork = nil
                    self.overlayPanel.pressRelease()
                    self.playSound("Pop")
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { [weak self] in
                        self?.advanceOnboardingStep()
                    }
                }
                onboardingHoldWork = workItem
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.6, execute: workItem)
                return
            default:
                break // .tryIt, .speakTip, .holdTip — fall through to start recording
            }
        }

        // Cancel any pending tip/error dismiss timers
        tipDismissWork?.cancel()
        tipDismissWork = nil

        // Clear transient tip steps so the UI resets cleanly
        if let step = overlayPanel.currentOnboardingStep {
            switch step {
            case .speakTip, .holdTip:
                if UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete) {
                    overlayPanel.completeOnboarding()
                } else {
                    overlayPanel.advanceOnboarding(to: .tryIt)
                }
            default:
                break
            }
        }

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
                self?.playSound("Blow")
            }
            chimeWorkItem = workItem
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1, execute: workItem)
        } catch {
            log("Recording failed: \(error)")
            state = .idle
            updateIcon(.idle)
            overlayPanel.dismiss()
            restoreOnboardingIfNeeded()
        }
    }
    
    private func stopAndTranscribe() {
        log("Key up — stopping recording")

        // Handle onboarding hold-to-confirm (released too early)
        if let work = onboardingHoldWork {
            work.cancel()
            onboardingHoldWork = nil
            overlayPanel.pressRelease()
            overlayPanel.shake()
            return
        }

        // In hands-free mode, fn release stops recording (ignore the entry release)
        if state == .handsFreeRecording || state == .handsFreePaused {
            if let entry = handsFreeEntryTime, Date().timeIntervalSince(entry) < 0.5 {
                handsFreeEntryTime = nil
                return
            }
            stopHandsFreeRecording()
            return
        }

        guard state == .recording else { return }

        let duration = Date().timeIntervalSince(recordingStart ?? Date())
        log("Duration: \(String(format: "%.1f", duration))s, peak: \(peakAudioLevel)")

        // Too short = accidental tap (but if speech was detected, let it through)
        guard duration >= 0.5 || peakAudioLevel >= 0.15 else {
            // Show the hold tip quickly after a short tap
            let remaining = max(0.15, 0.4 - duration)
            let work = DispatchWorkItem { [weak self] in
                guard let self, self.state == .recording else { return }
                self.shortTapCleanupWork = nil
                self.audioRecorder.cancel()
                self.chimeWorkItem = nil
                self.playSound("Pop")
                self.showTip(.holdTip)
            }
            shortTapCleanupWork = work
            DispatchQueue.main.asyncAfter(deadline: .now() + remaining, execute: work)
            return
        }

        chimeWorkItem = nil

        state = .processing
        updateIcon(.processing)
        overlayPanel.showProcessing()

        guard let audioURL = audioRecorder.stop() else {
            playSound("Pop")
            finishProcessing()
            return
        }
        playSound("Pop")

        processRecordedAudio(audioURL: audioURL)
    }

    /// Shared audio processing pipeline used by both hold-to-record and hands-free modes
    private func processRecordedAudio(audioURL: URL) {
        // Silence check — levels are RMS * 5, so 0.15 ≈ actual quiet speech threshold
        if peakAudioLevel < 0.15 {
            log("Silence detected (peak \(peakAudioLevel)) — skipping")
            showTip(.speakTip)
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
                        self?.showTip(.speakTip)
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
                        self?.showTip(.speakTip)
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
                        self?.pasteText(formatted)
                    case .failure(let error):
                        log("Format failed, using raw: \(error)")
                        self?.pasteText(text)
                    }
                    self?.finishProcessing()
                }
            }
        } else {
            pasteText(text)
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
                pasteText(trimmed)
            }
            finishProcessing()
        case .failure(let error):
            log("❌ Failed: \(error)")
            showError(error)
        }
    }
    
    private func showTip(_ step: OnboardingStep) {
        state = .idle
        updateIcon(.idle)
        overlayPanel.showNoSpeech()
        overlayPanel.advanceOnboarding(to: step)
        tipDismissWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard self?.overlayPanel.currentOnboardingStep == step else { return }
            self?.overlayPanel.dismiss()
            self?.restoreOnboardingIfNeeded()
            if UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete) {
                self?.overlayPanel.completeOnboarding()
            }
        }
        tipDismissWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5, execute: work)
    }

    private func pasteText(_ text: String) {
        pasteManager.paste(text)
        if overlayPanel.currentOnboardingStep == .tryIt {
            overlayPanel.advanceOnboarding(to: .success(text))
            playSound("Submarine")
        }
    }

    private func finishProcessing() {
        state = .idle
        updateIcon(.idle)
        overlayPanel.dismiss()
        restoreOnboardingIfNeeded()
    }

    // MARK: - Hands-Free Recording

    private func startHandsFreeRecording() {
        log("Double-tap — entering hands-free mode")

        // Cancel pending short-tap cleanup from the first tap
        shortTapCleanupWork?.cancel()
        shortTapCleanupWork = nil

        // Must already be recording (from the first tap's onKeyDown)
        guard state == .recording else { return }

        // Don't allow hands-free during onboarding
        guard UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete) else { return }

        state = .handsFreeRecording
        handsFreeEntryTime = Date()

        overlayPanel.showHandsFreeRecording(
            onPauseResume: { [weak self] in self?.toggleHandsFreePause() },
            onStop: { [weak self] in self?.stopHandsFreeRecording() }
        )
    }

    private func toggleHandsFreePause() {
        if state == .handsFreeRecording {
            audioRecorder.pause()
            state = .handsFreePaused
            overlayPanel.setHandsFreePaused(true)
            log("Hands-free: paused")
        } else if state == .handsFreePaused {
            audioRecorder.resume()
            state = .handsFreeRecording
            overlayPanel.setHandsFreePaused(false)
            log("Hands-free: resumed")
        }
    }

    private func stopHandsFreeRecording() {
        guard state == .handsFreeRecording || state == .handsFreePaused else { return }
        log("Hands-free: stopping")

        chimeWorkItem = nil

        guard let audioURL = audioRecorder.stop() else {
            playSound("Pop")
            state = .idle
            updateIcon(.idle)
            overlayPanel.contractHandsFree()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
                self?.overlayPanel.dismiss()
            }
            return
        }
        playSound("Pop")

        // Check silence before committing to processing UI
        if peakAudioLevel < 0.15 {
            log("Silence detected (peak \(peakAudioLevel)) — skipping")
            overlayPanel.contractHandsFree()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
                self?.showTip(.speakTip)
            }
            return
        }

        state = .processing
        updateIcon(.processing)
        overlayPanel.showProcessing()
        processRecordedAudio(audioURL: audioURL)
    }

    private func restoreOnboardingIfNeeded() {
        guard !UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete) else { return }
        let step = overlayPanel.currentOnboardingStep
        // Only restore if we're currently in an onboarding step that should bounce back to tryIt
        // If step is nil, onboarding hasn't started or was completed — don't restart it
        if step == .tryIt || step == .speakTip || step == .holdTip {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.overlayPanel.advanceOnboarding(to: .tryIt)
            }
        }
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
        // After error auto-dismisses (2s), restore onboarding
        tipDismissWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            self?.restoreOnboardingIfNeeded()
        }
        tipDismissWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5, execute: work)
    }
}
