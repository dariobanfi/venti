import AuthenticationServices
import SwiftUI
import VentiCore

/// The pilot's account on a Venti server: sign in or create one, see how
/// the logbook and the server stand, sign out, delete.
struct AccountScreen: View {
    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var server

    var body: some View {
        @Bindable var account = account
        Group {
            if let signedIn = account.account {
                SignedInView(signedIn: signedIn)
            } else if let signUp = account.googleSignUp {
                GoogleHandleStep(signUp: signUp)
                    .id(signUp.signup)
            } else {
                SignInForm()
            }
        }
        .navigationTitle("Account")
        .navigationBarTitleDisplayMode(.inline)
        // What Venti says of the account now, and what it offers.
        .task {
            await server.readStatus()
            await account.read()
        }
        .alert(
            "Account",
            isPresented: Binding(
                get: { account.problem != nil },
                set: { isPresented in
                    if !isPresented {
                        account.problem = nil
                    }
                }
            ),
            presenting: account.problem
        ) { _ in
            Button(commonWords().ok, role: .cancel) {}
        } message: { problem in
            Text(problem)
        }
    }
}

/// Signing in, or creating an account.
private struct SignInForm: View {
    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var server

    @Environment(\.webAuthenticationSession) private var webAuthenticationSession

    @State private var isNew = false
    @State private var handle = ""
    @State private var displayName = ""
    @State private var password = ""
    @State private var invite = ""
    /// Whether the handle and password form is open, where Google is the
    /// first way in.
    @State private var withPassword = false

    private let words = googleWords()

    var body: some View {
        VentiList {
            Section {
                Text("With an account, your flights are copied to Venti. They appear on your other devices, and you can share one with a link.")
            }

            if server.server != nil, !server.hasAskedStatus {
                Section {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                        .accessibilityIdentifier("account-status-loading")
                }
            } else if let address = server.server, server.offersGoogle, !withPassword {
                Section {
                    // Google's own button, to Google's guidelines: not the
                    // pack's (see `GoogleButton`).
                    GoogleButton(title: words.continueWithGoogle, isWorking: account.isWorking) {
                        Task { await continueWithGoogle(on: address) }
                    }
                    .disabled(account.isWorking)
                    .accessibilityIdentifier("account-google")
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())

                    Button {
                        withPassword = true
                    } label: {
                        Text(words.withPassword)
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(.quiet))
                    .accessibilityIdentifier("account-with-password")
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                } header: {
                    Text(address.host() ?? address.absoluteString)
                }
                .listRowSeparator(.hidden)
            } else if let address = server.server {
                Section {
                    VentiSegmented(
                        options: [(value: false, name: "Sign in"), (value: true, name: "New account")],
                        selection: $isNew
                    )
                    .listRowSeparator(.hidden)

                    TextField("Handle", text: $handle)
                        .textContentType(.username)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    if isNew {
                        TextField("Name shown to others", text: $displayName)
                            .textContentType(.name)
                    }
                    SecureField("Password", text: $password)
                        .textContentType(isNew ? .newPassword : .password)
                    if isNew {
                        TextField("Invite code", text: $invite)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                    }
                } header: {
                    Text(address.host() ?? address.absoluteString)
                } footer: {
                    if isNew {
                        Text("A handle is 3 to 20 letters, digits or underscores. A password needs at least 15 characters; a few words in a row are easy to remember. A forgotten password cannot be recovered: Venti knows no email address. Handle and name are public once you share a flight.")
                    }
                }

                Section {
                    Button(isNew ? "Create account" : "Sign in") {
                        Task {
                            if isNew {
                                await account.createAccount(
                                    server: address, handle: handle, displayName: displayName,
                                    password: password, invite: invite
                                )
                            } else {
                                await account.signIn(server: address, handle: handle, password: password)
                            }
                            if account.account != nil {
                                password = ""
                            }
                        }
                    }
                    .disabled(account.isWorking || handle.isEmpty || password.isEmpty)
                    .accessibilityIdentifier("account-submit")
                }
            }
        }
    }

    /// Google's page, then the server: signed in, or on to the handle step.
    private func continueWithGoogle(on server: URL) async {
        do {
            let tokens = GoogleTokens(session: webAuthenticationSession, server: server)
            guard let token = try await tokens.idToken() else { return }
            await account.continueWithGoogle(server: server, idToken: token)
        } catch {
            account.problem = AccountModel.words(for: error)
        }
    }
}

/// After a first sign-in with Google: the handle the pilot is known by,
/// the server's suggestion filled in, the name from Google, and an invite
/// code where Venti takes accounts by invite.
private struct GoogleHandleStep: View {
    let signUp: GoogleSignUp

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var server

