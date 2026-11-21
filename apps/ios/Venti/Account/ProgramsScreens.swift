import SwiftUI
import UIKit
import VentiCore

/// "Programs with access", under the account: the tokens the pilot gave to
/// a logger, a script or another maker's app, each with what it may do and
/// the button that withdraws it, and "Give access" for a new one. Every
/// word about what a token is and does is the core's.
struct ProgramsScreen: View {
    @Environment(AccountModel.self) private var account

    /// Made when the page opens and gone when it is left, with whatever
    /// it holds.
    @State private var model: ProgramsModel?

    var body: some View {
        Group {
            if let model {
                ProgramsList(model: model)
            } else {
                VentiList {}
            }
        }
        .navigationTitle(tokenForm().title)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            if model == nil {
                model = ProgramsModel(account: account)
            }
            await model?.load()
        }
    }
}

/// The page itself.
private struct ProgramsList: View {
    let model: ProgramsModel

    @State private var isGiving = false
    /// The access the pilot is asked about before it is withdrawn, and
    /// the core's words for asking.
    @State private var asked: TokenView?
    @State private var question: TokenWithdrawal?
    @State private var isAsking = false

    var body: some View {
        let form = model.form
        VentiList {
            Section {
                Text(form.intro)
            }

            switch model.state {
            case .loading:
                Section {
                    HStack(spacing: VentiSpace.s2) {
                        ProgressView()
                        Text("Loading")
                    }
                    .foregroundStyle(.secondary)
                }
            case .failed(let words):
                Section {
                    Label {
                        Text(words)
                    } icon: {
                        Image("icon-caution")
                    }
                    .accessibilityIdentifier("programs-problem")
                    Button("Try again") {
                        Task { await model.load() }
                    }
                }
            case .loaded(let tokens):
                if let problem = model.problem {
                    Section {
                        Label {
                            Text(problem)
                        } icon: {
                            Image("icon-caution")
                        }
                        .accessibilityIdentifier("programs-problem")
                    }
                }
                if tokens.isEmpty {
                    Section {
                        Text(form.empty)
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("programs-empty")
                    }
                }
                ForEach(tokens, id: \.id) { token in
                    Section {
                        ProgramRows(token: token)
                        Button(tokenWithdrawal(name: token.name).confirm, role: .destructive) {
                            asked = token
                            question = tokenWithdrawal(name: token.name)
                            isAsking = true
                        }
                        .disabled(model.isWorking)
                        .accessibilityIdentifier("program-withdraw")
                    }
                }
            }

            Section {
                Button {
                    isGiving = true
                } label: {
                    Text(form.submit)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.primary))
                .listRowSeparator(.hidden)
                .accessibilityIdentifier("programs-give")
            } footer: {
                Text(form.never)
            }
        }
        .sheet(isPresented: $isGiving) {
            GiveAccessSheet(model: model)
        }
        .alert(question?.title ?? "", isPresented: $isAsking, presenting: question) { question in
            Button(question.confirm, role: .destructive) {
                if let asked {
                    Task { await model.withdraw(asked) }
                }
            }
            Button(question.cancel, role: .cancel) {}
        } message: { question in
            Text(question.message)
        }
    }
}

/// One program with access: its name, what it may do, and its three days.
private struct ProgramRows: View {
    let token: TokenView

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                Text(token.name)
                    .font(.venti(.body, .semibold))
                Text(token.scopesLine)
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            if token.hasEnded {
                VentiStatus(kind: .caution, word: "Ended")
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("program-row")
        LabeledContent("Made", value: token.made)
        LabeledContent("Last used", value: token.lastUsed)
        LabeledContent(token.hasEnded ? "Ended" : "Ends", value: token.ends)
    }
}

/// The form that gives a program access, and after it the page with the
/// new token's secret, shown this once. The sheet cannot be left from that
/// page except through "I have copied it".
private struct GiveAccessSheet: View {
    let model: ProgramsModel

    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var chosen: Set<String> = []
    @State private var ends = false
    @State private var endDay = GiveAccessSheet.firstEndDay
    @State private var password = ""
    @State private var hasTried = false

    /// The first day an access can end on: tomorrow.
    private static var firstEndDay: Date {
        Calendar.current.date(byAdding: .day, value: 1, to: Calendar.current.startOfDay(for: Date())) ?? Date()
    }

