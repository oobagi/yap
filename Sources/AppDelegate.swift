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

class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate, SettingsDelegate {
    private var statusItem: NSStatusItem!
    private var hotkeyManager: HotkeyManager!
    private var audioRecorder = AudioRecorder()
    private var transcriber = Transcriber()
    private var pasteManager = PasteManager()
    private lazy var overlayPanel = OverlayPanel()
    private var settingsWindow: SettingsWindow?
    private var historyWindow: HistoryWindow?
    private var historyMenuItem: NSMenuItem!
    private var state: AppState = .idle
    private var recordingStart: Date?
    private var isEnabled = true
    private var enableMenuItem: NSMenuItem!
    private var peakAudioLevel: Float = 0
    private var ignorePendingKeyUp = false

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
        guard UserDefaults.standard.object(forKey: SettingsKey.soundsEnabled) as? Bool ?? true else { return }
        guard let player = soundPlayers[name] else { return }
        player.currentTime = 0
        player.play()
    }
    private var tipDismissWork: DispatchWorkItem?
    /// The onboarding step that was active before a transient tip (.speakTip/.holdTip) appeared.
    /// Used to enforce the same input restrictions during the tip as before it.
    private var preTipOnboardingStep: OnboardingStep? = nil
    
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
        overlayPanel.setOnClickToRecord { [weak self] in self?.startClickRecording() }
        let alwaysVisible = UserDefaults.standard.object(forKey: SettingsKey.alwaysVisiblePill) as? Bool ?? true
        overlayPanel.setGradientEnabled(UserDefaults.standard.object(forKey: SettingsKey.gradientEnabled) as? Bool ?? true)
        overlayPanel.orderFront(nil)
        overlayPanel.setAlwaysVisible(alwaysVisible, animated: false)
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

        historyMenuItem = NSMenuItem(title: "History", action: nil, keyEquivalent: "")
        let historySubmenu = NSMenu()
        historyMenuItem.submenu = historySubmenu
        menu.addItem(historyMenuItem)

        let settingsItem = NSMenuItem(title: "Settings...", action: #selector(openSettings(_:)), keyEquivalent: ",")
        settingsItem.target = self
        menu.addItem(settingsItem)
        
        menu.addItem(NSMenuItem.separator())
        
        let quitItem = NSMenuItem(title: "Quit", action: #selector(quit(_:)), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)
        
        menu.delegate = self
        statusItem.menu = menu
    }

    private func rebuildHistoryMenu() {
        guard let submenu = historyMenuItem.submenu else { return }
        submenu.removeAllItems()

        let entries = HistoryManager.shared.entries
        if entries.isEmpty {
            let empty = NSMenuItem(title: "No History", action: nil, keyEquivalent: "")
            empty.isEnabled = false
            submenu.addItem(empty)
        } else {
            let shown = Array(entries.prefix(10))
            for entry in shown {
                let truncated: String
                if entry.text.count <= 60 {
                    truncated = entry.text
                } else {
                    truncated = String(entry.text.prefix(59)) + "\u{2026}"
                }
                let item = NSMenuItem(title: truncated, action: #selector(copyHistoryEntry(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = entry.text
                submenu.addItem(item)
            }

            submenu.addItem(NSMenuItem.separator())

            let showAll = NSMenuItem(title: "Show All...", action: #selector(openHistory(_:)), keyEquivalent: "")
            showAll.target = self
            submenu.addItem(showAll)
        }

        submenu.addItem(NSMenuItem.separator())

        let clear = NSMenuItem(title: "Clear History", action: #selector(clearHistory(_:)), keyEquivalent: "")
        clear.target = self
        clear.isEnabled = !entries.isEmpty
        submenu.addItem(clear)
    }

    @objc private func copyHistoryEntry(_ sender: NSMenuItem) {
        guard let text = sender.representedObject as? String else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    @objc private func openHistory(_ sender: Any?) {
        if historyWindow == nil {
            historyWindow = HistoryWindow()
        }
        historyWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func clearHistory(_ sender: Any?) {
        HistoryManager.shared.clear()
    }

    func menuWillOpen(_ menu: NSMenu) {
        if menu === statusItem.menu {
            rebuildHistoryMenu()
        }
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
        overlayPanel.setGradientEnabled(UserDefaults.standard.object(forKey: SettingsKey.gradientEnabled) as? Bool ?? true)
        overlayPanel.setAlwaysVisible(UserDefaults.standard.object(forKey: SettingsKey.alwaysVisiblePill) as? Bool ?? true)
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
        case .doubleTapTip:
            overlayPanel.advanceOnboarding(to: .apiTip) // fallback; normally double-tap does this
        case .clickTip:
            overlayPanel.advanceOnboarding(to: .apiTip) // fallback; normally click does this
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

    /// The logical onboarding step for input gating. When the overlay shows a transient tip
    /// (.speakTip / .holdTip), returns the step that caused the tip so that input restrictions
    /// from the parent step are enforced regardless of the current visual state.
    private var effectiveOnboardingStep: OnboardingStep? {
        preTipOnboardingStep ?? overlayPanel.currentOnboardingStep
    }

    /// Dismiss any currently-showing transient tip (.speakTip / .holdTip) and restore the
    /// overlay to the step that was active before the tip. No-op if not in a transient tip.
    private func dismissTransientTip() {
        guard let step = overlayPanel.currentOnboardingStep,
              step == .speakTip || step == .holdTip else { return }
        tipDismissWork?.cancel()
        tipDismissWork = nil
        if UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete) {
            overlayPanel.completeOnboarding()
        } else {
            overlayPanel.advanceOnboarding(to: preTipOnboardingStep ?? .tryIt)
        }
        preTipOnboardingStep = nil
    }

    private func startRecording() {
        log("Key down — starting recording")

        // Gate on the effective step — the parent step if we're inside a transient tip.
        // This enforces input restrictions in all visual states, including error/tip overlays.
        if let step = effectiveOnboardingStep {
            switch step {

            // Click-only and double-tap-only: fn key fully blocked in every visual state
            case .clickTip, .doubleTapTip:
                return

            // Confirmation steps: fn hold advances onboarding, never starts recording
            case .apiTip, .formattingTip, .welcome:
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

            // .tryIt and any other permitted step: dismiss tip overlay if showing, then record
            default:
                break
            }
        }

        // Dismiss any transient tip and restore the overlay to the correct step
        dismissTransientTip()

        // Cancel any pending tip/error dismiss timers
        tipDismissWork?.cancel()
        tipDismissWork = nil

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

        // In hands-free mode, ignore the key-up only if fn was already held when we entered
        if state == .handsFreeRecording || state == .handsFreePaused {
            if ignorePendingKeyUp {
                ignorePendingKeyUp = false
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
        // Capture which onboarding step we're on *before* the tip overwrites it.
        // Saved as an instance property so input handlers can enforce the same restrictions.
        let preTipStep = overlayPanel.currentOnboardingStep
        preTipOnboardingStep = preTipStep
        state = .idle
        updateIcon(.idle)
        overlayPanel.showNoSpeech()
        overlayPanel.advanceOnboarding(to: step)
        tipDismissWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.overlayPanel.currentOnboardingStep == step else { return }
            self.preTipOnboardingStep = nil
            self.overlayPanel.dismiss()
            if UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete) {
                self.overlayPanel.completeOnboarding()
            } else {
                let restoreTo: OnboardingStep
                switch preTipStep {
                case .clickTip:    restoreTo = .clickTip
                case .doubleTapTip: restoreTo = .doubleTapTip
                default:           restoreTo = .tryIt
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                    self.overlayPanel.advanceOnboarding(to: restoreTo)
                }
            }
        }
        tipDismissWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5, execute: work)
    }

    private func pasteText(_ text: String) {
        let txProvider = audioTranscriber?.provider.rawValue ?? "apple"
        let fmtProvider = textFormatter?.provider.rawValue
        let fmtStyle = textFormatter != nil ? formattingStyle.rawValue : nil
        HistoryManager.shared.append(text: text, txProvider: txProvider, fmtProvider: fmtProvider, fmtStyle: fmtStyle)
        pasteManager.paste(text)
        let nextStep: OnboardingStep?
        if overlayPanel.currentOnboardingStep == .tryIt {
            nextStep = .doubleTapTip
        } else if overlayPanel.currentOnboardingStep == .doubleTapTip {
            nextStep = .clickTip
        } else if overlayPanel.currentOnboardingStep == .clickTip {
            nextStep = .apiTip
        } else {
            nextStep = nil
        }
        if let next = nextStep {
            overlayPanel.advanceOnboarding(to: .nice(next: next))
            playSound("Submarine")
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { [weak self] in
                guard let self, case .nice = self.overlayPanel.currentOnboardingStep else { return }
                self.overlayPanel.advanceOnboarding(to: next)
            }
        }
    }

    private func finishProcessing() {
        state = .idle
        updateIcon(.idle)
        overlayPanel.dismiss()
        restoreOnboardingIfNeeded()
    }

    // MARK: - Click-to-Record

    private func startClickRecording() {
        guard isEnabled else { return }
        let onboardingComplete = UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete)
        // Use effective step so click is blocked on doubleTapTip even when a tip overlay is showing
        guard onboardingComplete || effectiveOnboardingStep == .clickTip else { return }

        // If already in hold-to-record, convert to hands-free so the key can be released
        if state == .recording {
            log("Pill clicked during hold-recording — converting to hands-free")
            startHandsFreeRecording()
            return
        }

        // If in hands-free recording/paused, stop the current session and fall through to restart
        if state == .handsFreeRecording || state == .handsFreePaused {
            log("Pill clicked during hands-free — stopping and restarting")
            audioRecorder.cancel()
            chimeWorkItem = nil
            overlayPanel.contractHandsFree()
            state = .idle
            updateIcon(.idle)
        }

        guard state == .idle else {
            log("Skipped pill click: state=\(state)")
            return
        }
        log("Pill clicked — starting hands-free recording")

        // Dismiss any transient tip and restore the overlay to the correct step
        dismissTransientTip()
        tipDismissWork?.cancel()
        tipDismissWork = nil

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
            playSound("Blow")

            // Immediately enter hands-free mode (fn not held — no key-up to ignore)
            state = .handsFreeRecording
            ignorePendingKeyUp = false
            overlayPanel.showHandsFreeRecording(
                onPauseResume: { [weak self] in self?.toggleHandsFreePause() },
                onStop: { [weak self] in self?.stopHandsFreeRecording() }
            )
        } catch {
            log("Recording failed: \(error)")
            state = .idle
            updateIcon(.idle)
            overlayPanel.dismiss()
        }
    }

    // MARK: - Hands-Free Recording

    private func startHandsFreeRecording() {
        log("Double-tap — entering hands-free mode")

        shortTapCleanupWork?.cancel()
        shortTapCleanupWork = nil

        let onDoubleTapTip = effectiveOnboardingStep == .doubleTapTip
        guard UserDefaults.standard.bool(forKey: SettingsKey.onboardingComplete) || onDoubleTapTip else { return }

        if state == .recording {
            // Normal path: first tap started recording, convert it to hands-free
            state = .handsFreeRecording
            ignorePendingKeyUp = hotkeyManager.isHeld
            overlayPanel.showHandsFreeRecording(
                onPauseResume: { [weak self] in self?.toggleHandsFreePause() },
                onStop: { [weak self] in self?.stopHandsFreeRecording() }
            )
        } else if state == .idle && onDoubleTapTip {
            // doubleTapTip path: fn key was blocked on first tap, so start recorder now.
            // dismissTransientTip() restores currentOnboardingStep to .doubleTapTip so that
            // any subsequent showTip call captures the correct preTipOnboardingStep.
            dismissTransientTip()
            guard isEnabled else { return }
            recordingStart = Date()
            peakAudioLevel = 0
            updateIcon(.recording)
            overlayPanel.showRecording()
            audioRecorder.onLevelUpdate = { [weak self] level in
                self?.overlayPanel.updateLevel(level)
                if level > (self?.peakAudioLevel ?? 0) { self?.peakAudioLevel = level }
            }
            audioRecorder.onBandLevels = { [weak self] bands in
                self?.overlayPanel.updateBandLevels(bands)
            }
            do {
                try audioRecorder.start()
                state = .handsFreeRecording
                ignorePendingKeyUp = hotkeyManager.isHeld
                overlayPanel.showHandsFreeRecording(
                    onPauseResume: { [weak self] in self?.toggleHandsFreePause() },
                    onStop: { [weak self] in self?.stopHandsFreeRecording() }
                )
                playSound("Blow")
            } catch {
                log("Recording failed: \(error)")
                state = .idle
                updateIcon(.idle)
                overlayPanel.dismiss()
            }
        }
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
        } else if step == .clickTip {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.overlayPanel.advanceOnboarding(to: .clickTip)
            }
        } else if step == .doubleTapTip {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.overlayPanel.advanceOnboarding(to: .doubleTapTip)
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