    @State private var handle: String
    @State private var displayName: String
    @State private var invite = ""

    private let words = googleWords()

    init(signUp: GoogleSignUp) {
        self.signUp = signUp
        _handle = State(initialValue: signUp.suggestedHandle ?? "")
        _displayName = State(initialValue: signUp.name ?? "")
    }

    /// Where the server does not say who may make an account, it may ask
    /// for a code: the field is there then too.
    private var asksForInvite: Bool {
        server.registration == nil || server.registration == .invite
    }

    var body: some View {
        VentiList {
            Section {
                VStack(alignment: .leading, spacing: VentiSpace.s2) {
                    Text(words.chooseHandle)
                        .font(.venti(.title3, .semibold))
                        .accessibilityAddTraits(.isHeader)
                        .accessibilityIdentifier("google-handle-title")
                    Text(words.chooseHandleLine)
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, VentiSpace.s1)
            }

            Section {
                TextField("Handle", text: $handle)
                    .textContentType(.username)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .accessibilityIdentifier("google-handle")
                TextField("Name shown to others", text: $displayName)
                    .textContentType(.name)
                    .accessibilityIdentifier("google-display-name")
                if asksForInvite {
                    TextField("Invite code", text: $invite)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("google-invite")
                }
            } footer: {
                if let line = signUp.suggestionLine {
                    Text(line)
                        .accessibilityIdentifier("google-suggestion-line")
                }
            }

            if let address = server.server {
                Section {
                    Button {
                        Task {
                            await account.createGoogleAccount(
                                server: address, handle: handle, displayName: displayName, invite: invite
                            )
                        }
                    } label: {
                        Group {
                            if account.isWorking {
                                ProgressView()
                            } else {
                                Text(words.createAccount)
                            }
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(.primary))
                    .disabled(account.isWorking || handle.isEmpty)
                    .accessibilityIdentifier("google-create")
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                }
            }
        }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button(commonWords().cancel) {
                    account.leaveGoogleSignUp()
                }
                .disabled(account.isWorking)
                .accessibilityIdentifier("google-cancel")
            }
        }
    }
}

/// The account, and how the logbook and the server stand.
private struct SignedInView: View {
    let signedIn: AccountView

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var server
    @Environment(\.dismiss) private var dismiss
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession

    @State private var isConfirmingSignOut = false
    @State private var isDeleting = false
    @State private var password = ""

    private let words = googleWords()

