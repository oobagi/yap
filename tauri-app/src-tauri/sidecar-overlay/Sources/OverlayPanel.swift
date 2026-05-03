import Cocoa
import SwiftUI

// MARK: - Hit-testing

/// NSView subclass that receives first-click and forwards to a callback.
class ClickTargetView: NSView {
    var onClick: (() -> Void)?
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override func mouseDown(with event: NSEvent) { onClick?() }
}

/// Content view that passes clicks through except on ClickTargetViews and the pill region.
class OverlayContentView: NSView {
    var pillHitRegion: NSRect = .zero

    override func hitTest(_ point: NSPoint) -> NSView? {
        for subview in subviews.reversed() {
            guard subview is ClickTargetView else { continue }
            let local = convert(point, to: subview)
            if subview.bounds.contains(local) { return subview }
        }
        if pillHitRegion.contains(point) {
            return super.hitTest(point)
        }
        return nil
    }
}

// MARK: - Overlay Panel

class OverlayPanel: NSPanel {
    let overlayState = OverlayState()
    private var errorDismissWork: DispatchWorkItem?
    private var pauseTarget: ClickTargetView?
    private var stopTarget: ClickTargetView?
    private var contentOverlay: OverlayContentView?

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    init() {
        let width = OverlayLayout.panelWidth
        let height = OverlayLayout.panelHeight

        let screenFrame = NSScreen.main?.frame ?? OverlayLayout.fallbackScreenFrame
        let x = screenFrame.midX - width / 2
        let y = screenFrame.minY + OverlayLayout.visiblePanelBottomInset - height

        super.init(
            contentRect: NSRect(x: x, y: y, width: width, height: height),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )

        level = .floating
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        isMovableByWindowBackground = false
        hidesOnDeactivate = false

        let container = OverlayContentView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        contentOverlay = container

        let hostingView = NSHostingView(rootView:
            OverlayView(state: overlayState)
                .frame(width: width, height: height)
        )
        hostingView.frame = NSRect(x: 0, y: 0, width: width, height: height)
        container.addSubview(hostingView)

        let pause = ClickTargetView(frame: .zero)
        pause.onClick = { [weak self] in self?.overlayState.onPauseResume?() }
        container.addSubview(pause)
        pauseTarget = pause

        let stop = ClickTargetView(frame: .zero)
        stop.onClick = { [weak self] in self?.overlayState.onStop?() }
        container.addSubview(stop)
        stopTarget = stop

        contentView = container
        updatePillTarget()
    }

    private var onScreenFrame: NSRect {
        let screenFrame = NSScreen.main?.frame ?? OverlayLayout.fallbackScreenFrame
        return NSRect(
            x: screenFrame.midX - frame.width / 2,
            y: screenFrame.minY + OverlayLayout.visiblePanelBottomInset - frame.height,
            width: frame.width,
            height: frame.height
        )
    }

    private var offScreenFrame: NSRect {
        let screenFrame = NSScreen.main?.frame ?? OverlayLayout.fallbackScreenFrame
        return NSRect(
            x: screenFrame.midX - frame.width / 2,
            y: screenFrame.minY - frame.height,
            width: frame.width,
            height: frame.height
        )
    }

    private func showAtRest() {
        setFrame(onScreenFrame, display: true)
        orderFront(nil)
    }

