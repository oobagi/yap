import Cocoa

protocol SettingsDelegate: AnyObject {
    func settingsDidChange()
}

class SettingsWindow: NSWindow {
    weak var settingsDelegate: SettingsDelegate?
    
    private var stylePopup: NSPopUpButton!
    private var providerPopup: NSPopUpButton!
    private var apiKeyField: NSTextField!
    private var modelField: NSTextField!
    private var hotkeyPopup: NSPopUpButton!
    private var exampleBox: NSBox!
    private var exampleLabel: NSTextField!
    
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
        
        setupUI()
        loadSettings()
    }
    
    private func setupUI() {
        guard let contentView = contentView else { return }
        
        let padding: CGFloat = 20
        let labelWidth: CGFloat = 110
        let fieldX: CGFloat = padding + labelWidth + 8
        let fieldWidth: CGFloat = 480 - fieldX - padding
        var y: CGFloat = 470
        let rowHeight: CGFloat = 36
        
        // Hotkey
        addLabel("Hotkey:", x: padding, y: y, width: labelWidth, to: contentView)
        hotkeyPopup = NSPopUpButton(frame: NSRect(x: fieldX, y: y, width: fieldWidth, height: 26))
        hotkeyPopup.addItems(withTitles: ["fn / Globe 🌐", "Option ⌥"])
        contentView.addSubview(hotkeyPopup)
        y -= rowHeight
        
        // Style
        addLabel("Formatting:", x: padding, y: y, width: labelWidth, to: contentView)
        stylePopup = NSPopUpButton(frame: NSRect(x: fieldX, y: y, width: fieldWidth, height: 26))
        for style in FormattingStyle.allCases {
            stylePopup.addItem(withTitle: style.label)
        }
        stylePopup.target = self
        stylePopup.action = #selector(styleChanged(_:))
        contentView.addSubview(stylePopup)
        y -= rowHeight
        
        // Provider
        addLabel("AI Provider:", x: padding, y: y, width: labelWidth, to: contentView)
        providerPopup = NSPopUpButton(frame: NSRect(x: fieldX, y: y, width: fieldWidth, height: 26))
        for provider in APIProvider.allCases {
            providerPopup.addItem(withTitle: provider.label)
        }
        providerPopup.target = self
        providerPopup.action = #selector(providerChanged(_:))
        contentView.addSubview(providerPopup)
        y -= rowHeight
        
        // API Key
        addLabel("API Key:", x: padding, y: y, width: labelWidth, to: contentView)
        apiKeyField = NSTextField(frame: NSRect(x: fieldX, y: y, width: fieldWidth, height: 24))
        apiKeyField.placeholderString = "sk-..."
        contentView.addSubview(apiKeyField)
        y -= rowHeight
        
        // Model
        addLabel("Model:", x: padding, y: y, width: labelWidth, to: contentView)
        modelField = NSTextField(frame: NSRect(x: fieldX, y: y, width: fieldWidth, height: 24))
        modelField.placeholderString = "Leave blank for default"
        contentView.addSubview(modelField)
        y -= rowHeight + 10
        
        // Example box
        exampleBox = NSBox(frame: NSRect(x: padding, y: 60, width: 480 - padding * 2, height: y - 60))
        exampleBox.title = "Example"
        exampleBox.titlePosition = .atTop
        exampleBox.contentViewMargins = NSSize(width: 10, height: 8)
        contentView.addSubview(exampleBox)
        
        exampleLabel = NSTextField(wrappingLabelWithString: "")
        exampleLabel.font = .systemFont(ofSize: 11.5)
        exampleLabel.textColor = .secondaryLabelColor
        exampleLabel.isSelectable = true
        exampleLabel.frame = NSRect(x: 0, y: 0, width: exampleBox.frame.width - 30, height: exampleBox.frame.height - 30)
        exampleBox.contentView?.addSubview(exampleLabel)
        
        updateExample()
        
        // Buttons
        let buttonY: CGFloat = 18
        
        let saveButton = NSButton(frame: NSRect(x: 480 - padding - 80, y: buttonY, width: 80, height: 30))
        saveButton.title = "Save"
        saveButton.bezelStyle = .rounded
        saveButton.keyEquivalent = "\r"
        saveButton.target = self
        saveButton.action = #selector(save(_:))
        contentView.addSubview(saveButton)
        
        let cancelButton = NSButton(frame: NSRect(x: 480 - padding - 170, y: buttonY, width: 80, height: 30))
        cancelButton.title = "Cancel"
        cancelButton.bezelStyle = .rounded
        cancelButton.keyEquivalent = "\u{1b}"
        cancelButton.target = self
        cancelButton.action = #selector(cancel(_:))
        contentView.addSubview(cancelButton)
    }
    
    private func addLabel(_ text: String, x: CGFloat, y: CGFloat, width: CGFloat, to view: NSView) {
        let label = NSTextField(labelWithString: text)
        label.frame = NSRect(x: x, y: y + 2, width: width, height: 20)
        label.alignment = .right
        label.font = .systemFont(ofSize: 13)
        view.addSubview(label)
    }
    
    private func updateExample() {
        let selectedIndex = stylePopup?.indexOfSelectedItem ?? 0
        let style = FormattingStyle.allCases[selectedIndex]
        
        var lines = "Someone says:\n"
        lines += "\"\(FormattingStyle.exampleInput)\"\n\n"
        
        for mode in FormattingStyle.allCases {
            let marker = mode == style ? "→" : "  "
            let bold = mode == style
            lines += "\(marker) \(mode.label): \(mode.description)\n"
            lines += "    \"\(mode.exampleOutput)\"\n\n"
        }
        
        let attributed = NSMutableAttributedString(string: lines)
        let fullRange = NSRange(location: 0, length: attributed.length)
        attributed.addAttribute(.font, value: NSFont.systemFont(ofSize: 11), range: fullRange)
        attributed.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: fullRange)
        
        // Bold the selected mode's line
        let selectedHeader = "\(style.label): \(style.description)"
        if let range = lines.range(of: selectedHeader) {
            let nsRange = NSRange(range, in: lines)
            attributed.addAttribute(.font, value: NSFont.boldSystemFont(ofSize: 11), range: nsRange)
            attributed.addAttribute(.foregroundColor, value: NSColor.labelColor, range: nsRange)
        }
        
        // Bold the arrow
        if let range = lines.range(of: "→") {
            let nsRange = NSRange(range, in: lines)
            attributed.addAttribute(.foregroundColor, value: NSColor.controlAccentColor, range: nsRange)
            attributed.addAttribute(.font, value: NSFont.boldSystemFont(ofSize: 11), range: nsRange)
        }
        
        exampleLabel.attributedStringValue = attributed
    }
    
    @objc private func styleChanged(_ sender: NSPopUpButton) {
        updateExample()
    }
    
    // MARK: - Load / Save
    
    private func loadSettings() {
        let config = Self.loadConfig()
        
        // Hotkey
        let hotkey = config["hotkey"] as? String ?? "fn"
        hotkeyPopup.selectItem(at: hotkey == "option" ? 1 : 0)
        
        // Formatting settings
        let formatting = config["formatting"] as? [String: Any] ?? [:]
        
        let styleName = formatting["style"] as? String ?? "verbatim"
        if let style = FormattingStyle.allCases.firstIndex(where: { $0.rawValue == styleName }) {
            stylePopup.selectItem(at: style)
        }
        
        let providerName = formatting["provider"] as? String ?? "none"
        if let provider = APIProvider.allCases.firstIndex(where: { $0.rawValue == providerName }) {
            providerPopup.selectItem(at: provider)
        }
        
        apiKeyField.stringValue = formatting["api_key"] as? String ?? ""
        modelField.stringValue = formatting["model"] as? String ?? ""
        
        updateExample()
    }
    
    @objc private func save(_ sender: Any) {
        let hotkey = hotkeyPopup.indexOfSelectedItem == 1 ? "option" : "fn"
        let style = FormattingStyle.allCases[stylePopup.indexOfSelectedItem]
        let provider = APIProvider.allCases[providerPopup.indexOfSelectedItem]
        
        let config: [String: Any] = [
            "hotkey": hotkey,
            "formatting": [
                "style": style.rawValue,
                "provider": provider.rawValue,
                "api_key": apiKeyField.stringValue,
                "model": modelField.stringValue
            ] as [String: Any]
        ]
        
        Self.saveConfig(config)
        settingsDelegate?.settingsDidChange()
        close()
    }
    
    @objc private func cancel(_ sender: Any) {
        close()
    }
    
    @objc private func providerChanged(_ sender: NSPopUpButton) {
        let provider = APIProvider.allCases[sender.indexOfSelectedItem]
        if modelField.stringValue.isEmpty || APIProvider.allCases.contains(where: { $0.defaultModel == modelField.stringValue }) {
            modelField.placeholderString = provider.defaultModel.isEmpty ? "Leave blank for default" : provider.defaultModel
        }
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
