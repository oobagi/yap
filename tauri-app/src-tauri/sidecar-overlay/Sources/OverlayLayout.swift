import Cocoa

enum OverlayLayout {
    static let panelWidth: CGFloat = 1400
    static let panelHeight: CGFloat = 700
    static let fallbackScreenFrame = NSRect(x: 0, y: 0, width: 1440, height: 900)
    static let visiblePanelBottomInset: CGFloat = 330

    static let expandedStackYOffset: CGFloat = 30
    static let minimizedStackYOffset: CGFloat = 40
    static let bottomSpacerHeight: CGFloat = 415

    static let controlCenterY: CGFloat = 435
    static let controlScale: CGFloat = 0.82
    static let audioBounceScale: CGFloat = 0.12
    static let controlButtonRadius: CGFloat = 13
    static let controlHitPadding: CGFloat = 4
    static let controlButtonSpacing: CGFloat = 49

    static let activePillHitBaseY: CGFloat = 405
    static let activePillHitWidth: CGFloat = 180
    static let activePillHitHeight: CGFloat = 70
    static let idlePillHitY: CGFloat = 350
    static let idlePillHitWidth: CGFloat = 160
    static let idlePillHitHeight: CGFloat = 70

    static let permissionCardHitY: CGFloat = 425
    static let permissionCardHitWidth: CGFloat = 390
    static let permissionCardHitHeight: CGFloat = 160

    static let hoverTooltipYOffset: CGFloat = -24
    static let hoverTooltipTransitionYOffset: CGFloat = 4
}
