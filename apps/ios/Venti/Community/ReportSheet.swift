import SwiftUI
import VentiCore

/// Something a pilot reports to whoever runs the server.
struct Reported: Identifiable {
    let subject: ReportSubject
    /// The form's title.
    let title: String

    var id: String {
        switch subject {
        case .flight(let id): "flight \(id)"
        case .pilot(let handle): "pilot \(handle)"
        case .liveTrack(let id): "track \(id)"
        }
    }

    static func flight(_ id: String) -> Reported {
        Reported(subject: .flight(id: id), title: "Report this flight")
    }

    static func pilot(_ handle: String) -> Reported {
        Reported(subject: .pilot(handle: handle), title: "Report this pilot")
    }

    static func liveTrack(_ id: String) -> Reported {
        Reported(subject: .liveTrack(id: id), title: "Report this track")
    }
}

/// The form for telling whoever runs the server that a flight, a pilot's
/// page or a live track should not be there.
///
/// A report goes to the operator and nobody else, with the name of the
/// account it comes from: the form says so before anything is sent.
struct ReportSheet: View {
    let reported: Reported

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(\.dismiss) private var dismiss

    @State private var reason: ReportReason?
    @State private var text = ""
    @State private var isSending = false
    @State private var problem: String?
    /// What became of the report, once it is sent.
    @State private var outcome: String?

    private let form = reportForm()

    var body: some View {
        NavigationStack {
            Form {
                if let outcome {
                    Section {
                        Label(outcome, systemImage: "checkmark.circle")
                            .accessibilityIdentifier("report-outcome")
                    }
                } else if let obstacle {
                    Section {
                        Text(obstacle)
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("report-obstacle")
                    }
                } else {
                    Section("What is wrong?") {
                        ForEach(form.reasons, id: \.reason) { choice in
                            Button {
                                reason = choice.reason
                            } label: {
                                HStack(spacing: 12) {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(choice.title)
                                            .foregroundStyle(.primary)
                                        Text(choice.explanation)
                                            .font(.footnote)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer(minLength: 8)
                                    Image(systemName: "checkmark")
                                        .fontWeight(.semibold)
                                        .foregroundStyle(Palette.accent)
                                        .opacity(reason == choice.reason ? 1 : 0)
                                        .accessibilityHidden(true)
                                }
                            }
                            // A list to choose from, not six links: the
                            // names stand in the colour of text, and the
                            // tick alone in the app's.
                            .tint(.primary)
                            .accessibilityAddTraits(reason == choice.reason ? .isSelected : [])
                            .accessibilityIdentifier("report-reason")
                        }
                    }
                    Section {
                        TextField("In your own words", text: $text, axis: .vertical)
                            .lineLimit(3...8)
                            .accessibilityIdentifier("report-text")
                    } header: {
                        Text(reason == .other ? "What is it?" : "Anything to add?")
                    } footer: {
                        VStack(alignment: .leading, spacing: 6) {
                            if text.count > Int(form.textAtMost) {
                                Text(verbatim: "\(text.count) of \(form.textAtMost) characters")
                                    .foregroundStyle(.red)
                            }
                            Text(form.notice)
                        }
                    }
                    if let problem {
                        Section {
                            Text(problem)
                                .foregroundStyle(.red)
                        }
                    }
                }
            }
            .navigationTitle(reported.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(outcome == nil && obstacle == nil ? "Cancel" : "Done") {
                        dismiss()
                    }
                }
                if outcome == nil, obstacle == nil {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Send") {
                            send()
                        }
                        .disabled(reason == nil || isSending)
                    }
                }
            }
        }
    }

    /// Why no report can be sent from here, if none can.
    private var obstacle: String? {
        guard account.isSignedIn, let mine = account.account?.server else {
            return "A report is sent from your account, so that whoever runs the server knows who it comes from. Sign in under Settings, Account."
        }
        guard let shown = wind.server, Self.same(mine, shown.absoluteString) else {
            return "You are signed in on another server than the one this is on. A report goes to the server it is about, from an account there."
        }
        return nil
    }

    /// Whether two addresses name the same server.
    static func same(_ one: String, _ other: String) -> Bool {
        func bare(_ address: String) -> String {
            var address = address.lowercased()
            while address.hasSuffix("/") {
                address.removeLast()
            }
            return address
        }
        return bare(one) == bare(other)
    }

    private func send() {
        guard let reason, !isSending else { return }
        isSending = true
        problem = nil
        Task {
            defer { isSending = false }
            do {
                outcome = try await account.report(reported.subject, reason: reason, text: text)
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}