    private func slideIn() {
        orderFront(nil)
        let target = onScreenFrame
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.5
            ctx.timingFunction = CAMediaTimingFunction(controlPoints: 0.16, 1, 0.3, 1)
            animator().setFrame(target, display: true)
        }
    }

    private func slideOut() {
        let target = offScreenFrame
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.4
            ctx.timingFunction = CAMediaTimingFunction(controlPoints: 0.4, 0, 1, 1)
            animator().setFrame(target, display: true)
        }
    }

    // MARK: - Public API (called from IPC)

    func applyState(_ state: String, handsFree: Bool, paused: Bool, elapsed: Double) {
        errorDismissWork?.cancel()
        errorDismissWork = nil

        let wasIdle = overlayState.mode == .idle

        switch state {
        case "recording":
            if wasIdle {
                overlayState.audioLevel = 0
                alphaValue = 1
                if !overlayState.alwaysVisible { slideIn() } else { showAtRest() }
            }
            withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.5)) {
                overlayState.mode = .recording
            }
            overlayState.isHandsFree = handsFree
            overlayState.isPaused = paused
            overlayState.handsFreeElapsed = elapsed
            updateButtonTargets()

        case "processing":
            overlayState.isHandsFree = false
            overlayState.isPaused = false
            overlayState.mode = .processing

        case "idle":
            overlayState.isHandsFree = false
            overlayState.isPaused = false
            withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.35)) {
                overlayState.mode = .idle
            }
            if !overlayState.alwaysVisible && overlayState.onboardingStep == nil {
                slideOut()
            }

        case "noSpeech":
            withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.35)) {
                overlayState.mode = .noSpeech
            }
            overlayState.shakeToken = UUID()

        default:
            break
        }
        updatePillTarget()
    }

    func applyLevels(level: Float, bars: [Float]) {
        overlayState.audioLevel = level
        overlayState.bandLevels = bars
        updateButtonTargets()
    }

    func applyError(_ message: String) {
        overlayState.mode = .error(message)
        updatePillTarget()
        errorDismissWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            self?.applyState("idle", handsFree: false, paused: false, elapsed: 0)
        }
        errorDismissWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5, execute: work)
    }

    func applyOnboarding(step: String?, text: String?, hotkeyLabel: String?) {
        if let label = hotkeyLabel {
            overlayState.hotkeyLabel = label
        }
        if let stepStr = step, let parsed = OnboardingStep.from(stepStr) {
            withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.5)) {
                overlayState.onboardingStep = parsed
            }
            overlayState.onboardingText = text ?? ""
            showAtRest()
        } else {
            withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.35)) {
                overlayState.onboardingStep = nil
            }
            overlayState.onboardingText = ""
            if !overlayState.alwaysVisible && overlayState.mode == .idle {
                slideOut()
            }
        }
        updatePillTarget()
    }

    func applyOnboardingPress(_ pressed: Bool) {
        withAnimation(.spring(response: pressed ? 0.2 : 0.35, dampingFraction: pressed ? 0.7 : 0.5)) {
            overlayState.isPressed = pressed
        }
    }

    func applyConfig(gradientEnabled: Bool?, alwaysVisible: Bool?, hotkeyLabel: String?) {
        if let g = gradientEnabled { overlayState.gradientEnabled = g }
        if let label = hotkeyLabel { overlayState.hotkeyLabel = label }
        if let visible = alwaysVisible {
            overlayState.alwaysVisible = visible
            guard overlayState.mode == .idle && overlayState.onboardingStep == nil else { return }
            if visible { showAtRest() } else { slideOut() }
        }
    }

    func applyCelebrating() {
        overlayState.celebratingToken = UUID()
    }

    // MARK: - Private

    private func updateButtonTargets() {
        guard let pause = pauseTarget, let stop = stopTarget else { return }
        let cx = frame.width / 2
        let cy = OverlayLayout.controlCenterY - OverlayLayout.expandedStackYOffset

        let scale: CGFloat
        if overlayState.mode == .recording && !overlayState.isPaused {
            let lvl = min(CGFloat(overlayState.audioLevel), 1.0)
            scale = OverlayLayout.controlScale * (1.0 + pow(lvl, 1.5) * OverlayLayout.audioBounceScale)
        } else {
            scale = OverlayLayout.controlScale
        }

        let targetRadius = OverlayLayout.controlButtonRadius * scale + OverlayLayout.controlHitPadding
        let pauseCX = cx - OverlayLayout.controlButtonSpacing * scale
        let stopCX  = cx + OverlayLayout.controlButtonSpacing * scale

        pause.frame = NSRect(x: pauseCX - targetRadius, y: cy - targetRadius,
                             width: targetRadius * 2, height: targetRadius * 2)
        stop.frame  = NSRect(x: stopCX - targetRadius, y: cy - targetRadius,
                             width: targetRadius * 2, height: targetRadius * 2)
    }

    private func updatePillTarget() {
        let cx = frame.width / 2
        let isExpanded = overlayState.mode != .idle || overlayState.onboardingStep != nil
        switch overlayState.mode {
        case .processing:
            contentOverlay?.pillHitRegion = .zero
        default:
            if isExpanded {
                let width = OverlayLayout.activePillHitWidth
                let y = OverlayLayout.activePillHitBaseY - OverlayLayout.expandedStackYOffset
                contentOverlay?.pillHitRegion = NSRect(
                    x: cx - width / 2,
                    y: y,
                    width: width,
                    height: OverlayLayout.activePillHitHeight
                )
            } else {
                let width = OverlayLayout.idlePillHitWidth
                contentOverlay?.pillHitRegion = NSRect(
                    x: cx - width / 2,
                    y: OverlayLayout.idlePillHitY,
                    width: width,
                    height: OverlayLayout.idlePillHitHeight
                )
            }
        }
    }
}

