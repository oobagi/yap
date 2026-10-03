import Foundation
import FoundationModels

struct FormatRequest: Decodable {
    let text: String
    let prompt: String
    let timeoutSeconds: Double?
}

enum HelperError: Error, CustomStringConvertible {
    case message(String)
    case timeout(TimeInterval)

    var description: String {
        switch self {
        case .message(let message):
            return message
        case .timeout(let seconds):
            return "Foundation Models formatter timed out after \(String(format: "%.1f", seconds)) seconds"
        }
    }
}

@Generable
struct FormatOutput {
    @Guide(description: "The final transformed text only.")
    let text: String
}

func log(_ message: String) {
    FileHandle.standardError.write(Data((message + "\n").utf8))
}

func fail(_ message: String, code: Int32 = 1) -> Never {
    log(message)
    exit(code)
}

func withTimeout<T: Sendable>(
    seconds: TimeInterval,
    operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await operation()
        }
        group.addTask {
            let nanoseconds = UInt64(max(seconds, 0.1) * 1_000_000_000)
            try await Task.sleep(nanoseconds: nanoseconds)
            throw HelperError.timeout(seconds)
        }

        guard let result = try await group.next() else {
            throw HelperError.message("Foundation Models formatter task ended without a result")
        }
        group.cancelAll()
        return result
    }
}

func availabilityLabel(_ availability: SystemLanguageModel.Availability) -> String {
    switch availability {
    case .available:
        return "available"
    case .unavailable(let reason):
        switch reason {
        case .deviceNotEligible:
            return "unavailable: deviceNotEligible"
        case .appleIntelligenceNotEnabled:
            return "unavailable: appleIntelligenceNotEnabled"
        case .modelNotReady:
            return "unavailable: modelNotReady"
        @unknown default:
            return "unavailable: unknown"
        }
    @unknown default:
        return "unknown"
    }
}

func ensureAvailable(_ model: SystemLanguageModel) throws {
    let availability = model.availability
    log("availability=\(availabilityLabel(availability))")

    guard case .available = availability else {
        throw HelperError.message("Foundation Models formatter is \(availabilityLabel(availability))")
    }
}

func format(_ request: FormatRequest) async throws -> String {
    let text = request.text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !text.isEmpty else {
        return ""
    }

    let model = SystemLanguageModel.default
    try ensureAvailable(model)

    let session = LanguageModelSession(model: model, instructions: request.prompt)
    let response = try await session.respond(
        to: """
        Transform this transcription according to the formatter instructions.
        Return exactly one final transformed text in the text field.
        Do not include both the original input and a transformed version.
        Do not add headings, labels, examples, placeholder items, or extra sections unless the instructions explicitly ask for them.

        <input>\(request.text)</input>
        """,
        generating: FormatOutput.self,
        options: GenerationOptions(sampling: .greedy, maximumResponseTokens: 2048)
    )

    let output = response.content.text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !output.isEmpty else {
        throw HelperError.message("Foundation Models formatter returned empty text")
    }
    return output
}

func readRequest() throws -> FormatRequest {
    let data = FileHandle.standardInput.readDataToEndOfFile()
    guard !data.isEmpty else {
        throw HelperError.message("missing JSON request on stdin")
    }
    return try JSONDecoder().decode(FormatRequest.self, from: data)
}

@main
struct YapFormat {
    static func main() async {
        do {
            let request = try readRequest()
            let timeoutSeconds = request.timeoutSeconds ?? 15
            let output = try await withTimeout(seconds: timeoutSeconds) {
                try await format(request)
            }
            print(output)
            exit(0)
        } catch let error as HelperError {
            switch error {
            case .timeout:
                fail(error.description, code: 2)
            case .message:
                fail(error.description)
            }
        } catch {
            fail(error.localizedDescription)
        }
    }
}
