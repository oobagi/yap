import Cocoa
import QuartzCore

/// A floating pill-shaped overlay at the bottom of the screen
/// that shows recording/processing state — like Wispr Flow's indicator.
class OverlayPanel: NSPanel {
    private var visualEffect: NSVisualEffectView!
    private var indicatorDot: NSView!
    private var label: NSTextField!
    
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
    
    init() {
        let width: CGFloat = 180
        let height: CGFloat = 38
        
        // Center horizontally, 80px from bottom
        let screenFrame = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let x = screenFrame.midX - width / 2
        let y = screenFrame.minY + 80
        
        super.init(
            contentRect: NSRect(x: x, y: y, width: width, height: height),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        
        // Floating, transparent, always-on-top, doesn't steal focus
        level = .floating
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        isMovableByWindowBackground = false
        hidesOnDeactivate = false
        
        setupUI(width: width, height: height)
    }
    
    private func setupUI(width: CGFloat, height: CGFloat) {
        guard let contentView = contentView else { return }
        contentView.wantsLayer = true
        
        // Translucent dark pill background
        visualEffect = NSVisualEffectView(frame: contentView.bounds)
        visualEffect.autoresizingMask = [.width, .height]
        visualEffect.material = .hudWindow
        visualEffect.blendingMode = .behindWindow
        visualEffect.state = .active
        visualEffect.wantsLayer = true
        visualEffect.layer?.cornerRadius = height / 2
        visualEffect.layer?.masksToBounds = true
        contentView.addSubview(visualEffect)
        
        // Red indicator dot
        let dotSize: CGFloat = 10
        indicatorDot = NSView(frame: NSRect(
            x: 20,
            y: (height - dotSize) / 2,
            width: dotSize,
            height: dotSize
        ))
        indicatorDot.wantsLayer = true
        indicatorDot.layer?.backgroundColor = NSColor.systemRed.cgColor
        indicatorDot.layer?.cornerRadius = dotSize / 2
        visualEffect.addSubview(indicatorDot)
        
        // Label
        label = NSTextField(labelWithString: "")
        label.font = .systemFont(ofSize: 13, weight: .medium)
        label.textColor = .white
        label.frame = NSRect(x: 38, y: (height - 20) / 2, width: width - 52, height: 20)
        label.alignment = .left
        visualEffect.addSubview(label)
    }
    
    // MARK: - States
    
    func showRecording() {
        label.stringValue = "Recording…"
        indicatorDot.layer?.backgroundColor = NSColor.systemRed.cgColor
        startPulse()
        
        // Fade in
        alphaValue = 0
        orderFront(nil)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.15
            self.animator().alphaValue = 1
        }
    }
    
    func showProcessing() {
        stopPulse()
        label.stringValue = "Transcribing…"
        indicatorDot.layer?.backgroundColor = NSColor.systemYellow.cgColor
        indicatorDot.layer?.opacity = 1.0
    }
    
    func dismiss() {
        stopPulse()
        
        // Fade out
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.2
            self.animator().alphaValue = 0
        }, completionHandler: {
            self.orderOut(nil)
        })
    }
    
    // MARK: - Pulse Animation
    
    private func startPulse() {
        let pulse = CABasicAnimation(keyPath: "opacity")
        pulse.fromValue = 1.0
        pulse.toValue = 0.25
        pulse.duration = 0.6
        pulse.autoreverses = true
        pulse.repeatCount = .infinity
        pulse.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        indicatorDot.layer?.add(pulse, forKey: "pulse")
    }
    
    private func stopPulse() {
        indicatorDot.layer?.removeAnimation(forKey: "pulse")
        indicatorDot.layer?.opacity = 1.0
    }
}