// MARK: - State

enum OverlayMode: Equatable {
    case idle, recording, processing, noSpeech, error(String)
}

enum OnboardingStep: Hashable {
    case tryIt
    case nice
    case doubleTapTip
    case clickTip
    case apiTip
    case formattingTip
    case welcome

    static func from(_ str: String) -> OnboardingStep? {
        switch str {
        case "tryIt": return .tryIt
        case "nice": return .nice
        case "doubleTapTip": return .doubleTapTip
        case "clickTip": return .clickTip
        case "apiTip": return .apiTip
        case "formattingTip": return .formattingTip
        case "welcome": return .welcome
        default: return nil
        }
    }
}

struct ShakeEffect: GeometryEffect {
    var progress: CGFloat = 0
    var animatableData: CGFloat {
        get { progress }
        set { progress = newValue }
    }
    func effectValue(size: CGSize) -> ProjectionTransform {
        let offset = 4 * sin(progress * .pi * 6) * (1 - progress)
        return ProjectionTransform(CGAffineTransform(translationX: offset, y: 0))
    }
}

class OverlayState: ObservableObject {
    @Published var mode: OverlayMode = .idle
    @Published var audioLevel: Float = 0
    @Published var bandLevels: [Float] = Array(repeating: 0, count: 11)
    @Published var onboardingStep: OnboardingStep? = nil
    @Published var onboardingText: String = ""
    @Published var hotkeyLabel: String = "fn"
    @Published var shakeToken: UUID = UUID()
    @Published var isPressed: Bool = false
    @Published var isHandsFree: Bool = false
    @Published var isPaused: Bool = false
    @Published var isHovering: Bool = false
    @Published var gradientEnabled: Bool = true
    @Published var alwaysVisible: Bool = true
    @Published var handsFreeElapsed: TimeInterval = 0
    @Published var celebratingToken: UUID = UUID()
    var onPauseResume: (() -> Void)?
    var onStop: (() -> Void)?
    var onClickToRecord: (() -> Void)?
    var isOnboarding: Bool { onboardingStep != nil }
}

// MARK: - SwiftUI Views

struct OverlayView: View {
    @ObservedObject var state: OverlayState
    @State private var shakeProgress: CGFloat = 0
    @State private var celebrationPhase: Double = 0

    private var isActive: Bool { state.mode != .idle || state.isOnboarding }
    private var isExpanded: Bool { state.mode != .idle || state.isOnboarding }
    private var isMinimized: Bool { state.mode == .idle && !state.isOnboarding }

    private var gradientEnergy: CGFloat {
        switch state.mode {
        case .recording: return 1.0
        case .processing: return 0.6
        default:
            if state.isHovering { return 0.15 }
            return state.isOnboarding ? 0.3 : 0.4
        }
    }

    private var showGradient: Bool { (isExpanded || state.isHovering) && state.gradientEnabled }
    private var stackYOffset: CGFloat {
        isExpanded ? OverlayLayout.expandedStackYOffset : OverlayLayout.minimizedStackYOffset
    }

    private var audioBounceFactor: CGFloat {
        guard state.mode == .recording, !state.isPaused else { return 1.0 }
        let level = min(CGFloat(state.audioLevel), 1.0)
        return 1.0 + pow(level, 1.5) * OverlayLayout.audioBounceScale
    }

