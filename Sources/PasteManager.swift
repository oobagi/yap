import Cocoa

class PasteManager {
    /// Put text on the clipboard and simulate Cmd+V into the focused app.
    /// Restores the previous clipboard contents after a short delay.
    func paste(_ text: String) {
        let pasteboard = NSPasteboard.general
        let previous = pasteboard.string(forType: .string)
        
        // Set transcription on clipboard
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        
        // Small delay to ensure clipboard is ready
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
            self.simulateCmdV()
            
            // Restore previous clipboard after paste completes
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                pasteboard.clearContents()
                if let previous = previous {
                    pasteboard.setString(previous, forType: .string)
                }
            }
        }
    }
    
    private func simulateCmdV() {
        let source = CGEventSource(stateID: .combinedSessionState)
        
        // Virtual key 0x09 = 'V' on US keyboard layout
        let vKeyCode: CGKeyCode = 0x09
        
        guard let keyDown = CGEvent(keyboardEventSource: source, virtualKey: vKeyCode, keyDown: true),
              let keyUp = CGEvent(keyboardEventSource: source, virtualKey: vKeyCode, keyDown: false) else {
            return
        }
        
        keyDown.flags = .maskCommand
        keyUp.flags = .maskCommand
        
        keyDown.post(tap: .cgAnnotatedSessionEventTap)
        keyUp.post(tap: .cgAnnotatedSessionEventTap)
    }
}
