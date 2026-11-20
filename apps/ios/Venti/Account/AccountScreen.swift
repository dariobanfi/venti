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
            } else {
                SignInForm()
            }
        }
        .navigationTitle("Account")
        .navigationBarTitleDisplayMode(.inline)
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
            Button("OK", role: .cancel) {}
        } message: { problem in
            Text(problem)
        }
    }
}

/// Signing in, or creating an account.
private struct SignInForm: View {
    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var server

    @State private var isNew = false
    @State private var handle = ""
    @State private var displayName = ""
    @State private var password = ""
    @State private var invite = ""

    var body: some View {
        Form {
            Section {
                Text("With an account, your flights are copied to Venti. They appear on your other devices, and you can share one with a link. A flight stays private until you share it.")
            }

            if let address = server.server {
                Section {
                    Picker("What to do", selection: $isNew) {
                        Text("Sign in").tag(false)
                        Text("New account").tag(true)
                    }
                    .pickerStyle(.segmented)
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
                        Text("A handle is 3 to 20 letters, digits or underscores. A password needs at least 15 characters; a few words in a row are easy to remember. A forgotten password cannot be recovered: the server knows no email address. Handle and name are public once you share a flight.")
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
            } else {
                Section {
                    Text("Set Venti's address in the settings first.")
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}

/// The account, and how the logbook and the server stand.
private struct SignedInView: View {
    let signedIn: AccountView

    @Environment(AccountModel.self) private var account
    @Environment(\.dismiss) private var dismiss

    @State private var isConfirmingSignOut = false
    @State private var isDeleting = false
    @State private var password = ""

    var body: some View {
        Form {
            Section {
                LabeledContent("Handle", value: "@\(signedIn.handle)")
                LabeledContent("Name", value: signedIn.displayName)
                LabeledContent("Server", value: URL(string: signedIn.server)?.host() ?? signedIn.server)
            }

            Section {
                if signedIn.needsSignIn {
                    Label("This iPhone was signed out. Sign out here and sign in again.", systemImage: "exclamationmark.triangle")
                }
                LabeledContent("Flights on the server", value: Format.count(signedIn.flightsOnServer))
                if signedIn.flightsToSend > 0 {
                    LabeledContent("Waiting to be sent", value: Format.count(signedIn.flightsToSend))
                }
                if let last = signedIn.lastSyncUnixS {
                    LabeledContent(
                        "In step since",
                        value: "\(Format.dayInYear(unixSeconds: last)), \(Format.time(unixSeconds: last))"
                    )
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
                Text("Venti syncs by itself when you open the app and after a flight. Your flights stay private on the server until you share one.")
            }

            Section {
                Button("Sign out") {
                    isConfirmingSignOut = true
                }
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
        } message: {
            Text("Your flights stay on this iPhone and on the server.")
        }
        .alert("Delete your account?", isPresented: $isDeleting) {
            SecureField("Password", text: $password)
            Button("Delete account", role: .destructive) {
                let entered = password
                password = ""
                Task { _ = await account.deleteAccount(password: entered) }
            }
            Button("Cancel", role: .cancel) {
                password = ""
            }
        } message: {
            Text("The account and every flight on the server are deleted at once and for good, and your shared links stop working. The flights on this iPhone stay. Enter your password to go on.")
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
            Form {
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
                        Section("Link") {
                            Text(sharing.link)
                                .font(.venti(.footnote))
                                .textSelection(.enabled)
                            ShareLink("Share the link", item: link)
                        }
                    }
                } else {
                    Section {
                        Text("This flight is not on the server yet.")
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
                    Button("Done") {
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