    var body: some View {
        ZStack {
            if showGradient {
                LavaLampBackground(energy: gradientEnergy, celebrationPhase: celebrationPhase)
                    .allowsHitTesting(false)
                    .offset(y: stackYOffset)
                    .transition(.opacity.combined(with: .offset(y: 60)))
                    .animation(.easeInOut(duration: 0.8), value: gradientEnergy)
                    .animation(.spring(response: 0.4, dampingFraction: 0.8), value: stackYOffset)
            }

            VStack(spacing: 0) {
                Spacer()

                VStack(spacing: 8) {
                    // Shared prompt card for onboarding.
                    if state.onboardingStep != nil,
                       state.mode == .idle || state.mode == .noSpeech {
                        PromptCardView(step: state.onboardingStep!, hotkeyLabel: state.hotkeyLabel)
                            .id(state.onboardingStep)
                            .transition(.opacity.combined(with: .scale(scale: 0.95, anchor: .bottom)))
                    }

                    if case .error(let message) = state.mode {
                        ErrorCardView(message: message)
                            .transition(.opacity.combined(with: .scale(scale: 0.95, anchor: .bottom)))
                    }

                    if state.mode == .recording && state.handsFreeElapsed >= 10 {
                        Text(formatElapsed(state.handsFreeElapsed))
                            .font(.system(size: 11, weight: .medium, design: .monospaced))
                            .foregroundColor(.white.opacity(0.5))
                            .fixedSize()
                            .transition(.opacity.combined(with: .offset(y: 12)))
                    }

                    ZStack {
                        if isActive {
                            pillContent
                                .transition(.opacity)
                        }
                    }
                    .fixedSize()
                    .frame(minWidth: 40, minHeight: isExpanded ? 28 : 8)
                    .padding(.horizontal, state.isHandsFree ? 7 : 12)
                    .padding(.vertical, 6)
                    .background(
                        ZStack {
                            Capsule().fill(Color.black.opacity(isExpanded ? 0.75 : 0.4))
                            Capsule().fill(.thinMaterial)
                        }
                        .shadow(color: .black.opacity(isExpanded ? 0.35 : 0.1),
                                radius: isExpanded ? 16 : 6, y: isExpanded ? 4 : 2)
                    )
                    .overlay(
                        Capsule()
                            .strokeBorder(Color.white.opacity(isExpanded ? 0.3 : 0.35),
                                          lineWidth: isExpanded ? 1 : 1.5)
                    )
                    .contentShape(Capsule())
                    .onHover { hovering in
                        guard isMinimized else { return }
                        withAnimation(.easeOut(duration: 0.35)) {
                            state.isHovering = hovering
                        }
                    }
                    .onTapGesture {
                        guard state.mode != .processing else { return }
                        state.onClickToRecord?()
                    }
                    .scaleEffect(pillScale * audioBounceFactor * (state.isPressed ? 0.85 : 1.0) * (state.mode == .processing ? 0.8 : 1.0))
                    .opacity(state.isPressed ? 0.7 : 1.0)
                    .modifier(ShakeEffect(progress: shakeProgress))
                    .animation(.spring(response: 0.25, dampingFraction: 0.45, blendDuration: 0.05), value: state.audioLevel)
                    .animation(.spring(response: 0.4, dampingFraction: 0.8), value: state.onboardingStep)
                    .animation(.spring(response: 0.4, dampingFraction: 0.8), value: state.mode)
                    .animation(.spring(response: 0.35, dampingFraction: 0.8), value: state.isHandsFree)
                    .animation(.spring(response: 0.3, dampingFraction: 0.8), value: state.isPaused)
                    .animation(.spring(response: 0.3, dampingFraction: 0.7), value: state.isHovering)
                    .overlay(alignment: .top) {
                        if state.isHovering && isMinimized {
                            Text("Click to start transcribing")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundColor(.white)
                                .shadow(color: .black.opacity(0.6), radius: 6, y: 2)
                                .fixedSize()
                                .allowsHitTesting(false)
                                .offset(y: OverlayLayout.hoverTooltipYOffset)
                                .transition(.opacity.combined(with: .offset(y: OverlayLayout.hoverTooltipTransitionYOffset)))
                        }
                    }
                }
                .offset(y: stackYOffset)
                .animation(.spring(response: 0.4, dampingFraction: 0.8), value: state.onboardingStep)
                .animation(.spring(response: 0.4, dampingFraction: 0.8), value: state.mode)
                .animation(.spring(response: 0.45, dampingFraction: 0.7), value: state.mode == .recording && state.handsFreeElapsed >= 10)
                .animation(.spring(response: 0.4, dampingFraction: 0.8), value: stackYOffset)

                Spacer().frame(height: OverlayLayout.bottomSpacerHeight)
            }
        }
        .onChange(of: state.shakeToken) { _ in
            shakeProgress = 0
            withAnimation(.easeOut(duration: 0.5)) { shakeProgress = 1 }
        }
        .onChange(of: state.mode) { _ in
            if state.mode != .idle { state.isHovering = false }
        }
        .onChange(of: state.onboardingStep) { step in
            guard step == .nice else { return }
            withAnimation(.linear(duration: 3.0)) {
                celebrationPhase += .pi * 4
            }
        }
    }

