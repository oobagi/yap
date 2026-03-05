import Cocoa

/// Monitors a modifier key (fn, Option, etc.) via CGEventTap.
/// Calls onKeyDown when the modifier is pressed and onKeyUp when released.
class HotkeyManager {
    var onKeyDown: () -> Void
    var onKeyUp: () -> Void
    
    private let modifierMask: CGEventFlags
    private var eventTap: CFMachPort?
    private(set) var isHeld = false
    
    init(modifierMask: UInt64, onKeyDown: @escaping () -> Void, onKeyUp: @escaping () -> Void) {
        self.modifierMask = CGEventFlags(rawValue: modifierMask)
        self.onKeyDown = onKeyDown
        self.onKeyUp = onKeyUp
    }
    
    /// Start the event tap. Returns false if accessibility permission is missing.
    func start() -> Bool {
        let mask: CGEventMask = (1 << CGEventType.flagsChanged.rawValue)
        let selfPtr = Unmanaged.passUnretained(self).toOpaque()
        
        guard let tap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: hotkeyCallback,
            userInfo: selfPtr
        ) else {
            return false
        }
        
        eventTap = tap
        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        return true
    }
    
    func stop() {
        if let tap = eventTap {
            CGEvent.tapEnable(tap: tap, enable: false)
            eventTap = nil
        }
    }
    
    fileprivate func handleEvent(type: CGEventType, event: CGEvent) -> Unmanaged<CGEvent>? {
        // Re-enable if system disabled the tap
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            if let tap = eventTap {
                CGEvent.tapEnable(tap: tap, enable: true)
            }
            return Unmanaged.passUnretained(event)
        }
        
        guard type == .flagsChanged else {
            return Unmanaged.passUnretained(event)
        }
        
        let flags = event.flags
        let triggerActive = flags.contains(modifierMask)
        
        // Only trigger if no other modifiers are held (don't steal fn+arrows, option+letter, etc.)
        let otherModifiers: CGEventFlags = [.maskShift, .maskControl, .maskAlternate, .maskCommand]
        let relevantOthers = flags.intersection(otherModifiers).subtracting(modifierMask)
        let hasOtherModifiers = !relevantOthers.isEmpty
        
        if triggerActive && !hasOtherModifiers && !isHeld {
            isHeld = true
            DispatchQueue.main.async { self.onKeyDown() }
            return nil // consume event (suppress system fn/option behavior)
        } else if !triggerActive && isHeld {
            isHeld = false
            DispatchQueue.main.async { self.onKeyUp() }
            return nil // consume release too
        }
        
        return Unmanaged.passUnretained(event)
    }
}

/// Global C callback for the event tap — forwards to HotkeyManager instance.
private func hotkeyCallback(
    proxy: CGEventTapProxy,
    type: CGEventType,
    event: CGEvent,
    refcon: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    guard let refcon = refcon else { return Unmanaged.passUnretained(event) }
    let manager = Unmanaged<HotkeyManager>.fromOpaque(refcon).takeUnretainedValue()
    return manager.handleEvent(type: type, event: event)
}
