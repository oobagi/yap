import Cocoa
import SwiftUI

/// A floating pill-shaped overlay at the bottom of the screen
/// with audio-reactive waveform bars and a processing spinner.
class OverlayPanel: NSPanel {
    private let overlayState = OverlayState()
    
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
    
    init() {
        let width: CGFloat = 320
        let height: CGFloat = 80
        
        let screenFrame = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let x = screenFrame.midX - width / 2
        let y = screenFrame.minY + 60
        
        super.init(
            contentRect: NSRect(x: x, y: y, width: width, height: height),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        
        level = .floating
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false // no window-level shadow, SwiftUI handles it
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        isMovableByWindowBackground = false
        hidesOnDeactivate = false
        ignoresMouseEvents = true // clicks pass through entirely
        
        let hostingView = NSHostingView(rootView:
            OverlayView(state: overlayState)
                .frame(width: width, height: height)
        )
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
        // Don't zero audioLevel — let bars hold their last position
        // so the transition into the pulse animation is seamless
    }
    
    func showError(_ message: String) {
        overlayState.mode = .error(message)
        // Auto-dismiss after 2 seconds
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self] in
            self?.dismiss()
        }
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

enum OverlayMode: Equatable {
    case idle, recording, processing, error(String)
}

class OverlayState: ObservableObject {
    @Published var mode: OverlayMode = .idle
    @Published var audioLevel: Float = 0
}

// MARK: - SwiftUI Views

struct OverlayView: View {
    @ObservedObject var state: OverlayState
    
    var body: some View {
        Group {
            if state.mode != .idle {
                pillContent
                    .padding(.horizontal, 18)
                    .padding(.vertical, 10)
                    .background(
                        Capsule()
                            .fill(.ultraThinMaterial)
                            .shadow(color: .black.opacity(0.25), radius: 10, y: 3)
                    )

            }
        }
    }
    
    @ViewBuilder
    private var pillContent: some View {
        switch state.mode {
        case .recording, .processing:
            WaveformBars(level: CGFloat(state.audioLevel), isProcessing: state.mode == .processing)
                .frame(width: 56, height: 24)
        case .error(let message):
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundColor(.red)
                    .font(.system(size: 12))
                Text(message)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(.primary)
                    .lineLimit(1)
            }
        case .idle:
            EmptyView()
        }
    }
}

struct WaveformBars: View {
    var level: CGFloat
    var isProcessing: Bool
    let barCount = 7
    
    @State private var pulseStrength: CGFloat = 0
    
    var body: some View {
        TimelineView(.animation(paused: !isProcessing)) { timeline in
            let phase = isProcessing ? timeline.date.timeIntervalSinceReferenceDate : 0
            let t = phase.truncatingRemainder(dividingBy: 0.8) / 0.8
            let pulseCenter = t * Double(barCount - 1)
            
            HStack(spacing: 3) {
                ForEach(0..<barCount, id: \.self) { index in
                    let audioH = audioBarHeight(index: index)
                    
                    // Pulse: gaussian bump that sweeps left → right
                    let distance = abs(Double(index) - pulseCenter)
                    let pulse = exp(-distance * distance / 0.6)
                    let pulseH = 18.0 * CGFloat(pulse) * pulseStrength
                    
                    let minH: CGFloat = 3
                    let maxH: CGFloat = 24
                    let barHeight = min(maxH, max(minH, audioH + pulseH))
                    
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color.white.opacity(0.9))
                        .frame(width: 4, height: barHeight)
                        .animation(.easeOut(duration: 0.08), value: level)
                }
            }
        }
        .onChange(of: isProcessing) { processing in
            withAnimation(.easeInOut(duration: 0.4)) {
                pulseStrength = processing ? 1 : 0
            }
        }
    }
    
    private func audioBarHeight(index: Int) -> CGFloat {
        let center = CGFloat(barCount - 1) / 2.0
        let distFromCenter = abs(CGFloat(index) - center) / center
        let positionScale = 1.0 - (distFromCenter * 0.5)
        
        let minHeight: CGFloat = 3
        let maxHeight: CGFloat = 24
        
        let boosted = pow(level, 0.5)
        let targetHeight = minHeight + (maxHeight - minHeight) * boosted * positionScale
        
        let seed = sin(Double(index) * 2.5 + Double(level) * 8.0)
        let variation = CGFloat(seed) * 3.5 * boosted
        
        return max(minHeight, min(maxHeight, targetHeight + variation))
    }
}