    private var pillScale: CGFloat {
        if isExpanded { return 0.82 }
        return state.isHovering ? 0.58 : 0.5
    }

    private func formatElapsed(_ seconds: TimeInterval) -> String {
        let s = Int(seconds)
        return "\(s / 60):\(String(format: "%02d", s % 60))"
    }

    private var showHoldPromptInPill: Bool {
        guard let step = state.onboardingStep, state.mode == .idle || state.mode == .noSpeech else { return false }
        switch step {
        case .apiTip, .formattingTip, .welcome: return true
        default: return false
        }
    }

    @ViewBuilder
    private var pillContent: some View {
        if showHoldPromptInPill {
            PromptInlineView(step: state.onboardingStep!, hotkeyLabel: state.hotkeyLabel)
                .transition(.opacity)
        } else {
            switch state.mode {
            case .recording, .processing:
                ZStack {
                    BarVisualizer(bandLevels: state.bandLevels, isProcessing: state.mode == .processing)
                        .frame(width: 52, height: 28)
                        .opacity(state.isHandsFree && state.isPaused ? 0 : 1)

                    if state.isHandsFree && state.isPaused {
                        flatBars
                    }

                    Image(systemName: state.isPaused ? "play.fill" : "pause.fill")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundColor(.white.opacity(0.9))
                        .frame(width: 26, height: 26)
                        .background(Circle().fill(Color.white.opacity(0.15)))
                        .contentShape(Circle())
                        .onTapGesture { state.onPauseResume?() }
                        .offset(x: state.isHandsFree ? -49 : 0)
                        .scaleEffect(state.isHandsFree ? 1 : 0.001)
                        .opacity(state.isHandsFree ? 1 : 0)

                    Image(systemName: "stop.fill")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundColor(.white)
                        .frame(width: 26, height: 26)
                        .background(Circle().fill(Color.red.opacity(0.85)))
                        .contentShape(Circle())
                        .onTapGesture { state.onStop?() }
                        .offset(x: state.isHandsFree ? 49 : 0)
                        .scaleEffect(state.isHandsFree ? 1 : 0.001)
                        .opacity(state.isHandsFree ? 1 : 0)
                }
                .frame(width: state.isHandsFree ? 124 : 52, height: 28)
                .transition(.opacity)

            case .noSpeech:
                flatBars.transition(.opacity)

            case .error:
                flatBars.transition(.opacity)

            case .idle:
                if state.isOnboarding {
                    flatBars.transition(.opacity)
                } else if state.isHovering {
                    Image(systemName: "mic.fill")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundColor(.white.opacity(0.9))
                        .frame(width: 28, height: 28)
                        .transition(.opacity)
                } else {
                    EmptyView()
                }
            }
        }
    }

    private var flatBars: some View {
        HStack(spacing: 2) {
            ForEach(0..<11, id: \.self) { _ in
                RoundedRectangle(cornerRadius: 1.5)
                    .fill(Color.white.opacity(0.25))
                    .frame(width: 3, height: 5)
            }
        }
        .frame(width: 52, height: 28)
    }
}

// MARK: - Sub-views

struct LavaLampBackground: View {
    var energy: CGFloat
    var celebrationPhase: Double = 0

