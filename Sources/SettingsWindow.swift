import Cocoa
import SwiftUI

protocol SettingsDelegate: AnyObject {
    func settingsDidChange()
}

// MARK: - SwiftUI Settings View

struct SettingsView: View {
    @State private var hotkey: String
    @State private var style: String
    @State private var provider: String
    @State private var apiKey: String
    @State private var model: String
    
    var onSave: ((String, String, String, String, String) -> Void)?
    var onCancel: (() -> Void)?
    
    init(config: [String: Any]) {
        let formatting = config["formatting"] as? [String: Any] ?? [:]
        _hotkey = State(initialValue: config["hotkey"] as? String ?? "fn")
        _style = State(initialValue: formatting["style"] as? String ?? "verbatim")
        _provider = State(initialValue: formatting["provider"] as? String ?? "gemini")
        _apiKey = State(initialValue: formatting["api_key"] as? String ?? "")
        _model = State(initialValue: formatting["model"] as? String ?? "")
    }
    
    private var selectedStyle: FormattingStyle {
        FormattingStyle.allCases.first { $0.rawValue == style } ?? .verbatim
    }
    
    private var selectedProvider: APIProvider {
        APIProvider.allCases.first { $0.rawValue == provider } ?? .gemini
    }
    
    private var hasAPIKey: Bool {
        !apiKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
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
                
                // Formatting
                Section {
                    Picker("Mode", selection: $style) {
                        ForEach(FormattingStyle.allCases, id: \.rawValue) { mode in
                            Text(mode.label).tag(mode.rawValue)
                        }
                    }
                    .pickerStyle(.menu)
                    
                    // Example preview right under the picker
                    VStack(alignment: .leading, spacing: 8) {
                        Text("**\(selectedStyle.label)** — \(selectedStyle.description)")
                            .font(.caption)
                            .foregroundColor(.primary)
                        
                        Divider()
                        
                        Text("Input:")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                        Text("\"\(FormattingStyle.exampleInput)\"")
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .italic()
                        
                        Text("Output:")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                            .padding(.top, 2)
                        Text("\"\(selectedStyle.exampleOutput)\"")
                            .font(.caption)
                            .foregroundColor(.primary)
                    }
                    .padding(.vertical, 4)
                    .animation(.easeInOut(duration: 0.15), value: style)
                } header: {
                    Text("Formatting")
                }
                
                // AI Provider
                Section {
                    Picker("Provider", selection: $provider) {
                        ForEach(APIProvider.allCases, id: \.rawValue) { p in
                            Text(p.label).tag(p.rawValue)
                        }
                    }
                    .pickerStyle(.menu)
                    
                    TextField("API Key", text: $apiKey)
                        .textFieldStyle(.roundedBorder)
                    
                    TextField("Model (blank = \(selectedProvider.defaultModel))", text: $model)
                        .textFieldStyle(.roundedBorder)
                        .font(.body)
                } header: {
                    Text("AI Provider")
                } footer: {
                    if hasAPIKey {
                        if selectedProvider.handlesTranscription {
                            Text("✅ \(selectedProvider.label) will handle transcription and formatting.")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        } else {
                            Text("✅ Apple Speech transcribes → \(selectedProvider.label) formats the text.")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    } else {
                        Text("No API key — using Apple's built-in dictation (on-device, no formatting).")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
            
            // Buttons
            HStack {
                Spacer()
                Button("Cancel") {
                    onCancel?()
                }
                .keyboardShortcut(.cancelAction)
                
                Button("Save") {
                    onSave?(hotkey, style, provider, apiKey, model)
                }
                .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 16)
            .padding(.top, 4)
        }
        .frame(width: 480, height: 520)
    }
}

// MARK: - NSWindow wrapper

class SettingsWindow: NSWindow {
    weak var settingsDelegate: SettingsDelegate?
    
    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: 520),
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
        
        settingsView.onSave = { [weak self] hotkey, style, provider, apiKey, model in
            let config: [String: Any] = [
                "hotkey": hotkey,
                "formatting": [
                    "style": style,
                    "provider": provider,
                    "api_key": apiKey,
                    "model": model
                ] as [String: Any]
            ]
            Self.saveConfig(config)
            self?.settingsDelegate?.settingsDidChange()
            self?.close()
        }
        
        settingsView.onCancel = { [weak self] in
            self?.close()
        }
        
        contentView = NSHostingView(rootView: settingsView)
    }
    
    // Reload UI when window is shown again (picks up external config changes)
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
