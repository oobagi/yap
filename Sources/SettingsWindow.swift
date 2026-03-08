import Cocoa
import SwiftUI

protocol SettingsDelegate: AnyObject {
    func settingsDidChange()
}

// MARK: - UserDefaults Keys

enum SettingsKey {
    static let hotkey = "hotkey"
    static let txProvider = "txProvider"
    static let txApiKey = "txApiKey"
    static let txModel = "txModel"
    static let fmtProvider = "fmtProvider"
    static let fmtApiKey = "fmtApiKey"
    static let fmtModel = "fmtModel"
    static let fmtStyle = "fmtStyle"
    static let onboardingComplete = "onboardingComplete"

    // Deepgram options
    static let dgSmartFormat = "dgSmartFormat"
    static let dgKeywords = "dgKeywords"
    static let dgLanguage = "dgLanguage"

    // OpenAI transcription options
    static let oaiLanguage = "oaiLanguage"
    static let oaiPrompt = "oaiPrompt"

    // Gemini transcription options
    static let geminiTemperature = "geminiTemperature"

    // ElevenLabs options
    static let elLanguageCode = "elLanguageCode"
}

// MARK: - Reusable Components

/// Inline description shown below a setting — visible without hovering
private struct SettingDescription: View {
    let text: String
    var body: some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A toggle with an inline description underneath
private struct DescribedToggle: View {
    let title: String
    let description: String
    @Binding var isOn: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Toggle(title, isOn: $isOn)
            SettingDescription(text: description)
        }
    }
}

