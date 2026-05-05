import Cocoa

// -- App setup (LSUIElement — no dock icon) --
let app = NSApplication.shared
app.setActivationPolicy(.accessory)

// -- Create overlay --
let panel = OverlayPanel()

// Wire click callbacks → stdout IPC events
panel.overlayState.onClickToRecord = { sendEvent("pill_click") }
panel.overlayState.onPermissionAction = { sendEvent("permission_action") }
panel.overlayState.onPauseResume = { sendEvent("pause") }
panel.overlayState.onStop = { sendEvent("stop") }

// Show the panel (starts hidden or visible based on default alwaysVisible=true)
panel.orderFront(nil)

// -- stdin reader (background thread) --
let decoder = JSONDecoder()

DispatchQueue.global(qos: .userInteractive).async {
    while let line = readLine(strippingNewline: true) {
        guard !line.isEmpty,
              let data = line.data(using: .utf8),
              let msg = try? decoder.decode(IPCMessage.self, from: data) else {
            continue
        }

        DispatchQueue.main.async {
            switch msg.type {
            case "state":
                if let state = msg.state {
                    panel.applyState(
                        state,
                        handsFree: msg.handsFree ?? false,
                        paused: msg.paused ?? false,
                        elapsed: msg.elapsed ?? 0
                    )
                }

            case "levels":
                panel.applyLevels(
                    level: msg.level ?? 0,
                    bars: msg.bars ?? Array(repeating: 0, count: 11)
                )

            case "error":
                if let message = msg.message {
                    panel.applyError(message)
                }

            case "permission":
                panel.applyPermission(
                    title: msg.title,
                    message: msg.message,
                    actionLabel: msg.actionLabel,
                    visible: msg.visible ?? false
                )

            case "onboarding":
                panel.applyOnboarding(
                    step: msg.step,
                    text: msg.text,
                    hotkeyLabel: msg.hotkeyLabel
                )

            case "onboardingPress":
                panel.applyOnboardingPress(msg.pressed ?? false)

            case "config":
                panel.applyConfig(
                    gradientEnabled: msg.gradientEnabled,
                    alwaysVisible: msg.alwaysVisible,
                    hotkeyLabel: msg.hotkeyLabel
                )

            case "celebrating":
                panel.applyCelebrating()

            default:
                break
            }
        }
    }

    // stdin closed = parent terminated, exit cleanly
    DispatchQueue.main.async { NSApp.terminate(nil) }
}

// Signal ready
sendEvent("ready")

// -- Run the app --
app.run()
