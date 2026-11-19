import Foundation
import Observation
import VentiCore

/// One conversation with the assistant, as the screen sees it.
///
/// The conversation itself is the core's: what to send, which tools to run
/// against the logbook, what the reply is. This class does the one thing
/// the core cannot: it sends each request over the network and hands back
/// the answer. Requests go to the pilot's model provider; when the model
/// looks up live wind, the core names an address on the pilot's Venti
/// server and this class fetches it, without the key or the question.
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

    /// The model in use at Anthropic.
    private(set) var model: String

    /// Where the model runs.
    enum Service: String {
        /// Anthropic, with one of its models.
        case anthropic
        /// Another service the pilot set by its address.
        case other
    }

    private(set) var service: Service
    /// The other service's address and its name for the model, as the
    /// pilot last set them. Empty until then.
    private(set) var otherAddress: String
    private(set) var otherModel: String

    @ObservationIgnored private let makeAssistant: (String) -> Assistant
    @ObservationIgnored private let makeAssistantAt: (String, String) throws -> Assistant
    @ObservationIgnored private var assistant: Assistant
    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private let readKey: () -> String?
    @ObservationIgnored private let otherKey: OtherKey
    /// Whether the choice of service is kept for the next start. Tests
    /// do not keep it, and do not start from one that was kept.
    @ObservationIgnored private let remembersService: Bool
    @ObservationIgnored private let fetch: WindModel.Transport
    @ObservationIgnored private let server: () -> String?
    @ObservationIgnored private let now: () -> Int64

    /// Where the key for another service is kept.
    struct OtherKey {
        var read: () -> String?
        var save: (String) -> Bool
        var remove: () -> Void

        /// The iPhone's Keychain.
        static func keychain() -> OtherKey {
            OtherKey(
                read: { KeyStore.readOther() },
                save: { key in KeyStore.saveOther(key) },
                remove: { KeyStore.removeOther() }
            )
        }
    }

    /// - Parameters:
    ///   - makeAssistantAt: a conversation at another service than
    ///     Anthropic, from its address and its name for a model.
    ///   - fetch: how an address on the Venti server is fetched.
    ///   - server: the pilot's Venti server, if one is set. With one, the
    ///     assistant can look up live wind.
    ///   - now: the current time, by the server's clock where it is known.
    init(
        makeAssistant: @escaping (String) -> Assistant,
        makeAssistantAt: @escaping (String, String) throws -> Assistant = { _, _ in throw URLError(.unsupportedURL) },
        transport: @escaping Transport = AssistantChat.send,
        readKey: @escaping () -> String? = KeyStore.read,
        otherKey: OtherKey = .keychain(),
        remembersService: Bool = true,
        fetch: @escaping WindModel.Transport = WindModel.fetch,
        server: @escaping () -> String? = { nil },
        now: @escaping () -> Int64 = { Int64(Date().timeIntervalSince1970) }
    ) {
        let defaults = UserDefaults.standard
        let model = defaults.string(forKey: Preferences.assistantModel)
            ?? assistantModels().first?.id
            ?? ""
        let address = remembersService ? defaults.string(forKey: Preferences.assistantAddress) ?? "" : ""
        let named = remembersService ? defaults.string(forKey: Preferences.assistantOtherModel) ?? "" : ""
        // The other service is used only if it is still set up whole.
        var service = Service.anthropic
        var assistant = makeAssistant(model)
        if remembersService, defaults.string(forKey: Preferences.assistantService) == Service.other.rawValue,
           let other = try? makeAssistantAt(address, named)
        {
            service = .other
            assistant = other
        }
        self.model = model
        self.service = service
        otherAddress = address
        otherModel = named
        self.makeAssistant = makeAssistant
        self.makeAssistantAt = makeAssistantAt
        self.assistant = assistant
        self.transport = transport
        self.readKey = readKey
        self.otherKey = otherKey
        self.remembersService = remembersService
        self.fetch = fetch
        self.server = server
        self.now = now
        hasKey = (service == .anthropic ? readKey() : otherKey.read()) != nil
    }

    /// The key for the service in use.
    private var key: String? {
        service == .anthropic ? readKey() : otherKey.read()
    }

    /// Whether a key for the other service is kept.
    var hasOtherKey: Bool {
        otherKey.read() != nil
    }

    var isWorking: Bool { pending != nil }

    /// Asks a question and waits for the answer.
    func ask(_ question: String) async {
        let question = question.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !question.isEmpty, pending == nil, let key = self.key else { return }
        pending = question
        defer { pending = nil }

        assistant.useServer(address: server())
        var step = assistant.ask(question: question, nowUnixS: now(), key: key)
        working: while true {
            switch step {
            case .send(let request):
                do {
                    let (status, body) = try await transport(request)
                    step = assistant.receive(status: UInt16(clamping: status), body: body, key: key)
                } catch {
                    assistant.abandon()
                    problem = "The provider could not be reached. \(error.localizedDescription)"
                    return
                }
            case .fetch(let url):
                // A Venti server that cannot be reached does not end the
                // conversation: the model is told, and says so in its reply.
                var answer: (Int, Data) = (0, Data())
                if let address = URL(string: url), let fetched = try? await fetch(address) {
                    answer = fetched
                }
                step = assistant.receive(status: UInt16(clamping: answer.0), body: answer.1, key: key)
            case .failed(let message):
                problem = message
                break working
            case .finished:
                break working
            }
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

    /// Removes the key of the service in use. Without its key another
    /// service is no longer used.
    func removeKey() {
        if service == .other {
            otherKey.remove()
            useAnthropic()
        } else {
            KeyStore.remove()
            hasKey = false
            startAgain()
        }
    }

    /// Runs the assistant at another service than Anthropic: any that
    /// speaks the chat completions format. The core says whether the
    /// address is one a key may be sent to. A key typed now replaces the
    /// one kept for that service; with none typed, the kept one stays.
    /// Returns whether it is set up; if not, `problem` says why.
    @discardableResult
    func useOtherService(address: String, model: String, key: String) -> Bool {
        let key = key.trimmingCharacters(in: .whitespacesAndNewlines)
        let model = model.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            let address = try assistantServiceAddress(text: address)
            let made = try makeAssistantAt(address, model)
            if !key.isEmpty, !otherKey.save(key) {
                problem = "The key could not be stored in the Keychain."
                return false
            }
            guard otherKey.read() != nil else {
                problem = "The service needs a key. A model you run yourself may take any text as one."
                return false
            }
            if remembersService {
                let defaults = UserDefaults.standard
                defaults.set(Service.other.rawValue, forKey: Preferences.assistantService)
                defaults.set(address, forKey: Preferences.assistantAddress)
                defaults.set(model, forKey: Preferences.assistantOtherModel)
            }
            service = .other
            otherAddress = address
            otherModel = model
            assistant = made
            entries = []
            hasKey = true
            return true
        } catch {
            problem = error.words
            return false
        }
    }

    /// Goes back to Anthropic. The other service's address and key stay
    /// kept, for changing back.
    func useAnthropic() {
        service = .anthropic
        if remembersService {
            UserDefaults.standard.set(Service.anthropic.rawValue, forKey: Preferences.assistantService)
        }
        hasKey = readKey() != nil
        startAgain()
    }

    /// Forgets the conversation.
    func startAgain() {
        if service == .other, let other = try? makeAssistantAt(otherAddress, otherModel) {
            assistant = other
        } else {
            assistant = makeAssistant(model)
        }
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
