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
    
    func updateBandLevels(_ levels: [Float]) {
        overlayState.bandLevels = levels
    }
    
    func showProcessing() {
        overlayState.mode = .processing
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
    @Published var bandLevels: [Float] = Array(repeating: 0, count: 11)
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
            WaveformBars(level: CGFloat(state.audioLevel), bandLevels: state.bandLevels, isProcessing: state.mode == .processing)
                .frame(width: 52, height: 28)
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
    var bandLevels: [Float]
    var isProcessing: Bool
    let barCount = 11
    
    var body: some View {
        if isProcessing {
            WaveAnimationBars(lastLevel: level, barCount: barCount)
        } else {
            AudioReactiveBars(bandLevels: bandLevels, barCount: barCount)
        }
    }
}

// MARK: - Recording: lightweight, no TimelineView
struct AudioReactiveBars: View {
    var bandLevels: [Float]
    let barCount: Int
    
    // Position scaling — dramatic center emphasis, relaxed edges
    private let positionScale: [CGFloat] = [0.2, 0.35, 0.55, 0.75, 0.92, 1.0, 0.94, 0.78, 0.58, 0.38, 0.22]
    
    var body: some View {
        HStack(spacing: 2) {
            ForEach(0..<barCount, id: \.self) { index in
                let bandLevel = index < bandLevels.count ? CGFloat(bandLevels[index]) : 0
                let scale = positionScale[index]
                
                let minH: CGFloat = 5
                let maxH: CGFloat = 28
                // Center bars can reach full max, edges capped much lower
                let barCeiling = minH + (maxH - minH) * scale
                
                // Power curve so bars spring up dramatically
                let driven = pow(bandLevel, 0.7)
                let barHeight = max(minH, min(barCeiling, minH + (barCeiling - minH) * driven))
                
                RoundedRectangle(cornerRadius: 1.5)
                    .fill(Color.white.opacity(0.9))
                    .frame(width: 3, height: barHeight)
                    .animation(.easeOut(duration: 0.1), value: bandLevel)
            }
        }
    }
}

// MARK: - Processing: TimelineView for wave animation
struct WaveAnimationBars: View {
    let lastLevel: CGFloat
    let barCount: Int
    
    @State private var displayLevel: CGFloat = 1
    @State private var waveStrength: CGFloat = 0
    @State private var startTime: Date? = nil
    
    var body: some View {
        TimelineView(.animation) { timeline in
            let elapsed = startTime.map { timeline.date.timeIntervalSince($0) } ?? 0
            // Sweep from well off-left to well off-right so the wave
            // fully exits before looping. With gaussian width 6.0,
            // need ~5 units of margin for the tail to fully disappear.
            let margin = 5.0
            let sweepRange = Double(barCount - 1) + margin * 2
            let t = elapsed.truncatingRemainder(dividingBy: 1.2) / 1.2
            let waveCenter = -margin + t * sweepRange
            
            HStack(spacing: 2) {
                ForEach(0..<barCount, id: \.self) { index in
                    // Audio-reactive base (decaying via displayLevel)
                    let center = CGFloat(barCount - 1) / 2.0
                    let distFromCenter = abs(CGFloat(index) - center) / center
                    let positionScale = 1.0 - (distFromCenter * 0.5)
                    let boosted = pow(lastLevel * displayLevel, 0.5)
                    let audioH = max(6.0, min(28.0, 6.0 + 22.0 * boosted * positionScale))
                    
                    // Wave overlay — very wide and gentle rolling wave
                    let distance = abs(Double(index) - waveCenter)
                    let wave = exp(-distance * distance / 6.0)
                    let waveH = 14.0 * CGFloat(wave) * waveStrength
                    
                    let barHeight = min(28.0, max(6.0, audioH + waveH))
                    
                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(Color.white.opacity(0.9))
                        .frame(width: 3, height: barHeight)
                }
            }
        }
        .onAppear {
            startTime = Date()
            displayLevel = 1
            waveStrength = 0
            withAnimation(.easeOut(duration: 0.35)) {
                displayLevel = 0
            }
            withAnimation(.easeIn(duration: 0.35).delay(0.15)) {
                waveStrength = 1
            }
        }
    }
}