    var body: some View {
        TimelineView(.animation) { timeline in
            let t = timeline.date.timeIntervalSinceReferenceDate
            let speed = 0.4 + energy * 0.6
            let brightness = 0.25 + energy * 0.25
            let p = celebrationPhase
            let envelope = CGFloat(max(0, sin(p / 4.0)))
            let r: CGFloat = 150 * envelope
            let ox0 = CGFloat(cos(p + 0)) * r;          let oy0 = CGFloat(sin(p + 0)) * r
            let ox1 = CGFloat(cos(p + .pi * 0.5)) * r;  let oy1 = CGFloat(sin(p + .pi * 0.5)) * r
            let ox2 = CGFloat(cos(p + .pi)) * r;         let oy2 = CGFloat(sin(p + .pi)) * r
            let ox3 = CGFloat(cos(p + .pi * 1.5)) * r;  let oy3 = CGFloat(sin(p + .pi * 1.5)) * r

            ZStack {
                Ellipse().fill(Color.purple.opacity(brightness))
                    .frame(width: 220, height: 105)
                    .offset(x: cos(t * 0.7 * speed) * 80 + ox0, y: sin(t * 0.5 * speed) * 26 + oy0)
                Ellipse().fill(Color.blue.opacity(brightness * 0.9))
                    .frame(width: 260, height: 120)
                    .offset(x: sin(t * 0.6 * speed + 1.5) * 92 + ox1, y: cos(t * 0.45 * speed + 1.0) * 30 + oy1)
                Ellipse().fill(Color.cyan.opacity(brightness * 0.85))
                    .frame(width: 205, height: 94)
                    .offset(x: cos(t * 0.8 * speed + 3.0) * 68 + ox2, y: sin(t * 0.6 * speed + 2.0) * 24 + oy2)
                Ellipse().fill(Color.indigo.opacity(brightness * 0.9))
                    .frame(width: 235, height: 102)
                    .offset(x: sin(t * 0.55 * speed + 4.5) * 86 + ox3, y: cos(t * 0.7 * speed + 3.5) * 26 + oy3)
            }
            .blur(radius: 42)
            .offset(y: 36)
        }
    }
}

struct KeyCapView: View {
    let label: String
    var body: some View {
        Text(label)
            .font(.system(size: 12, weight: .semibold, design: .rounded))
            .foregroundColor(.white)
            .padding(.horizontal, 10).padding(.vertical, 5)
            .background(RoundedRectangle(cornerRadius: 5).fill(Color(white: 0.25)))
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Color(white: 0.45), lineWidth: 1))
            .shadow(color: .black.opacity(0.5), radius: 1, y: 1)
    }
}

struct ErrorCardView: View {
    let message: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 12, weight: .semibold))
                .foregroundColor(.red)
            Text(message)
                .font(.system(size: 15, weight: .medium))
                .foregroundColor(.white.opacity(0.9))
                .lineLimit(1)
        }
        .padding(.horizontal, 16).padding(.vertical, 10)
        .fixedSize()
        .background(
            ZStack {
                RoundedRectangle(cornerRadius: 25).fill(Color.black.opacity(0.75))
                RoundedRectangle(cornerRadius: 25).fill(.thinMaterial)
            }
            .shadow(color: .black.opacity(0.35), radius: 16, y: 4)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 25).strokeBorder(Color.white.opacity(0.3), lineWidth: 1)
        )
    }
}

struct PromptInlineView: View {
    let step: OnboardingStep
    var hotkeyLabel: String = "fn"

    var body: some View {
        HStack(spacing: 6) {
            Text("Hold")
            KeyCapView(label: hotkeyLabel)
            Text(step == .welcome ? "to finish" : "to continue")
        }
        .font(.system(size: 12, weight: .medium))
        .foregroundColor(.white.opacity(0.8))
    }
}

struct PromptCardView: View {
    let step: OnboardingStep
    var hotkeyLabel: String = "fn"

    static let niceMessages = [
        "Nice! 🎉", "Nailed it! ✨", "Sounds good! 👌",
        "Got it! 🙌", "Perfect! 🎯", "Love it! 💫",
    ]

