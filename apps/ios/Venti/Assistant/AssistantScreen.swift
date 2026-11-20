import SwiftUI
import VentiCore

/// The assistant: questions about the pilot's own flying, answered by a
/// model on the pilot's own account.
struct AssistantScreen: View {
    @Environment(AssistantChat.self) private var chat

    @State private var draft = ""
    @State private var isSettingService = false
    @FocusState private var isTyping: Bool

    var body: some View {
        NavigationStack {
            Group {
                if chat.hasKey {
                    conversation
                } else {
                    KeyEntry()
                }
            }
            .navigationTitle("Assistant")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(isPresented: $isSettingService) {
                OtherServiceForm()
            }
            .toolbar {
                if chat.hasKey {
                    ToolbarItem(placement: .primaryAction) {
                        Menu("Assistant options", systemImage: "ellipsis") {
                            if chat.service == .anthropic {
                                Picker(
                                    "Model",
                                    selection: Binding(
                                        get: { chat.model },
                                        set: { model in chat.use(model: model) }
                                    )
                                ) {
                                    ForEach(assistantModels(), id: \.id) { model in
                                        Text(model.name).tag(model.id)
                                    }
                                }
                            } else {
                                // Which model answers, and where.
                                Section("\(chat.otherModel) at \(Self.host(of: chat.otherAddress))") {
                                    Button("Use Anthropic again", systemImage: "arrow.uturn.backward") {
                                        chat.useAnthropic()
                                    }
                                }
                            }
                            Button("Another service", systemImage: "server.rack") {
                                isSettingService = true
                            }
                            Button("New conversation", systemImage: "square.and.pencil") {
                                chat.startAgain()
                            }
                            Button("Remove my key", systemImage: "trash", role: .destructive) {
                                chat.removeKey()
                            }
                        }
                    }
                }
            }
        }
        .alert(
            "Assistant",
            isPresented: Binding(
                get: { chat.problem != nil },
                set: { isPresented in
                    if !isPresented {
                        chat.problem = nil
                    }
                }
            ),
            presenting: chat.problem
        ) { _ in
            Button("OK", role: .cancel) {}
        } message: { problem in
            Text(problem)
        }
    }

    private var conversation: some View {
        ScrollViewReader { scroll in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    if chat.entries.isEmpty, !chat.isWorking {
                        Suggestions { question in
                            ask(question)
                        }
                    }
                    ForEach(Array(chat.entries.enumerated()), id: \.offset) { entry in
                        Bubble(entry: entry.element)
                    }
                    if let pending = chat.pending {
                        Bubble(entry: TranscriptEntry(fromPilot: true, text: pending, lookedUp: []))
                        ProgressView()
                            .padding(.leading, 4)
                            .accessibilityLabel("The assistant is answering")
                    }
                    Color.clear
                        .frame(height: 1)
                        .id("end")
                }
                .padding(16)
            }
            .scrollDismissesKeyboard(.interactively)
            .onChange(of: chat.entries.count) {
                withAnimation {
                    scroll.scrollTo("end", anchor: .bottom)
                }
            }
            .onChange(of: chat.pending) {
                scroll.scrollTo("end", anchor: .bottom)
            }
        }
        .safeAreaInset(edge: .bottom) {
            HStack(alignment: .bottom, spacing: 8) {
                TextField("Ask about your flying", text: $draft, axis: .vertical)
                    .lineLimit(1...5)
                    .focused($isTyping)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 20))
                Button("Send", systemImage: "arrow.up.circle.fill") {
                    ask(draft)
                }
                .labelStyle(.iconOnly)
                .font(.venti(.largeTitle))
                .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || chat.isWorking)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.bar)
        }
    }

    private func ask(_ question: String) {
        draft = ""
        Task {
            await chat.ask(question)
        }
    }

    /// The name of the computer an address points to: "api.example.org".
    static func host(of address: String) -> String {
        URL(string: address)?.host() ?? address
    }
}

/// What the pilot or the assistant said.
private struct Bubble: View {
    let entry: TranscriptEntry

    var body: some View {
        VStack(alignment: entry.fromPilot ? .trailing : .leading, spacing: 4) {
            Text(Self.formatted(entry.text))
                .textSelection(.enabled)
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .foregroundStyle(entry.fromPilot ? Color.white : Color.venti(.text))
                .background(
                    entry.fromPilot ? Palette.accent : Color(.secondarySystemBackground),
                    in: .rect(cornerRadius: 18)
                )
            if !entry.lookedUp.isEmpty {
                // Where the answer's numbers came from.
                Label("Looked up \(entry.lookedUp.formatted(.list(type: .and)))", systemImage: "magnifyingglass")
                    .font(.venti(.caption))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
            }
        }
        .frame(maxWidth: .infinity, alignment: entry.fromPilot ? .trailing : .leading)
        .padding(entry.fromPilot ? .leading : .trailing, 40)
    }