/// A text field with an inline description underneath
private struct DescribedTextField: View {
    let title: String
    let description: String
    let placeholder: String
    @Binding var text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            TextField(title, text: $text, prompt: Text(placeholder))
            SettingDescription(text: description)
        }
    }
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
    @State private var fmtUseSameKey: Bool

    // Deepgram options
    @State private var dgSmartFormat: Bool
    @State private var dgKeywords: String
    @State private var dgLanguage: String

    // OpenAI transcription options
    @State private var oaiLanguage: String
    @State private var oaiPrompt: String

    // Gemini options
    @State private var geminiTemperature: Double

    // ElevenLabs options
    @State private var elLanguageCode: String

    var onSave: (() -> Void)?
    var onCancel: (() -> Void)?

    init() {
        let d = UserDefaults.standard
        _hotkey = State(initialValue: d.string(forKey: SettingsKey.hotkey) ?? "fn")
        _txProvider = State(initialValue: d.string(forKey: SettingsKey.txProvider) ?? "none")
        _txApiKey = State(initialValue: d.string(forKey: SettingsKey.txApiKey) ?? "")
        _txModel = State(initialValue: d.string(forKey: SettingsKey.txModel) ?? "")
        _fmtProvider = State(initialValue: d.string(forKey: SettingsKey.fmtProvider) ?? "none")
        _fmtApiKey = State(initialValue: d.string(forKey: SettingsKey.fmtApiKey) ?? "")
        _fmtModel = State(initialValue: d.string(forKey: SettingsKey.fmtModel) ?? "")
        _fmtStyle = State(initialValue: d.string(forKey: SettingsKey.fmtStyle) ?? "formatted")

        let txKey = d.string(forKey: SettingsKey.txApiKey) ?? ""
        let fKey = d.string(forKey: SettingsKey.fmtApiKey) ?? ""
        _fmtUseSameKey = State(initialValue: fKey.isEmpty || fKey == txKey)

        _dgSmartFormat = State(initialValue: d.object(forKey: SettingsKey.dgSmartFormat) as? Bool ?? true)
        _dgKeywords = State(initialValue: d.string(forKey: SettingsKey.dgKeywords) ?? "")
        _dgLanguage = State(initialValue: d.string(forKey: SettingsKey.dgLanguage) ?? "")

        _oaiLanguage = State(initialValue: d.string(forKey: SettingsKey.oaiLanguage) ?? "")
        _oaiPrompt = State(initialValue: d.string(forKey: SettingsKey.oaiPrompt) ?? "")

        _geminiTemperature = State(initialValue: d.object(forKey: SettingsKey.geminiTemperature) as? Double ?? 0.0)

        _elLanguageCode = State(initialValue: d.string(forKey: SettingsKey.elLanguageCode) ?? "")
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

    /// Whether the transcription and formatting providers share the same API backend
    private var canShareApiKey: Bool {
        guard hasTxProvider, hasFmtProvider else { return false }
        switch (selectedTxProvider, selectedFmtProvider) {
        case (.gemini, .gemini), (.openai, .openai):
            return true
        default:
            return false
        }
    }

    // MARK: - Provider Option Views

    private var deepgramOptions: some View {
        Group {
            DescribedToggle(
                title: "Smart Format",
                description: "Auto-formats numbers, dates, currencies, and adds punctuation",
                isOn: $dgSmartFormat
            )
            DescribedTextField(
                title: "Language",
                description: "ISO 639-1 language code (e.g. en, es, fr, ja). Leave empty to auto-detect.",
                placeholder: "Auto-detect",
                text: $dgLanguage
            )
            DescribedTextField(
                title: "Keywords",
                description: "Boost recognition of specific words or names, separated by commas",
                placeholder: "e.g. Kubernetes, Jira, OAuth",
                text: $dgKeywords
            )
        }
    }

    private var openAIOptions: some View {
        Group {
            DescribedTextField(
                title: "Language",
                description: "ISO 639-1 language code (e.g. en, es, fr). Improves accuracy and speed.",
                placeholder: "Auto-detect",
                text: $oaiLanguage
            )
            DescribedTextField(
                title: "Prompt",
                description: "Guide the model with context \u{2014} useful for domain-specific terms, names, or jargon it might mishear",
                placeholder: "e.g. The speaker discusses SwiftUI and Xcode",
                text: $oaiPrompt
            )
        }
    }

    private var geminiOptions: some View {
        Group {
            VStack(alignment: .leading, spacing: 3) {
                HStack {
                    Text("Temperature")
                    Slider(value: $geminiTemperature, in: 0...1, step: 0.1)
                    Text(String(format: "%.1f", geminiTemperature))
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                        .frame(width: 28)
                }
                SettingDescription(text: "Controls randomness. 0 = precise and deterministic, 1 = creative and varied. Lower is better for transcription.")
            }
        }
    }

    private var elevenLabsOptions: some View {
        Group {
            DescribedTextField(
                title: "Language",
                description: "ISO 639-1 language code (e.g. en, es, fr). Leave empty to auto-detect.",
                placeholder: "Auto-detect",
                text: $elLanguageCode
            )
        }
    }

    // MARK: - Style Preview Card

    private var stylePreview: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            VStack(alignment: .leading, spacing: 2) {
                Text(selectedStyle.label)
                    .font(.caption.weight(.semibold))
                Text(selectedStyle.description)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 14, alignment: .leading)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(.quaternary.opacity(0.3))

            // Before / After
            HStack(alignment: .top, spacing: 0) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Before")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.red.opacity(0.7))
                    Text(FormattingStyle.exampleInput)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)

                Divider()

                VStack(alignment: .leading, spacing: 4) {
                    Text("After")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.green.opacity(0.7))
                    Text(selectedStyle.exampleOutput)
                        .font(.caption)
                        .foregroundStyle(.primary)
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .fixedSize(horizontal: false, vertical: true)
        }
        .background(.quaternary.opacity(0.15))
        .clipShape(.rect(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(.quaternary, lineWidth: 1)
        )
        .animation(.easeInOut(duration: 0.15), value: fmtStyle)
    }

    // MARK: - Body

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                Form {
                    // General
                    Section {
                        Picker("Hotkey", selection: $hotkey) {
                            Text("fn / Globe").tag("fn")
                            Text("Option").tag("option")
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
                            TextField("API Key", text: $txApiKey, prompt: Text("Required"))

                            DescribedTextField(
                                title: "Model",
                                description: "Leave empty to use the default (\(selectedTxProvider.defaultModel))",
                                placeholder: selectedTxProvider.defaultModel,
                                text: $txModel
                            )

                            if selectedTxProvider == .deepgram {
                                deepgramOptions
                            } else if selectedTxProvider == .openai {
                                openAIOptions
                            } else if selectedTxProvider == .gemini {
                                geminiOptions
                            } else if selectedTxProvider == .elevenlabs {
                                elevenLabsOptions
                            }
                        }
                    } header: {
                        Text("Transcription")
                    } footer: {
                        if !hasTxProvider {
                            Text("Using Apple's built-in dictation \u{2014} free, on-device, no API key needed.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
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
                            let shareKey = fmtUseSameKey && canShareApiKey
                            TextField("API Key", text: shareKey ? $txApiKey : $fmtApiKey, prompt: Text("Required"))
                                .disabled(shareKey)

                            if canShareApiKey {
                                Toggle("Use same API key as transcription", isOn: $fmtUseSameKey)
                            }

                            DescribedTextField(
                                title: "Model",
                                description: "Leave empty to use the default (\(selectedFmtProvider.defaultModel))",
                                placeholder: selectedFmtProvider.defaultModel,
                                text: $fmtModel
                            )

                            Picker("Style", selection: $fmtStyle) {
                                ForEach(FormattingStyle.allCases, id: \.rawValue) { s in
                                    Text(s.label).tag(s.rawValue)
                                }
                            }
                            .pickerStyle(.segmented)

                            stylePreview
                        }
                    } header: {
                        Text("Formatting")
                    } footer: {
                        if !hasFmtProvider {
                            Text("No formatting \u{2014} raw transcription will be pasted as-is.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .formStyle(.grouped)
                .scrollContentBackground(.hidden)
            }
            .animation(.easeInOut(duration: 0.2), value: txProvider)
            .animation(.easeInOut(duration: 0.2), value: fmtProvider)

            Divider()

            // Buttons
            HStack {
                Button("Reset Onboarding") {
                    UserDefaults.standard.set(false, forKey: SettingsKey.onboardingComplete)
                    onSave?()
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .font(.caption)

                Spacer()

                Button("Cancel") { onCancel?() }
                    .keyboardShortcut(.cancelAction)

                Button("Save") {
                    let d = UserDefaults.standard
                    d.set(hotkey, forKey: SettingsKey.hotkey)
                    d.set(txProvider, forKey: SettingsKey.txProvider)
                    d.set(txApiKey, forKey: SettingsKey.txApiKey)
                    d.set(txModel, forKey: SettingsKey.txModel)
                    d.set(fmtProvider, forKey: SettingsKey.fmtProvider)
                    d.set(fmtUseSameKey ? "" : fmtApiKey, forKey: SettingsKey.fmtApiKey)
                    d.set(fmtModel, forKey: SettingsKey.fmtModel)
                    d.set(fmtStyle, forKey: SettingsKey.fmtStyle)

                    // Deepgram options
                    d.set(dgSmartFormat, forKey: SettingsKey.dgSmartFormat)
                    d.set(dgKeywords, forKey: SettingsKey.dgKeywords)
                    d.set(dgLanguage, forKey: SettingsKey.dgLanguage)

                    // OpenAI options
                    d.set(oaiLanguage, forKey: SettingsKey.oaiLanguage)
                    d.set(oaiPrompt, forKey: SettingsKey.oaiPrompt)

                    // Gemini options
                    d.set(geminiTemperature, forKey: SettingsKey.geminiTemperature)

                    // ElevenLabs options
                    d.set(elLanguageCode, forKey: SettingsKey.elLanguageCode)

                    onSave?()
                }
                .keyboardShortcut(.defaultAction)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
        }
        .frame(width: 500, height: 680)
    }
}

// MARK: - NSWindow wrapper

class SettingsWindow: NSWindow {
    weak var settingsDelegate: SettingsDelegate?

    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 500, height: 680),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )

        title = "Yap Settings"
        isReleasedWhenClosed = false
        center()
        loadUI()
    }

    private func loadUI() {
        var settingsView = SettingsView()

        settingsView.onSave = { [weak self] in
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

}
