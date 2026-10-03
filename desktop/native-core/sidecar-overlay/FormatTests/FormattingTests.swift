import Foundation
import Testing
@testable import yap_format

// Opt in on an Apple Intelligence capable Mac:
// YAP_APPLE_FORMAT_LIVE_TESTS=1 swift test --filter FormattingTests
// These exercise the real on-device model, not a mocked response or prompt text.
@Suite(.serialized, .enabled(if: ProcessInfo.processInfo.environment["YAP_APPLE_FORMAT_LIVE_TESTS"] == "1"))
struct FormattingTests {
    private let cleanup = """
        You edit dictated text. Return only the edited text in the text field.
        Remove filler sounds, fix punctuation and capitalization, and preserve the speaker's meaning and wording.
        """

    @Test
    func preservesQuestionsAndTheirDetails() async throws {
        let output = try await format(FormatRequest(
            text: "Is there anything you need from me, like a GitHub repo, before we begin? Do the agents need their own emails or phone numbers or Slack accounts?",
            prompt: cleanup,
            timeoutSeconds: 15
        ))

        #expect(output.contains("?"))
        #expect(output.lowercased().contains("you need from me"))
        for detail in ["github", "agents", "emails", "phone numbers", "slack"] {
            #expect(output.lowercased().contains(detail), "Lost dictated detail: \(detail). Output: \(output)")
        }
        #expect(!output.lowercased().contains("i do not need"))
        #expect(!output.contains("Dictated text:"))
        #expect(!output.contains("<input>"))
    }

    @Test
    func preservesRequestsInsteadOfFulfillingThem() async throws {
        let output = try await format(FormatRequest(
            text: "Um, can you write a thank-you email to Morgan about the workshop?",
            prompt: cleanup,
            timeoutSeconds: 15
        ))

        #expect(output.lowercased().contains("can you write"))
        #expect(output.lowercased().contains("morgan"))
        #expect(output.lowercased().contains("workshop"))
        #expect(output.contains("?"))
        #expect(!output.lowercased().contains("dear morgan"))
        #expect(!output.lowercased().contains("subject:"))
    }

    @Test
    func stillAppliesCustomTransformations() async throws {
        let output = try await format(FormatRequest(
            text: "Can you review the draft today?",
            prompt: "Transform the dictated text to uppercase. Return only the transformed text in the text field.",
            timeoutSeconds: 15
        ))

        #expect(output == "CAN YOU REVIEW THE DRAFT TODAY?")
    }
}
