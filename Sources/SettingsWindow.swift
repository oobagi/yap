import Cocoa
import SwiftUI

protocol SettingsDelegate: AnyObject {
    func settingsDidChange()
}

// MARK: - SwiftUI Settings View

struct SettingsView: View {
    @State private var hotkey: String
    
    // Transcription
    @State private var txProvider: String
    @State private var txApiKey: String
    @State private var txModel: String
    
    // Formatting
    @State private var fmtProvider: String
    @State private var fmtApiKey: String
    @State private var fmtModel: String
    @State private var fmtStyle: String
    
    var onSave: (([String: Any]) -> Void)?
    var onCancel: (() -> Void)?
    
    init(config: [String: Any]) {
        let tx = config["transcription"] as? [String: Any] ?? [:]
        let fmt = config["formatting"] as? [String: Any] ?? [:]
        
        _hotkey = State(initialValue: config["hotkey"] as? String ?? "fn")
        _txProvider = State(initialValue: tx["provider"] as? String ?? "none")
        _txApiKey = State(initialValue: tx["api_key"] as? String ?? "")
        _txModel = State(initialValue: tx["model"] as? String ?? "")
        _fmtProvider = State(initialValue: fmt["provider"] as? String ?? "none")
        _fmtApiKey = State(initialValue: fmt["api_key"] as? String ?? "")
        _fmtModel = State(initialValue: fmt["model"] as? String ?? "")
        _fmtStyle = State(initialValue: fmt["style"] as? String ?? "formatted")
    }
    
    private var selectedTxProvider: TranscriptionProvider {
        TranscriptionProvider.allCases.first { $0.rawValue == txProvider } ?? .none
    }
    
    private var selectedFmtProvider: FormattingProvider {
        FormattingProvider.allCases.first { $0.rawValue == fmtProvider } ?? .none
    }
    
    private var selectedStyle: FormattingStyle {
        FormattingStyle.allCases.first { $0.rawValue == fmtStyle } ?? .formatted
    }
    
    private var hasTxProvider: Bool { selectedTxProvider != .none }
    private var hasFmtProvider: Bool { selectedFmtProvider != .none }
    
    /// Whether formatting provider uses the same key as transcription (same provider name)
    private var fmtSharesKey: Bool {
        hasFmtProvider && fmtProvider == txProvider && !txApiKey.isEmpty
    }
    
    var body: some View {
        VStack(spacing: 0) {
            Form {
                // General
                Section {
                    Picker("Hotkey", selection: $hotkey) {
                        Text("fn / Globe 🌐").tag("fn")
                        Text("Option ⌥").tag("option")
                    }
                    .pickerStyle(.menu)
                }
                
                // Transcription
                Section {
                    Picker("Provider", selection: $txProvider) {
                        ForEach(TranscriptionProvider.allCases, id: \.rawValue) { p in
                            Text(p.label).tag(p.rawValue)
                        }
                    }
                    .pickerStyle(.menu)
                    
                    if hasTxProvider {
                        TextField("API Key", text: $txApiKey)
                            .textFieldStyle(.roundedBorder)
                        
                        TextField(selectedTxProvider.defaultModel, text: $txModel)
                            .textFieldStyle(.roundedBorder)
                    }
                } header: {
                    Text("Transcription")
                } footer: {
                    if !hasTxProvider {
                        Text("Using Apple's built-in dictation — free, on-device.")
                            .font(.caption).foregroundColor(.secondary)
                    }
                }
                
                // Formatting
                Section {
                    Picker("Provider", selection: $fmtProvider) {
                        ForEach(FormattingProvider.allCases, id: \.rawValue) { p in
                            Text(p.label).tag(p.rawValue)
                        }
                    }
                    .pickerStyle(.menu)
                    
                    if hasFmtProvider {
                        if fmtSharesKey {
                            Text("Using API key from transcription.")
                                .font(.caption).foregroundColor(.secondary)
                        } else {
                            TextField("API Key", text: $fmtApiKey)
                                .textFieldStyle(.roundedBorder)
                        }
                        
                        TextField(selectedFmtProvider.defaultModel, text: $fmtModel)
                            .textFieldStyle(.roundedBorder)
                        
                        Picker("Style", selection: $fmtStyle) {
                            ForEach(FormattingStyle.allCases, id: \.rawValue) { s in
                                Text(s.label).tag(s.rawValue)
                            }
                        }
                        .pickerStyle(.menu)
                        
                        // Example preview
                        VStack(alignment: .leading, spacing: 6) {
                            Text("**\(selectedStyle.label)** — \(selectedStyle.description)")
                                .font(.caption).foregroundColor(.primary)
                            Divider()
                            Text("Input:").font(.caption2).foregroundColor(.secondary)
                            Text("\"\(FormattingStyle.exampleInput)\"")
                                .font(.caption).foregroundColor(.secondary).italic()
                            Text("Output:").font(.caption2).foregroundColor(.secondary).padding(.top, 2)
                            Text("\"\(selectedStyle.exampleOutput)\"")
                                .font(.caption).foregroundColor(.primary)
                        }
                        .padding(.vertical, 4)
                        .animation(.easeInOut(duration: 0.15), value: fmtStyle)
                    }
                } header: {
                    Text("Formatting")
                } footer: {
                    if !hasFmtProvider {
                        Text("No formatting — raw transcription will be pasted as-is.")
                            .font(.caption).foregroundColor(.secondary)
                    }
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
            .animation(.easeInOut(duration: 0.2), value: txProvider)
            .animation(.easeInOut(duration: 0.2), value: fmtProvider)
            
            // Buttons
            HStack {
                Spacer()
                Button("Cancel") { onCancel?() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    let config: [String: Any] = [
                        "hotkey": hotkey,
                        "transcription": [
                            "provider": txProvider,
                            "api_key": txApiKey,
                            "model": txModel
                        ] as [String: Any],
                        "formatting": [
                            "provider": fmtProvider,
                            "api_key": fmtApiKey,
                            "model": fmtModel,
                            "style": fmtStyle
                        ] as [String: Any]
                    ]
                    onSave?(config)
                }
                .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 16)
            .padding(.top, 4)
        }
        .frame(width: 480, height: 600)
    }
}

// MARK: - NSWindow wrapper

class SettingsWindow: NSWindow {
    weak var settingsDelegate: SettingsDelegate?
    
    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: 600),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        
        title = "VoiceType Settings"
        isReleasedWhenClosed = false
        center()
        loadUI()
    }
    
    private func loadUI() {
        let config = Self.loadConfig()
        
        var settingsView = SettingsView(config: config)
        
        settingsView.onSave = { [weak self] config in
            Self.saveConfig(config)
            self?.settingsDelegate?.settingsDidChange()
            self?.close()
        }
        
        settingsView.onCancel = { [weak self] in
            self?.close()
        }
        
        contentView = NSHostingView(rootView: settingsView)
    }
    
    override func makeKeyAndOrderFront(_ sender: Any?) {
        loadUI()
        super.makeKeyAndOrderFront(sender)
    }
    
    // MARK: - Config File
    
    static func configURL() -> URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".config/voicetype/config.json")
    }
    
    static func loadConfig() -> [String: Any] {
        guard let data = try? Data(contentsOf: configURL()),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return [:]
        }
        return json
    }
    
    static func saveConfig(_ config: [String: Any]) {
        let url = configURL()
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        if let data = try? JSONSerialization.data(withJSONObject: config, options: .prettyPrinted) {
            try? data.write(to: url)
        }
    }
}
