import SwiftUI
import VentiCore

/// A flight on its way to a league: XContest, where the pilot uploads the
/// file themselves, and the DHV-XC, which takes it from the app.
///
/// What the leagues take, until when, and what is sent to the DHV-XC is
/// the core's to know. This sheet hands the pilot the file, opens
/// XContest's page, and sends the core's request to the DHV-XC from this
/// iPhone. The upload password is kept in the Keychain and goes nowhere
/// else.
struct LeagueSheet: View {
    /// The flight's identifier in the logbook.
    let flight: String

    @Environment(LogbookModel.self) private var logbook
    @Environment(WindModel.self) private var wind
    @Environment(\.dismiss) private var dismiss

    /// What the core says about the flight and the leagues. `nil` until
    /// it is there.
    @State private var handover: LeagueHandover?
    /// The flight's file, written out to be saved or shared.
    @State private var file: URL?
    /// Why there is nothing to hand over.
    @State private var problem: String?

    /// What the pilot signs in to the DHV-XC with, and the glider as it
    /// is set up there: remembered for the next flight.
    @AppStorage("dhvxc-user") private var user = ""
    @AppStorage("dhvxc-glider") private var glider = ""
    @AppStorage("dhvxc-publishes") private var publishes = false
    @State private var password = ""
    @State private var isAsking = false
    @State private var isSending = false
    /// What the DHV-XC answered, or why it could not be asked.
    @State private var answer: LeagueAnswer?

    var body: some View {
        NavigationStack {
            VentiList {
                if let handover {
                    before(handover)
                    xcontest(handover)
                    dhvxc(handover)
                } else if let problem {
                    Text(problem)
                        .foregroundStyle(.secondary)
                } else {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
            .navigationTitle("Send to a league")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                }
            }
            .task {
                await load()
            }
            .confirmationDialog(
                "Send this flight to the DHV-XC?",
                isPresented: $isAsking,
                titleVisibility: .visible
            ) {
                Button("Send") {
                    send()
                }
            } message: {
                Text(asking)
            }
            .alert(
                answer?.title ?? "",
                isPresented: Binding(get: { answer != nil }, set: { if !$0 { answer = nil } }),
                presenting: answer
            ) { _ in
                Button(commonWords().ok, role: .cancel) {}
            } message: { answer in
                Text(answer.words)
            }
        }
    }

    /// What both leagues ask about: the day, and the signature.
    private func before(_ handover: LeagueHandover) -> some View {
        Section("Before it goes") {
            Label {
                Text(handover.deadline)
            } icon: {
                Image(systemName: handover.isLate ? "calendar.badge.exclamationmark" : "calendar")
                    .foregroundStyle(handover.isLate ? Color.venti(.caution) : Palette.accent)
            }
            .accessibilityIdentifier("league-deadline")
            Label {
                Text(handover.signature)
            } icon: {
                Image(systemName: handover.isSigned ? "checkmark.seal" : "seal")
                    .foregroundStyle(handover.isSigned ? Palette.accent : Color.venti(.caution))
            }
            .accessibilityIdentifier("league-signature")
        }
        .font(.venti(.subheadline))
    }

    /// XContest: the file for the pilot, and the page to upload it on.
    private func xcontest(_ handover: LeagueHandover) -> some View {
        Section {
            if let file {
                ShareLink(item: file) {
                    Label("Save the flight file", systemImage: "square.and.arrow.down")
                }
                .accessibilityIdentifier("league-file")
            }
            if let page = URL(string: handover.xcontestClaimUrl) {
                Link(destination: page) {
                    Label("Open XContest’s claim page", systemImage: "safari")
                }
                .accessibilityIdentifier("xcontest-claim")
            }
        } header: {
            Text("XContest")
        } footer: {
            Text("XContest takes a flight on its own page, from you: save the file, sign in there, choose “Claim flight” and upload it. XContest gives other apps no way to do that for you yet.")
        }
    }

