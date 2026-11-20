import SwiftUI
import VentiCore

/// Whom the pilot shows flights to while they are under way. Kept between
/// flights: the choice is on the Fly tab before every start.
enum LiveChoice: String, CaseIterable, Identifiable {
    case nobody = ""
    case link
    case everyone = "public"

    var id: String { rawValue }

    /// The choice the pilot last made. Nobody, until one was made.
    static var stored: LiveChoice {
        LiveChoice(rawValue: UserDefaults.standard.string(forKey: Preferences.liveAudience) ?? "") ?? .nobody
    }

    /// What the core calls it. Nobody is no audience: nothing is sent.
    var audience: LiveAudience? {
        switch self {
        case .nobody: nil
        case .link: .link
        case .everyone: .public
        }
    }

    var title: String {
        switch self {
        case .nobody: "Nobody"
        case .link: "People I give the link"
        case .everyone: "Everyone"
        }
    }

    /// For the row on the Fly tab.
    var short: String {
        switch self {
        case .nobody: "Off"
        case .link: "With a link"
        case .everyone: "Everyone"
        }
    }
}

/// Live tracking: who may watch a flight while it is under way, and how
/// the flight in the air is being shared. The same page before take-off,
/// from the Fly tab, and in the cockpit.
struct LiveScreen: View {
    @Environment(FlySession.self) private var fly
    @Environment(AccountModel.self) private var account
    @AppStorage(Preferences.liveAudience) private var stored = ""
    @State private var isConfirmingDeletion = false

    private var choice: LiveChoice { LiveChoice(rawValue: stored) ?? .nobody }

    var body: some View {
        VentiList {
            if !account.isSignedIn {
                Section {
                    Label(
                        "Sharing a flight needs an account on Venti. Sign in under Settings.",
                        systemImage: "person.crop.circle.badge.exclamationmark"
                    )
                    .foregroundStyle(.secondary)
                }
            }

            Section {
                Picker(
                    "Who can watch",
                    selection: Binding(
                        get: { choice },
                        set: { chosen in
                            // Taking a flight in the air away from everyone
                            // deletes its track: asked first.
                            if chosen == .nobody, fly.isSharingLive {
                                isConfirmingDeletion = true
                            } else {
                                stored = chosen.rawValue
                                fly.share(with: chosen.audience)
                            }
                        }
                    )
                ) {
                    ForEach(LiveChoice.allCases) { choice in
                        Text(choice.title).tag(choice)
                    }
                }
                .pickerStyle(.inline)
                .labelsHidden()
                .disabled(!account.isSignedIn)
            } header: {
                Text("Who can watch you fly")
            } footer: {
                Text(Self.explanation(of: choice))
            }

            if let live = fly.live {
                Section("This flight") {
                    LabeledContent("Status", value: live.words)
                        .accessibilityIdentifier("live-status")
                    if live.state != .stopped {
                        LabeledContent("Positions sent", value: Format.count(live.taken))
                        if let link = URL(string: live.link) {
                            ShareLink(item: link) {
                                Label("Send the link", systemImage: "square.and.arrow.up")
                            }
                        }
                        Button("Delete the track", systemImage: "trash", role: .destructive) {
                            isConfirmingDeletion = true
                        }
                    }
                }
            }

            Section {
                Text("Your positions go from this iPhone to Venti from take-off on, every quarter of a minute, and nowhere else. Venti removes the track two days after its last position. A track is not a flight in your logbook: that is the recording, which stays on this iPhone until you sync.")
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Live tracking")
        .navigationBarTitleDisplayMode(.inline)
        .confirmationDialog(
            "Delete the track?",
            isPresented: $isConfirmingDeletion,
            titleVisibility: .visible
        ) {
            Button("Delete the track", role: .destructive) {
                stored = LiveChoice.nobody.rawValue
                fly.deleteLiveTrack()
            }
        } message: {
            Text("Nobody can watch this flight any more, with or without the link, and what was sent is removed from the server. Your recording is not touched.")
        }
    }

    static func explanation(of choice: LiveChoice) -> String {
        switch choice {
        case .nobody:
            "Nothing is sent. Nobody sees where you are."
        case .link:
            "Whoever has the link sees where you are and where you flew, with your name: send it to the people who should know. The flight is on nobody's map."
        case .everyone:
            "Everyone who looks at your server's map sees your glider on it, with your name, and can follow the flight."
        }
    }
}

/// The cockpit's way to live tracking: a button whose sign says whether
/// the flight is going out, by its shape and by its colour.
struct LiveButton: View {
    let live: LiveShareView?
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            Label {
                Text("Live tracking")
            } icon: {
                Image(systemName: Self.symbol(for: live?.state))
                    .foregroundStyle(.venti(Self.role(for: live?.state) ?? .text))
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.venti(.quiet))
        .accessibilityValue(live?.words ?? "Off")
        .accessibilityIdentifier("live-button")
    }

    static func symbol(for state: LiveShareState?) -> String {
        switch state {
        case .sharing, .ended: "antenna.radiowaves.left.and.right"
        case .waiting: "wifi.exclamationmark"
        case .stopped, nil: "antenna.radiowaves.left.and.right.slash"
        }
    }

    /// The colour of "good" while positions go out, of caution while they
    /// wait, plain when nothing is shared.
    static func role(for state: LiveShareState?) -> VentiColor.Role? {
        switch state {
        case .sharing: .good
        case .waiting: .caution
        case .ended, .stopped, nil: nil
        }
    }
}
