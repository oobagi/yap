// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "yap-overlay",
    platforms: [.macOS(.v26)],
    products: [
        .executable(name: "yap-overlay", targets: ["yap-overlay"]),
        .executable(name: "yap-speech", targets: ["yap-speech"]),
        .executable(name: "yap-format", targets: ["yap-format"]),
    ],
    targets: [
        .executableTarget(
            name: "yap-overlay",
            path: "Sources",
            swiftSettings: [
                .unsafeFlags(["-suppress-warnings"]),
            ],
            linkerSettings: [
                .linkedFramework("Cocoa"),
            ]
        ),
        .executableTarget(
            name: "yap-speech",
            path: "SpeechSources",
            swiftSettings: [
                .unsafeFlags(["-suppress-warnings"]),
            ],
            linkerSettings: [
                .linkedFramework("Foundation"),
                .linkedFramework("Speech"),
            ]
        ),
        .executableTarget(
            name: "yap-format",
            path: "FormatSources",
            swiftSettings: [
                .unsafeFlags(["-suppress-warnings"]),
            ],
            linkerSettings: [
                .linkedFramework("Foundation"),
                .linkedFramework("FoundationModels"),
            ]
        )
    ]
)
