import Foundation

// MARK: - Inbound (Tauri → Sidecar, stdin)

struct IPCMessage: Decodable {
    let type: String

    // state
    let state: String?
    let handsFree: Bool?
    let paused: Bool?
    let elapsed: Double?

    // levels
    let level: Float?
    let bars: [Float]?

    // error
    let message: String?

    // permission
    let title: String?
    let actionLabel: String?
    let visible: Bool?

    // onboarding
    let step: String?
    let text: String?
    let hotkeyLabel: String?

    // onboardingPress
    let pressed: Bool?

    // config
    let gradientEnabled: Bool?
    let alwaysVisible: Bool?
}

// MARK: - Outbound (Sidecar → Tauri, stdout)

struct IPCEvent: Encodable {
    let event: String
}

func sendEvent(_ name: String) {
    let event = IPCEvent(event: name)
    guard let data = try? JSONEncoder().encode(event),
          let json = String(data: data, encoding: .utf8) else { return }
    // stdout, newline-delimited
    print(json)
    fflush(stdout)
}