    var body: some View {
        cardContent
            .font(.system(size: 15, weight: .medium))
            .foregroundColor(.white)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 16).padding(.vertical, 10)
            .fixedSize()
            .background(
                ZStack {
                    RoundedRectangle(cornerRadius: 25).fill(Color.black.opacity(0.75))
                    RoundedRectangle(cornerRadius: 25).fill(.thinMaterial)
                }
                .shadow(color: .black.opacity(0.35), radius: 16, y: 4)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 25).strokeBorder(Color.white.opacity(0.3), lineWidth: 1)
            )
    }

    @ViewBuilder
    private var cardContent: some View {
        switch step {
        case .tryIt:
            HStack(spacing: 6) { Text("Hold"); KeyCapView(label: hotkeyLabel); Text("and speak — Yap transcribes it") }
        case .nice:
            Text(PromptCardView.niceMessages.randomElement()!)
        case .doubleTapTip:
            HStack(spacing: 6) { Text("Double-tap"); KeyCapView(label: hotkeyLabel); Text("for hands-free transcription") }
        case .clickTip:
            Text("Click the pill for hands-free transcription")
        case .apiTip:
            Text("Add an API key in the menu bar for better transcription")
        case .formattingTip:
            Text("Enable formatting in Settings to clean up grammar and punctuation automatically")
        case .welcome:
            Text("You're all set — enjoy! 🎉")
        }
    }
}

// MARK: - Bar Visualizer

struct BarVisualizer: View {
    var bandLevels: [Float]
    var isProcessing: Bool
    let barCount = 11

    private let positionScale: [CGFloat] = [0.35, 0.45, 0.6, 0.78, 0.92, 1.0, 0.94, 0.8, 0.63, 0.48, 0.38]

    @State private var appeared = false
    @State private var waveStrength: CGFloat = 0
    @State private var audioDecay: CGFloat = 1
    @State private var waveStart: Date? = nil

    var body: some View {
        TimelineView(.animation(paused: !isProcessing)) { timeline in
            let elapsed = (isProcessing && waveStart != nil) ? timeline.date.timeIntervalSince(waveStart!) : 0.0
            let margin = 5.0
            let sweepRange = Double(barCount - 1) + margin * 2
            let t = elapsed.truncatingRemainder(dividingBy: 1.2) / 1.2
            let waveCenter = -margin + t * sweepRange

            HStack(alignment: .center, spacing: 2) {
                ForEach(0..<barCount, id: \.self) { index in
                    let scale = positionScale[index]
                    let minH: CGFloat = 5
                    let maxH: CGFloat = 28
                    let bandLevel = index < bandLevels.count ? CGFloat(bandLevels[index]) : 0
                    let bandOffset = index < bandLevels.count ? CGFloat(bandLevels[index]) : 0
                    let overall = bandLevels.isEmpty ? bandLevel : CGFloat(bandLevels.reduce(Float(0), +) / Float(bandLevels.count))
                    let blended = (overall * 0.7 + bandOffset * 0.3) * audioDecay
                    let barCeiling = minH + (maxH - minH) * scale
                    let scaled = min(blended / 0.75, 1.0)
                    let driven = pow(scaled, 0.6)
                    let audioH = max(minH, min(barCeiling, minH + (barCeiling - minH) * driven))
                    let distance = abs(Double(index) - waveCenter)
                    let wave = exp(-distance * distance / 6.0)
                    let waveH = 14.0 * CGFloat(wave) * waveStrength
                    let barHeight = min(28.0, max(minH, audioH + waveH))
                    let dimOpacity = 0.35
                    let brightOpacity = 0.95
                    let shimmer = dimOpacity + (brightOpacity - dimOpacity) * Double(wave) * Double(waveStrength)
                    let barOpacity = isProcessing ? shimmer : 0.9

                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(Color.white.opacity(barOpacity))
                        .frame(width: 3, height: barHeight)
                        .animation(.interpolatingSpring(stiffness: 280, damping: 18), value: bandLevel)
                }
            }
        }
        .frame(height: 28)
        .scaleEffect(x: appeared ? 1 : 0.001, y: 1, anchor: .center)
        .opacity(appeared ? 1 : 0)
        .animation(.spring(response: 0.3, dampingFraction: 0.75), value: appeared)
        .onAppear {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { appeared = true }
        }
        .onChange(of: isProcessing) { processing in
            if processing {
                waveStart = Date()
                withAnimation(.easeOut(duration: 0.35)) { audioDecay = 0 }
                withAnimation(.easeIn(duration: 0.35).delay(0.15)) { waveStrength = 1 }
            } else {
                waveStart = nil; audioDecay = 1; waveStrength = 0
            }
        }
    }
}