    /// Models write Markdown; show its bold, italics and links, and leave
    /// the text as it is if it does not parse.
    private static func formatted(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace
        )
        return (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
    }
}

/// Questions to start with.
private struct Suggestions: View {
    @Environment(WindModel.self) private var wind

    let ask: (String) -> Void

    private static let questions = [
        "How was my flying this year?",
        "Which was my best flight, and why?",
        "Where did I find my strongest thermals?",
    ]

    /// Offered as well when a Venti server is set: the assistant can then
    /// look up live wind.
    static let windQuestion = "How is the wind on the Zugspitze right now?"

    private var hasWind: Bool { wind.server != nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Ask about your flying")
                .font(.venti(.title2, .semibold))
            Text(
                hasWind
                    ? "The assistant reads your logbook to answer, and on your Venti server the wind stations, the airspace, the forecast, the flying sites and the flights other pilots share with everyone. It does not say whether a day can be flown, and it cannot see where other pilots are."
                    : "The assistant reads your logbook to answer. With a Venti server set it can also look up wind, airspace, forecasts, flying sites and other pilots' public flights."
            )
            .foregroundStyle(.secondary)
            ForEach(Self.questions + (hasWind ? [Self.windQuestion] : []), id: \.self) { question in
                Button(question) {
                    ask(question)
                }
                .buttonStyle(.venti(.quiet))
                .multilineTextAlignment(.leading)
            }
        }
        .padding(.vertical, 8)
    }
}

/// Where the pilot enters the key for their own account, and reads what
/// that means.
private struct KeyEntry: View {
    @Environment(AssistantChat.self) private var chat
    @State private var key = ""

    var body: some View {
        Form {
            Section {
                Text("The assistant answers questions about your flying from your logbook. It runs on your own account with Anthropic, the maker of Claude: Venti pays for nothing and takes nothing.")
            }
            Section {
                SecureField("API key", text: $key)
                    .textContentType(.password)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                Button("Save key") {
                    chat.saveKey(key)
                    key = ""
                }
                .disabled(key.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            } header: {
                Text("Your key")
            } footer: {
                Text("Kept in this iPhone's Keychain and sent only to Anthropic. Your questions, and what the assistant looks up to answer them, go from this iPhone straight to Anthropic, which bills your account for them. No Venti server sees your key or your questions. When the assistant looks up live wind, the app asks your Venti server for the stations around a place, as the map does.")
            }
            Section {
                if let console = URL(string: "https://console.anthropic.com/settings/keys") {
                    Link("Get a key from Anthropic", destination: console)
                }
            }
            Section {
                NavigationLink("Use another service instead") {
                    OtherServiceForm()
                }
            } footer: {
                Text("OpenAI, OpenRouter and others, or a model you run yourself.")
            }
        }
    }
}

/// Where the pilot sets another service than Anthropic for the assistant:
/// its address, its name for a model, and a key.
private struct OtherServiceForm: View {
    @Environment(AssistantChat.self) private var chat
    @Environment(\.dismiss) private var dismiss

    @State private var address = ""
    @State private var model = ""
    @State private var key = ""
    @State private var problem: String?

    var body: some View {
        Form {
            Section {
                Text("The assistant can run at any service that speaks the chat completions format: OpenAI, OpenRouter and others, or a model you run on a computer of your own. You need the address of its API, its name for the model, and a key for your account there.")
            }
            Section {
                TextField("https://api.example.org/v1", text: $address)
                    .keyboardType(.URL)
                    .textContentType(.URL)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .accessibilityLabel("Address")
                TextField("Model", text: $model)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                SecureField(chat.hasOtherKey ? "API key (one is kept)" : "API key", text: $key)
                    .textContentType(.password)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
            } header: {
                Text("The service")
            } footer: {
                VStack(alignment: .leading, spacing: 8) {
                    if let problem {
                        Label(problem, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.venti(.danger))
                    }
                    Text("Your key and your questions, with what the assistant looks up to answer them, go from this iPhone to that address and nowhere else. The key is kept in the Keychain and sent over an encrypted connection only; without encryption only a computer on your own network is reached. Venti has not been tried with every service: if one answers oddly, tell the project.")
                }
            }
            Section {
                Button("Use this service") {
                    if chat.useOtherService(address: address, model: model, key: key) {
                        dismiss()
                    } else {
                        // Said here, beside the fields, not in an alert.
                        problem = chat.problem
                        chat.problem = nil
                    }
                }
                .disabled(
                    address.trimmingCharacters(in: .whitespaces).isEmpty
                        || model.trimmingCharacters(in: .whitespaces).isEmpty
                )
            }
        }
        .navigationTitle("Another service")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            address = chat.otherAddress
            model = chat.otherModel
        }
    }
}
