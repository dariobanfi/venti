import Foundation
import Observation
import VentiCore

/// One conversation with the assistant, as the screen sees it.
///
/// The conversation itself is the core's: what to send, which tools to run
/// against the logbook, what the reply is. This class does the one thing
/// the core cannot: it sends each request over the network and hands back
/// the answer.
@MainActor
@Observable
final class AssistantChat {
    /// Sends a request and returns the HTTP status and the body.
    typealias Transport = @Sendable (AssistantRequest) async throws -> (Int, Data)

    private(set) var entries: [TranscriptEntry] = []
    /// The question being answered right now, shown until the reply is in.
    private(set) var pending: String?
    var problem: String?
    private(set) var hasKey: Bool

    /// The model in use.
    private(set) var model: String

    @ObservationIgnored private let makeAssistant: (String) -> Assistant
    @ObservationIgnored private var assistant: Assistant
    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private let readKey: () -> String?

    init(
        makeAssistant: @escaping (String) -> Assistant,
        transport: @escaping Transport = AssistantChat.send,
        readKey: @escaping () -> String? = KeyStore.read
    ) {
        let model = UserDefaults.standard.string(forKey: Preferences.assistantModel)
            ?? assistantModels().first?.id
            ?? ""
        self.model = model
        self.makeAssistant = makeAssistant
        assistant = makeAssistant(model)
        self.transport = transport
        self.readKey = readKey
        hasKey = readKey() != nil
    }

    var isWorking: Bool { pending != nil }

    /// Asks a question and waits for the answer.
    func ask(_ question: String) async {
        let question = question.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !question.isEmpty, pending == nil, let key = readKey() else { return }
        pending = question
        defer { pending = nil }

        var step = assistant.ask(question: question, nowUnixS: Int64(Date().timeIntervalSince1970), key: key)
        while case .send(let request) = step {
            do {
                let (status, body) = try await transport(request)
                step = assistant.receive(status: UInt16(clamping: status), body: body, key: key)
            } catch {
                assistant.abandon()
                problem = "The provider could not be reached. \(error.localizedDescription)"
                return
            }
        }
        if case .failed(let message) = step {
            problem = message
        }
        entries = assistant.transcript()
    }

    /// Changes the model, which starts a new conversation.
    func use(model: String) {
        guard model != self.model else { return }
        self.model = model
        UserDefaults.standard.set(model, forKey: Preferences.assistantModel)
        startAgain()
    }

    func saveKey(_ key: String) {
        let key = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { return }
        if KeyStore.save(key) {
            hasKey = true
        } else {
            problem = "The key could not be stored in the Keychain."
        }
    }

    func removeKey() {
        KeyStore.remove()
        hasKey = false
        startAgain()
    }

    /// Forgets the conversation.
    func startAgain() {
        assistant = makeAssistant(model)
        entries = []
    }

    /// Sends a request with the system's own networking.
    nonisolated static func send(_ request: AssistantRequest) async throws -> (Int, Data) {
        guard let url = URL(string: request.url) else {
            throw URLError(.badURL)
        }
        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = "POST"
        urlRequest.timeoutInterval = 120
        for header in request.headers {
            urlRequest.setValue(header.value, forHTTPHeaderField: header.name)
        }
        urlRequest.httpBody = Data(request.body.utf8)
        // No caching and no cookies: nothing about a conversation is kept
        // by the app outside the conversation itself.
        let (data, response) = try await URLSession(configuration: .ephemeral).data(for: urlRequest)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, data)
    }
}