    var body: some View {
        NavigationStack {
            Group {
                if let created = model.created {
                    TokenSecretPage(created: created) {
                        model.forgetSecret()
                        dismiss()
                    }
                } else {
                    form
                }
            }
            .navigationTitle(model.form.submit)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if model.created == nil {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") {
                            dismiss()
                        }
                    }
                }
            }
        }
        .interactiveDismissDisabled(model.created != nil)
        // However the sheet goes, the secret and the password go with it.
        .onDisappear {
            password = ""
            model.forgetSecret()
            model.clearRefusal()
        }
    }

    private var form: some View {
        let words = model.form
        return VentiList {
            Section {
                TextField(words.nameHint, text: $name)
                    .autocorrectionDisabled()
                    .accessibilityIdentifier("token-name")
                    .onChange(of: name) {
                        // No longer than the server takes.
                        if name.count > Int(words.nameMaxLength) {
                            name = String(name.prefix(Int(words.nameMaxLength)))
                        }
                    }
                if hasTried, let fault = model.nameFault(name) {
                    Text(fault)
                        .foregroundStyle(.venti(.danger))
                        .accessibilityIdentifier("token-name-fault")
                }
            } header: {
                Text(words.nameLabel)
            }

            Section {
                ForEach(words.scopes, id: \.scope) { scope in
                    let isChosen = chosen.contains(scope.scope)
                    Button {
                        if isChosen {
                            chosen.remove(scope.scope)
                        } else {
                            chosen.insert(scope.scope)
                        }
                    } label: {
                        HStack(spacing: VentiSpace.s3) {
                            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                                Text(scope.title)
                                    .foregroundStyle(.primary)
                                Text(scope.line)
                                    .font(.venti(.footnote))
                                    .foregroundStyle(.secondary)
                            }
                            Spacer(minLength: VentiSpace.s2)
                            Image(systemName: isChosen ? "checkmark.circle.fill" : "circle")
                                .font(.venti(.title3))
                                .foregroundStyle(.venti(isChosen ? .primary : .borderStrong))
                        }
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                    .accessibilityElement(children: .combine)
                    .accessibilityAddTraits(isChosen ? .isSelected : [])
                    .accessibilityValue(isChosen ? "Chosen" : "")
                    .accessibilityIdentifier("token-scope-\(scope.scope)")
                }
            } header: {
                Text(words.scopesTitle)
            }

            Section {
                Toggle(words.endsLabel, isOn: $ends)
                    .accessibilityIdentifier("token-ends")
                if ends {
                    DatePicker(
                        words.endsLabel, selection: $endDay, in: Self.firstEndDay...,
                        displayedComponents: .date
                    )
                    .labelsHidden()
                }
            } footer: {
                Text(words.endsHint)
            }

            Section {
                SecureField(words.passwordLabel, text: $password)
                    .textContentType(.password)
                    .accessibilityIdentifier("token-password")
            } header: {
                Text(words.passwordLabel)
            } footer: {
                Text(words.passwordReason)
            }

            Section {
                if let refusal = model.refusal {
                    Text(refusal)
                        .foregroundStyle(.venti(.danger))
                        .accessibilityIdentifier("token-refusal")
                }
                Button {
                    give()
                } label: {
                    Text(words.submit)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.primary))
                .disabled(model.isWorking)
                .listRowSeparator(.hidden)
                .accessibilityIdentifier("token-submit")
            }
        }
    }

    private func give() {
        hasTried = true
        guard model.nameFault(name) == nil else { return }
        let entered = password
        Task {
            let isMade = await model.give(
                name: name, scopes: chosen, endsUnixS: ends ? ProgramsModel.end(of: endDay) : nil,
                password: entered
            )
            if isMade {
                password = ""
            }
        }
    }
}

/// A new token's secret, this once: the core's words beside it, a button
/// that copies it, and the button that leaves. The secret is drawn and
/// handed to the clipboard when asked; it is written nowhere.
private struct TokenSecretPage: View {
    let created: TokenCreated
    let onDone: () -> Void

    @State private var hasCopied = false

    var body: some View {
        VentiList {
            Section {
                Text(created.shownOnce)
                    .font(.venti(.body, .semibold))
                    .accessibilityIdentifier("token-shown-once")
                Text(created.secret)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("token-secret")
                Button {
                    UIPasteboard.general.string = created.secret
                    hasCopied = true
                } label: {
                    HStack(spacing: VentiSpace.s2) {
                        if hasCopied {
                            Image(systemName: "checkmark")
                        }
                        Text(created.copy)
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.quiet))
                .listRowSeparator(.hidden)
                .accessibilityValue(hasCopied ? "Copied" : "")
                .accessibilityIdentifier("token-copy")
            } header: {
                Text(created.token.name)
            } footer: {
                Text(created.howToUse)
            }

            Section {
                Text(created.token.scopesLine)
                    .foregroundStyle(.secondary)
                Button {
                    onDone()
                } label: {
                    Text(created.done)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.primary))
                .listRowSeparator(.hidden)
                .accessibilityIdentifier("token-done")
            }
        }
        .navigationBarBackButtonHidden()
    }
}