    /// The DHV-XC: what it asks for, and the button that sends.
    private func dhvxc(_ handover: LeagueHandover) -> some View {
        Section {
            TextField("User name or e-mail", text: $user)
                .textContentType(.username)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityIdentifier("dhvxc-user")
            SecureField("Upload password", text: $password)
                .accessibilityIdentifier("dhvxc-password")
            TextField("Glider, as set up at the DHV-XC", text: $glider)
                .autocorrectionDisabled()
            Toggle("Publish at once", isOn: $publishes)
            Button {
                isAsking = true
            } label: {
                if isSending {
                    Label("Sending", systemImage: "paperplane")
                } else {
                    Label("Send to the DHV-XC", systemImage: "paperplane")
                }
            }
            .disabled(!canSend)
            .accessibilityIdentifier("dhvxc-send")
        } header: {
            Text("DHV-XC")
        } footer: {
            VStack(alignment: .leading, spacing: 8) {
                Text("The upload password is not the one you sign in with: you set it in your DHV-XC profile. It is kept in this iPhone’s Keychain and sent to the DHV-XC only. Without “Publish at once” the flight waits among your uploads there, for you to look at and publish.")
                if let profile = URL(string: handover.dhvxcProfileUrl) {
                    Link("Your DHV-XC profile", destination: profile)
                }
                if let rules = URL(string: handover.dhvxcRulesUrl) {
                    Link("The DHV-XC’s rules", destination: rules)
                }
            }
            // The links in the size of the words they stand under.
            .font(.venti(.footnote))
        }
    }

    private var canSend: Bool {
        !isSending
            && !user.trimmingCharacters(in: .whitespaces).isEmpty
            && !password.isEmpty
    }

    /// What the pilot is asked before the flight leaves the iPhone.
    private var asking: String {
        let what = publishes
            ? "The DHV-XC publishes it at once if it can."
            : "It waits among your uploads there until you publish it."
        return "As \(user.trimmingCharacters(in: .whitespaces)). \(what) By the DHV-XC’s rules, whoever uploads a flight gives the DHV the right to use its file."
    }

    private func load() async {
        password = LeagueStore.readDhvxcPassword() ?? ""
        do {
            handover = try await logbook.leagueHandover(id: flight, nowUnixS: await wind.serverNowIfRead())
            file = try? await logbook.exportFile(id: flight)
        } catch {
            problem = error.words
        }
    }

    private func send() {
        guard canSend else { return }
        isSending = true
        let (id, user, password, publishes) = (flight, user, password, publishes)
        let glider = glider.trimmingCharacters(in: .whitespaces)
        Task {
            defer { isSending = false }
            do {
                let request = try await logbook.dhvxcRequest(
                    id: id, user: user, uploadPassword: password,
                    glider: glider.isEmpty ? nil : glider, publish: publishes
                )
                let said = try await LeagueUpload.send(request)
                // A password the DHV-XC took is worth keeping; one it
                // refused is not.
                if said.taken {
                    LeagueStore.saveDhvxcPassword(password)
                }
                answer = said
            } catch {
                answer = LeagueAnswer(
                    taken: false,
                    title: "The DHV-XC could not be reached",
                    words: error.localizedDescription
                )
            }
        }
    }
}

/// Sends the core's request to a league from this iPhone.
enum LeagueUpload {
    /// Posts the request and reads the answer with the core. Nothing is
    /// kept: the request holds the pilot's upload password.
    static func send(_ request: LeagueRequest) async throws -> LeagueAnswer {
        guard let url = URL(string: request.url) else {
            throw URLError(.badURL)
        }
        var post = URLRequest(url: url)
        post.httpMethod = "POST"
        post.setValue(request.contentType, forHTTPHeaderField: "Content-Type")
        post.httpBody = request.body
        post.timeoutInterval = 60
        let (status, data, _) = try await AccountModel.send(post)
        return dhvxcUploaded(status: UInt16(clamping: status), body: data)
    }
}
