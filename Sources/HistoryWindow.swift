import Cocoa
import SwiftUI

// MARK: - SwiftUI History View

private struct HistoryView: View {
    @State private var entries: [HistoryEntry] = HistoryManager.shared.entries

    var body: some View {
        VStack(spacing: 0) {
            if entries.isEmpty {
                Spacer()
                Text("No transcription history")
                    .foregroundStyle(.secondary)
                Spacer()
            } else {
                List(entries) { entry in
                    HistoryRowView(entry: entry)
                        .listRowInsets(EdgeInsets(top: 6, leading: 12, bottom: 6, trailing: 12))
                }
                .listStyle(.plain)
            }

            Divider()

            HStack {
                Spacer()
                Button("Clear History") {
                    HistoryManager.shared.clear()
                    entries = HistoryManager.shared.entries
                }
                .foregroundStyle(.red)
                .disabled(entries.isEmpty)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
        }
        .frame(width: 480, height: 400)
        .onAppear {
            entries = HistoryManager.shared.entries
        }
    }
}

private struct HistoryRowView: View {
    let entry: HistoryEntry
    @State private var copied = false

    private var relativeTime: String {
        let interval = Date().timeIntervalSince(entry.timestamp)
        switch interval {
        case ..<60:
            return "just now"
        case ..<3600:
            let m = Int(interval / 60)
            return "\(m) min ago"
        case ..<86400:
            let h = Int(interval / 3600)
            return "\(h)h ago"
        default:
            let formatter = DateFormatter()
            formatter.dateStyle = .short
            formatter.timeStyle = .short
            return formatter.string(from: entry.timestamp)
        }
    }

    private var providerLabel: String {
        var parts: [String] = [entry.transcriptionProvider]
        if let fmt = entry.formattingProvider, fmt != "none" {
            parts.append(fmt)
        }
        return parts.joined(separator: " + ")
    }

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text(entry.text)
                    .font(.body)
                    .lineLimit(3)
                    .frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: 6) {
                    Text(relativeTime)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text("\u{00B7}")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                    Text(providerLabel)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Button(copied ? "Copied!" : "Copy") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(entry.text, forType: .string)
                copied = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
                    copied = false
                }
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .animation(.easeInOut(duration: 0.15), value: copied)
        }
    }
}

// MARK: - NSWindow wrapper

class HistoryWindow: NSWindow {
    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: 400),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        title = "Transcription History"
        isReleasedWhenClosed = false
        center()
        contentView = NSHostingView(rootView: HistoryView())
    }

    override func makeKeyAndOrderFront(_ sender: Any?) {
        contentView = NSHostingView(rootView: HistoryView())
        super.makeKeyAndOrderFront(sender)
    }
}
