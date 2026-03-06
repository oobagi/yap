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
        overlayState.audioLevel = 0
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
                    .animation(.easeInOut(duration: 0.3), value: state.mode)
            }
        }
    }
    
    @ViewBuilder
    private var pillContent: some View {
        switch state.mode {
        case .recording:
            WaveformBars(level: CGFloat(state.audioLevel))
                .frame(width: 40, height: 24)
                .transition(.opacity.animation(.easeInOut(duration: 0.3)))
        case .processing:
            WaveLoadingAnimation()
                .frame(width: 40, height: 24)
                .transition(.opacity.animation(.easeInOut(duration: 0.3)))
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

struct WaveLoadingAnimation: View {
    let barCount = 5
    
    var body: some View {
        TimelineView(.animation) { timeline in
            let phase = timeline.date.timeIntervalSinceReferenceDate * .pi * 2
            
            HStack(spacing: 3) {
                ForEach(0..<barCount, id: \.self) { index in
                    let waveOffset = Double(index) / Double(barCount) * .pi * 2
                    let wave = sin(phase - waveOffset)
                    let normalized = (wave + 1) / 2
                    let minH: CGFloat = 4
                    let maxH: CGFloat = 22
                    let barHeight = minH + (maxH - minH) * CGFloat(normalized)
                    
                    RoundedRectangle(cornerRadius: 2)
                        .fill(Color.white.opacity(0.9))
                        .frame(width: 4, height: barHeight)
                }
            }
        }
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
    
    private var barHeight: CGFloat {
        let center = CGFloat(total - 1) / 2.0
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
    
    var body: some View {
        RoundedRectangle(cornerRadius: 2)
            .fill(Color.white.opacity(0.9))
            .frame(width: 4, height: barHeight)
            .animation(.easeOut(duration: 0.08), value: level)
    }
}
