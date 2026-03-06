import Cocoa
import SwiftUI

/// A floating pill-shaped overlay at the bottom of the screen
/// with audio-reactive waveform bars and a processing spinner.
class OverlayPanel: NSPanel {
    private let overlayState = OverlayState()
    
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
    
    init() {
        let width: CGFloat = 120
        let height: CGFloat = 40
        
        let screenFrame = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let x = screenFrame.midX - width / 2
        let y = screenFrame.minY + 80
        
        super.init(
            contentRect: NSRect(x: x, y: y, width: width, height: height),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        
        level = .floating
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        isMovableByWindowBackground = false
        hidesOnDeactivate = false
        
        let hostingView = NSHostingView(rootView: OverlayView(state: overlayState))
        hostingView.frame = NSRect(x: 0, y: 0, width: width, height: height)
        contentView = hostingView
    }
    
    func showRecording() {
        overlayState.mode = .recording
        overlayState.audioLevel = 0
        
        alphaValue = 0
        orderFront(nil)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.15
            self.animator().alphaValue = 1
        }
    }
    
    func updateLevel(_ level: Float) {
        overlayState.audioLevel = level
    }
    
    func showProcessing() {
        overlayState.mode = .processing
        overlayState.audioLevel = 0
    }
    
    func dismiss() {
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.2
            self.animator().alphaValue = 0
        }, completionHandler: {
            self.orderOut(nil)
            self.overlayState.mode = .idle
        })
    }
}

// MARK: - State

enum OverlayMode {
    case idle, recording, processing
}

class OverlayState: ObservableObject {
    @Published var mode: OverlayMode = .idle
    @Published var audioLevel: Float = 0
}

// MARK: - SwiftUI Views

struct OverlayView: View {
    @ObservedObject var state: OverlayState
    
    var body: some View {
        HStack(spacing: 6) {
            switch state.mode {
            case .idle:
                EmptyView()
            case .recording:
                WaveformBars(level: CGFloat(state.audioLevel))
            case .processing:
                WaveformBars(level: 0.05) // static low bars
                ProgressView()
                    .scaleEffect(0.5)
                    .frame(width: 14, height: 14)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .frame(height: 40)
        .background(
            VisualEffectBackground()
        )
        .clipShape(Capsule())
        .shadow(color: .black.opacity(0.2), radius: 8, y: 2)
    }
}

struct WaveformBars: View {
    var level: CGFloat
    let barCount = 5
    
    var body: some View {
        HStack(spacing: 3) {
            ForEach(0..<barCount, id: \.self) { index in
                WaveformBar(level: level, index: index, total: barCount)
            }
        }
    }
}

struct WaveformBar: View {
    var level: CGFloat
    var index: Int
    var total: Int
    
    // Each bar gets a slightly different height based on its position
    // Center bars are taller, edge bars shorter (like a real waveform)
    private var barHeight: CGFloat {
        let center = CGFloat(total - 1) / 2.0
        let distFromCenter = abs(CGFloat(index) - center) / center
        let positionScale = 1.0 - (distFromCenter * 0.5)
        
        let minHeight: CGFloat = 3
        let maxHeight: CGFloat = 24
        
        // Apply curve to make level changes more dramatic
        let boosted = pow(level, 0.5) // square root makes quiet sounds more visible
        let targetHeight = minHeight + (maxHeight - minHeight) * boosted * positionScale
        
        // Add variation per bar so they don't all move identically
        let seed = sin(Double(index) * 2.5 + Double(level) * 8.0)
        let variation = CGFloat(seed) * 3.5 * boosted
        
        return max(minHeight, min(maxHeight, targetHeight + variation))
    }
    
    var body: some View {
        RoundedRectangle(cornerRadius: 2)
            .fill(Color.white.opacity(0.9))
            .frame(width: 4, height: barHeight)
            .animation(.easeOut(duration: 0.08), value: level)
    }
}

struct VisualEffectBackground: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .hudWindow
        view.blendingMode = .behindWindow
        view.state = .active
        return view
    }
    
    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {}
}