    var body: some View {
        VentiList {
            // The pilot's face, as everyone sees it beside their flights.
            if account.isSignedIn {
                PictureSection(signedIn: signedIn)
            }
            Section {
                LabeledContent("Handle", value: "@\(signedIn.handle)")
                LabeledContent("Name", value: signedIn.displayName)
                if let email = signedIn.googleEmail {
                    LabeledContent("Google", value: email)
                        .accessibilityIdentifier("account-google-email")
                }
                // Said with the pilot's name on their public flights, once set.
                if account.isSignedIn {
                    NavigationLink {
                        CountryScreen(chosen: signedIn.country)
                    } label: {
                        LabeledContent(AccountWords.country) {
                            Text(signedIn.country.map(Countries.named) ?? AccountWords.noCountry)
                                .accessibilityLabel(signedIn.country.map(Countries.name) ?? AccountWords.noCountry)
                        }
                    }
                    .accessibilityIdentifier("account-country")
                }
            }

            // Google, shown to the pilot and to nobody else.
            if signedIn.googleEmail != nil || server.offersGoogle {
                Section {
                    if signedIn.googleEmail != nil {
                        Button(words.unlinkGoogle) {
                            Task { await account.unlinkGoogle() }
                        }
                        .accessibilityIdentifier("account-unlink-google")
                    } else {
                        Button(words.linkGoogle) {
                            Task {
                                guard let token = await googleToken() else { return }
                                await account.linkGoogle(idToken: token)
                            }
                        }
                        .accessibilityIdentifier("account-link-google")
                    }
                }
                .disabled(account.isWorking || signedIn.needsSignIn)
            }

            Section {
                if signedIn.needsSignIn {
                    Label("This iPhone was signed out. Sign out here and sign in again.", systemImage: "exclamationmark.triangle")
                }
                LabeledContent("Flights on Venti", value: Format.count(signedIn.flightsOnServer))
                if signedIn.flightsToSend > 0 {
                    LabeledContent("Waiting to be sent", value: Format.count(signedIn.flightsToSend))
                }
                if let last = signedIn.lastSyncUnixS {
                    LabeledContent(
                        "In step since",
                        value: "\(Format.dayInYear(unixSeconds: last)), \(Format.time(unixSeconds: last))"
                    )
                }
                // What the last sync did, in the core's words, as on
                // Android and the web.
                if let words = account.lastSyncWords {
                    LabeledContent("Last sync", value: words)
                        .accessibilityIdentifier("account-last-sync")
                }
                Button {
                    Task { await account.sync(force: true, quietly: false) }
                } label: {
                    if account.isWorking {
                        HStack(spacing: 8) {
                            ProgressView()
                            Text("Working")
                        }
                    } else {
                        Text("Sync now")
                    }
                }
                .disabled(account.isWorking || signedIn.needsSignIn)
            } header: {
                Text("Logbook")
            } footer: {
                Text("Venti syncs by itself when you open the app and after a flight.")
            }

            if account.isSignedIn {
                WhoSeesFlights(signedIn: signedIn)
                // What pilots nearby do: new open plans near a place, and
                // being asked for help after a landing. Both off until
                // turned on.
                NearbySection()
            }

            // What other makers' programs may do for the pilot: with a
            // token each, never with the password.
            Section {
                // The pilot's alarms, which Venti checks where the data
                // arrives, and what they fired.
                NavigationLink(alertFormWords().title) {
                    AlertsScreen()
                }
                .disabled(signedIn.needsSignIn)
                .accessibilityIdentifier("alerts-link")
                // What the pilot was told: who follows them, who cheered
                // a flight of theirs or wrote under one.
                NavigationLink(Social.words.notificationsTitle) {
                    NotificationsScreen()
                }
                .disabled(signedIn.needsSignIn)
                .accessibilityIdentifier("account-notifications")
                NavigationLink(tokenForm().title) {
                    ProgramsScreen()
                }
                .disabled(signedIn.needsSignIn)
                .accessibilityIdentifier("programs-link")
            }

            Section {
                Button("Sign out") {
                    isConfirmingSignOut = true
                }
                .accessibilityIdentifier("account-sign-out")
                Button("Delete account", role: .destructive) {
                    isDeleting = true
                }
            }
            .disabled(account.isWorking)
        }
        .confirmationDialog("Sign out?", isPresented: $isConfirmingSignOut, titleVisibility: .visible) {
            Button("Sign out") {
                Task { await account.signOut() }
            }
            .accessibilityIdentifier("account-sign-out-confirm")
        } message: {
            Text("Your flights stay on this iPhone and on Venti.")
        }
        .alert("Delete your account?", isPresented: $isDeleting) {
            SecureField("Password", text: $password)
            Button("Delete account", role: .destructive) {
                let entered = password
                password = ""
                Task { _ = await account.deleteAccount(password: entered) }
            }
            // An account made with Google has no password: its Google
            // account confirms in its place.
            if signedIn.googleEmail != nil {
                Button(words.continueWithGoogle, role: .destructive) {
                    password = ""
                    Task {
                        // The alert goes first: one thing is presented at a time.
                        try? await Task.sleep(for: .milliseconds(600))
                        guard let token = await googleToken() else { return }
                        _ = await account.deleteAccount(googleIDToken: token)
                    }
                }
            }
            Button(commonWords().cancel, role: .cancel) {
                password = ""
            }
        } message: {
            Text(
                signedIn.googleEmail == nil
                    ? "The account and every flight on Venti are deleted at once and for good, and your shared links stop working. The flights on this iPhone stay. Enter your password to go on."
                    : "The account and every flight on Venti are deleted at once and for good, and your shared links stop working. The flights on this iPhone stay. Enter your password, or confirm with Google, to go on."
            )
        }
    }

    /// An ID token of Google's, `nil` when the pilot closed Google's page
    /// or it could not be had (then said as the problem).
    private func googleToken() async -> String? {
        do {
            return try await GoogleTokens(
                session: webAuthenticationSession, server: URL(string: signedIn.server)
            ).idToken()
        } catch {
            account.problem = AccountModel.words(for: error)
            return nil
        }
    }
}

/// What the account's page says that is not the core's to say.
enum AccountWords {
    static let country = "Country"
    static let noCountry = "None"
    static let findCountry = "Find a country"
}

/// Who sees the pilot's flights: the switch for new ones with its line,
/// and the two buttons that change all of them, each asking the core's
/// question first. The answer's line stands under them. All the words are
/// the core's.
private struct WhoSeesFlights: View {
    let signedIn: AccountView

    @Environment(AccountModel.self) private var account

    /// What the pilot asked the switch for, until Venti has answered.
    @State private var switchedTo: Bool?
    /// Which of the two buttons asks its question.
    @State private var asking: FlightVisibility?

    private let words = flightVisibilityWords()

    /// The switch: the account's setting, or what the pilot just asked for.
    private var newFlightsPublic: Binding<Bool> {
        Binding(
            get: { switchedTo ?? signedIn.newFlightsPublic },
            set: { isPublic in
                switchedTo = isPublic
                Task {
                    await account.setNewFlightsPublic(isPublic)
                    switchedTo = nil
                }
            }
        )
    }

    var body: some View {
        Section {
            Toggle(isOn: newFlightsPublic) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(words.newFlightsPublic)
                    Text(words.newFlightsPublicLine)
                        .font(.venti(.footnote))
                        .foregroundStyle(.secondary)
                }
            }
            .accessibilityIdentifier("account-new-flights-public")
            // Being shown near the places one flies from: the second row
            // of who sees, as on Android and the web.
            ShownNearRow(signedIn: signedIn)
            Button(words.makeAllPublic) {
                asking = .public
            }
            .accessibilityIdentifier("account-all-public")
            Button(words.makeAllPrivate) {
                asking = .private
            }
            .accessibilityIdentifier("account-all-private")
        } footer: {
            if let line = account.allFlights?.line {
                Text(line)
                    .accessibilityIdentifier("account-all-flights-line")
            }
        }
        .disabled(account.isWorking || signedIn.needsSignIn)
        .alert(
            asking.map { allFlightsQuestion(flights: signedIn.flightsOnServer, visibility: $0) } ?? "",
            isPresented: Binding(
                get: { asking != nil },
                set: { isPresented in
                    if !isPresented {
                        asking = nil
                    }
                }
            ),
            presenting: asking
        ) { visibility in
            Button(visibility == .public ? words.makeAllPublic : words.makeAllPrivate) {
                Task { await account.setAllFlights(visibility) }
            }
            .accessibilityIdentifier("account-all-confirm")
            Button(commonWords().cancel, role: .cancel) {}
        }
    }
}

/// The pilot's country, chosen from every country `Locale` knows, by its
/// name with its flag, or none.
private struct CountryScreen: View {
    let chosen: String?

    @Environment(AccountModel.self) private var account

    var body: some View {
        CountryChoiceList(
            chosen: chosen, none: AccountWords.noCountry, title: AccountWords.country, prompt: AccountWords.findCountry
        ) { code in
            Task { await account.setCountry(code) }
        }
    }
}

/// Who may see a flight, and the link to it.
struct ShareFlightSheet: View {
    let flight: String

    @Environment(AccountModel.self) private var account
    @Environment(\.dismiss) private var dismiss

    @State private var sharing: FlightSharing?

    var body: some View {
        NavigationStack {
            VentiList {
                if !account.isSignedIn {
                    Section {
                        Text("To share a flight with a link, sign in to Venti under Settings, Account.")
                            .foregroundStyle(.secondary)
                    }
                } else if let sharing {
                    Section {
                        ForEach([FlightVisibility.private, .unlisted, .public], id: \.self) { visibility in
                            Button {
                                Task {
                                    if let now = await account.share(flight, with: visibility) {
                                        self.sharing = now
                                    }
                                }
                            } label: {
                                HStack {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(visibility.title)
                                            .foregroundStyle(.primary)
                                        Text(visibility.explanation)
                                            .font(.venti(.footnote))
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer(minLength: 8)
                                    if sharing.visibility == visibility {
                                        Image(systemName: "checkmark")
                                            .font(.venti(.body, .semibold))
                                            .foregroundStyle(Palette.accent)
                                    }
                                }
                                .contentShape(.rect)
                            }
                            .buttonStyle(.plain)
                            .accessibilityValue(sharing.visibility == visibility ? "Chosen" : "")
                            .accessibilityIdentifier("share-\(visibility.key)")
                        }
                    } header: {
                        Text("Who can see this flight")
                    }
                    .disabled(account.isWorking)

                    if sharing.visibility != .private, let link = URL(string: sharing.link) {
                        // Whether other pilots can write under it, then
                        // the link, as on Android and the web.
                        if sharing.visibility == .public, !sharing.flightId.isEmpty {
                            CommentsSwitchSection(flight: sharing.flightId)
                        }
                        Section("Link") {
                            Text(sharing.link)
                                .font(.venti(.footnote))
                                .textSelection(.enabled)
                            ShareLink("Share the link", item: link)
                        }
                    }
                } else {
                    Section {
                        Text("This flight is not on Venti yet.")
                        Button("Sync now") {
                            Task {
                                await account.sync(force: true, quietly: false)
                                sharing = account.sharing(of: flight)
                            }
                        }
                        .disabled(account.isWorking)
                    }
                }
            }
            .navigationTitle("Share flight")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                }
            }
            .onAppear {
                sharing = account.sharing(of: flight)
            }
        }
    }
}
