import Foundation
import Observation
import UIKit
import VentiCore

/// The pilot's account on a Venti server, as the screens see it.
///
/// What to ask the server and what its answers mean is the core's. This
/// class sends the requests, keeps the session token in the Keychain and
/// runs the sync that keeps the logbook and the server in step.
@MainActor
@Observable
final class AccountModel {
    /// Sends a request and returns the HTTP status, the body and the
    /// answer's `Retry-After` in seconds, if it had one.
    typealias Transport = @Sendable (URLRequest) async throws -> (Int, Data, UInt32?)

    /// The account this iPhone is signed in to.
    private(set) var account: AccountView?
    /// Whether a request or a sync is under way.
    private(set) var isWorking = false
    /// What the last sync did, for the account screen.
    private(set) var lastSync: SyncSummary?
    /// Something the pilot must be told.
    var problem: String?

    @ObservationIgnored private let core: Core
    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private let readToken: () -> String?
    @ObservationIgnored private let saveToken: (String?) -> Void
    /// Called after a sync changed the logbook, so its screens can catch up.
    @ObservationIgnored var onLogbookChanged: () -> Void = {}
    /// Called after the pilot's own sites were brought in step with the
    /// account, and when the account came or went: where the list is kept
    /// has changed, and with `true` the sites on this iPhone have too.
    @ObservationIgnored var onKeptSitesSynced: (_ changedHere: Bool) -> Void = { _ in }
    /// Called after signing in and out: the alarms follow the account.
    @ObservationIgnored var onAccountChanged: () -> Void = {}
    @ObservationIgnored private var lastAutomaticSync: Date?
    /// Set when a sync was asked for while one was under way: the logbook
    /// changed again, and another run follows the present one.
    @ObservationIgnored private var isSyncDue = false
    /// Set when a site was kept or forgotten while a request was under
    /// way: the pilot's own sites are brought in step once more.
    @ObservationIgnored private var areKeptSitesDue = false

    init(
        core: Core,
        transport: @escaping Transport = AccountModel.send,
        readToken: @escaping () -> String? = SessionStore.read,
        saveToken: @escaping (String?) -> Void = AccountModel.keep
    ) {
        self.core = core
        self.transport = transport
        self.readToken = readToken
        self.saveToken = saveToken
        account = try? core.account()
    }

    /// Whether the pilot is signed in and the session still holds.
    var isSignedIn: Bool {
        account.map { !$0.needsSignIn } ?? false
    }

    /// What deleting a flight means, said before the pilot confirms it.
    /// With an account the next sync deletes the flight on the server and
    /// from there on the pilot's other devices; saying "from this iPhone"
    /// alone would be a half-truth.
    var deletionWarning: String {
        account == nil
            ? "Its file is removed from this iPhone. This cannot be undone."
            : "Its file is removed from this iPhone, from Venti and from your other devices. This cannot be undone."
    }

    // MARK: Signing in and out

    /// Signs in on the given server.
    func signIn(server: URL, handle: String, password: String) async {
        await enter(server: server) {
            try core.signInRequest(handle: handle, password: password, device: UIDevice.current.name)
        }
    }

    /// Creates an account on the given server and signs in.
    func createAccount(
        server: URL, handle: String, displayName: String, password: String, invite: String
    ) async {
        await enter(server: server) {
            try core.createAccountRequest(
                handle: handle, displayName: displayName, password: password,
                invite: invite, device: UIDevice.current.name
            )
        }
    }

    private func enter(server: URL, request: () throws -> ServerRequest) async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        let signedIn: SignedIn
        do {
            let (status, body, _) = try await ask(try request(), of: server.absoluteString)
            signedIn = try core.signedIn(server: server.absoluteString, status: status, body: body)
        } catch {
            problem = Self.words(for: error)
            return
        }
        await keep(signedIn, on: server)
    }

    // MARK: Google

    /// The handle step after a first sign-in with Google: what the core
    /// read from the server's answer, until the account is made or the
    /// pilot goes back.
    private(set) var googleSignUp: GoogleSignUp?

    /// Signs in with an ID token Google gave for this iPhone. A Google
    /// account no Venti account is linked to yet goes on to the handle
    /// step (`googleSignUp`). An empty token is the core's to refuse, in
    /// its words.
    func continueWithGoogle(server: URL, idToken: String) async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        let answer: GoogleSignIn
        do {
            let request = try core.googleSignInRequest(idToken: idToken, device: UIDevice.current.name)
            let (status, body, _) = try await ask(request, of: server.absoluteString)
            answer = try core.googleSignIn(server: server.absoluteString, status: status, body: body)
        } catch {
            problem = Self.words(for: error)
            return
        }
        if let signedIn = answer.signedIn {
            googleSignUp = nil
            await keep(signedIn, on: server)
        } else {
            googleSignUp = answer.needsHandle
        }
    }

    /// Makes the account of the handle step, with the handle the pilot
    /// chose, and signs in.
    func createGoogleAccount(server: URL, handle: String, displayName: String, invite: String) async {
        guard let signUp = googleSignUp else { return }
        let invite = invite.trimmingCharacters(in: .whitespacesAndNewlines)
        await enter(server: server) {
            try core.googleAccountRequest(
                signup: signUp.signup, handle: handle, displayName: displayName,
                invite: invite.isEmpty ? nil : invite, device: UIDevice.current.name
            )
        }
        if account != nil {
            googleSignUp = nil
        }
    }

    /// Back from the handle step to the account screen's first buttons.
    func leaveGoogleSignUp() {
        googleSignUp = nil
    }

    /// Links the account this iPhone is signed in to to the Google
    /// account of an ID token.
    func linkGoogle(idToken: String) async {
        await changeGoogle {
            let (status, body, _) = try await ask(try core.linkGoogleRequest(idToken: idToken), of: $0)
            return try core.googleLinked(status: status, body: body)
        }
    }

    /// Unlinks the account from its Google account; the server refuses it
    /// for an account Google is the only way into, in the core's words.
    func unlinkGoogle() async {
        await changeGoogle {
            let (status, body, _) = try await ask(core.unlinkGoogleRequest(), of: $0)
            return try core.googleUnlinked(status: status, body: body)
        }
    }

    private func changeGoogle(_ change: (String) async throws -> AccountView?) async {
        guard !isWorking, let server = account?.server else { return }
        isWorking = true
        defer { isWorking = false }
        do {
            if let now = try await change(server) {
                account = now
            }
        } catch {
            problem = Self.words(for: error)
        }
    }

    /// Keeps a sign-in: the token in the Keychain and the account, then
    /// the first sync.
    private func keep(_ signedIn: SignedIn, on server: URL) async {
        do {
            saveToken(signedIn.token)
            // Without the token every later request would be refused. If
            // the Keychain did not keep it, say that, and do not leave a
            // session on the server that this iPhone cannot use.
            guard readToken() == signedIn.token else {
                if let request = try? core.signOutRequest(),
                   let url = URL(string: server.absoluteString + request.path)
                {
                    var signOut = URLRequest(url: url)
                    signOut.httpMethod = request.method
                    signOut.setValue("Bearer \(signedIn.token)", forHTTPHeaderField: "Authorization")
                    _ = try? await transport(signOut)
                }
                try? core.signedOut()
                problem = "This iPhone could not keep the session in its Keychain, so you are not signed in. Restart the iPhone and try again."
                return
            }
            account = signedIn.account
        }
        await runSync()
        onAccountChanged()
    }

    /// Signs this iPhone out. The flights in the logbook stay.
    func signOut() async {
        guard !isWorking else { return }
        isWorking = true
        defer { isWorking = false }
        // The server is told if it can be reached; the iPhone forgets the
        // session either way.
        if let account, let request = try? core.signOutRequest() {
            _ = try? await ask(request, of: account.server)
        }
        try? core.signedOut()
        saveToken(nil)
        account = nil
        lastSync = nil
        onKeptSitesSynced(false)
        onAccountChanged()
    }

    /// Deletes the account on the server, with its flights there. The
    /// logbook on this iPhone stays. Returns whether it is gone.
    func deleteAccount(password: String) async -> Bool {
        await deleteAccount { try core.deleteAccountRequest(password: password) }
    }

    /// Deletes an account that signs in with Google, confirmed with a
    /// fresh ID token of its Google account in place of the password.
    func deleteAccount(googleIDToken: String) async -> Bool {
        await deleteAccount { try core.deleteAccountWithGoogleRequest(idToken: googleIDToken) }
    }

    private func deleteAccount(_ asking: () throws -> ServerRequest) async -> Bool {
        guard !isWorking, let account else { return false }
        isWorking = true
        defer { isWorking = false }
        do {
            let request = try asking()
            let (status, body, _) = try await ask(request, of: account.server)
            try core.accountDeleted(status: status, body: body)
            saveToken(nil)
            self.account = nil
            lastSync = nil
            onKeptSitesSynced(false)
            onAccountChanged()
            return true
        } catch {
            problem = Self.words(for: error)
            return false
        }
    }

    // MARK: What Venti says of the account

    /// Asks Venti what it says of the account: its name, and whether it
    /// is a partner of Venti for flying sites or stopped from publishing,
    /// which the core keeps and reads what a site offers from. Asked when
    /// the app comes to the front and when the account's page opens.
    /// Quiet: without an answer the account is as it was last said. A
    /// session that ended is no error: the account then says so.
    func read() async {
        guard let server = account?.server, let request = try? core.accountRequest(),
              let (status, body, _) = try? await ask(request, of: server, timeout: 20),
              let read = try? core.accountRead(status: status, body: body)
        else { return }
        if read.needsSignIn {
            saveToken(nil)
        }
        account = read
    }

    // MARK: Flying sites

    /// What may be offered for a site to whoever is signed in here, as
    /// the core's one answer. `nil` only where the core cannot read the
    /// account it stores: nothing is offered then.
    func offers(
        for site: SiteView, takesSitesFromPilots: Bool, confirmations: SiteConfirmations?
    ) -> SiteOffers? {
        // The answer follows the account: a view that reads it is drawn
        // again when the account changes.
        _ = account
        return try? core.siteOffers(
            site: site, takesSitesFromPilots: takesSitesFromPilots, confirmations: confirmations
        )
    }

    /// What may be offered in a site's history to whoever is signed in
    /// here, as the core's one answer.
    func historyOffers(_ history: SiteHistory, takesSitesFromPilots: Bool) -> SiteHistoryOffers? {
        _ = account
        return try? core.siteHistoryOffers(history: history, takesSitesFromPilots: takesSitesFromPilots)
    }

    /// Whether a site may be added from here, into which tier, and the
    /// words of the entry and its form.
    func adding(takesSitesFromPilots: Bool) -> SiteAdding? {
        _ = account
        return try? core.siteAdding(takesSitesFromPilots: takesSitesFromPilots)
    }

    /// The places of an answer where pilots take off and no site stands
    /// that this pilot took off at, with the core's offer to add each
    /// (`candidateForPilot`): the core decides who is offered what. The
    /// iPhone shows a partner none, as it shows a partner no entry for
    /// adding. Worked out away from the screen: the core reads the pilot's
    /// flights from there.
    func candidateOffers(_ candidates: SiteCandidates, takesSitesFromPilots: Bool) async -> [CandidateOffer] {
        guard let adding = adding(takesSitesFromPilots: takesSitesFromPilots), !adding.asPartner,
              !candidates.candidates.isEmpty
        else { return [] }
        let core = core
        let offsetS = Int32(clamping: TimeZone.current.secondsFromGMT())
        return await Task.detached(priority: .userInitiated) {
            (try? core.candidateForPilot(
                candidates: candidates, takesSitesFromPilots: takesSitesFromPilots, utcOffsetS: offsetS
            )) ?? []
        }.value
    }

    /// "Not this one": the core keeps the place on this iPhone and offers
    /// it, or one near it, no more.
    func setAside(_ candidate: SiteCandidateView) {
        try? core.candidateSetAside(position: candidate.position)
    }

    /// The pilot's own flights of a day that began at a site, to name
    /// with a confirmation. Mostly none.
    func flights(toConfirm site: SiteView, on day: String, utcOffsetS: Int32) -> [SiteConfirmFlight] {
        (try? core.siteConfirmFlights(site: site, day: day, utcOffsetS: utcOffsetS)) ?? []
    }

    /// The request that says of a site "I took off here and it was good".
    func confirmRequest(
        for id: String, day: String, today: String, note: String, flight: String?
    ) throws -> ServerRequest {
        try core.siteConfirmRequest(id: id, day: day, today: today, note: note, flight: flight)
    }

    // MARK: Sync

    /// Brings the logbook and the server in step, if the pilot is signed
    /// in. Called when the app comes to the front and after the logbook
    /// changed. `force` skips the pause between runs. A run the pilot did
    /// not ask for is quiet: being offline after a flight is no news.
    func sync(force: Bool = false, quietly: Bool = true) async {
        guard isSignedIn else { return }
        guard !isWorking else {
            // An import of many flights changes the logbook again and
            // again: what the present run does not take, the next one does.
            if force {
                isSyncDue = true
            }
            return
        }
        if !force, let last = lastAutomaticSync, Date().timeIntervalSince(last) < 60 {
            return
        }
        isWorking = true
        defer { isWorking = false }
        repeat {
            isSyncDue = false
            await runSync(reportsProblems: !quietly)
        } while (isSyncDue || areKeptSitesDue) && isSignedIn
    }

    /// Brings the pilot's own sites in step with the account, if the
    /// pilot is signed in: called after a site was kept or forgotten.
    /// Quiet, as every run the pilot did not ask for: the list on the
    /// iPhone holds, and its note says that the account has yet to learn.
    func syncKeptSites() async {
        guard isSignedIn else { return }
        guard !isWorking else {
            areKeptSitesDue = true
            return
        }
        isWorking = true
        defer { isWorking = false }
        repeat {
            await runKeptSitesSync(reportsProblems: false)
        } while areKeptSitesDue && isSignedIn
    }

    /// Ties the pilot's own sites to the account: a site kept or forgotten
    /// while signed in goes to the account at once, and after every run,
    /// sign-in and sign-out the list is read again, with its note.
    func keepsInStep(_ own: KeptSitesModel) {
        onKeptSitesSynced = { [weak own] changedHere in
            Task { await own?.followedTheAccount(changedHere: changedHere) }
        }
        own.onChanged = { [weak self] in
            Task { await self?.syncKeptSites() }
        }
    }

    /// The logbook's run, and beside it the run of the pilot's own sites.
    private func runSync(reportsProblems: Bool = true) async {
        guard let server = account?.server else { return }
        lastAutomaticSync = Date()
        let run = core.sync()
        let summary = await finish(
            from: run.start(nowUnixS: Int64(Date().timeIntervalSince1970)),
            on: server, reportsProblems: reportsProblems
        ) { status, body, retryAfter in
            run.receive(status: status, body: body, retryAfterS: retryAfter)
        }
        if let summary {
            lastSync = summary
        }
        account = try? core.account()
        onLogbookChanged()
        await runKeptSitesSync(reportsProblems: reportsProblems)
    }

    /// One run of the core's sync of the pilot's own sites, to its end.
    private func runKeptSitesSync(reportsProblems: Bool) async {
        areKeptSitesDue = false
        // A session the logbook's run just found ended sends nothing more.
        guard isSignedIn, readToken() != nil, let server = account?.server else { return }
        let run = core.keptSitesSync()
        let summary = await finish(
            from: run.start(nowUnixS: Int64(Date().timeIntervalSince1970)),
            on: server, reportsProblems: reportsProblems
        ) { status, body, retryAfter in
            run.receive(status: status, body: body, retryAfterS: retryAfter)
        }
        account = try? core.account()
        onKeptSitesSynced(summary.map { $0.fetched > 0 || $0.removedHere > 0 } ?? false)
    }

    /// Runs a sync of the core's to its end: sends each request it hands
    /// out with the session and hands the answer back. Returns what it
    /// did, or `nil` when it stopped before its end.
    private func finish(
        from first: SyncStep, on server: String, reportsProblems: Bool,
        receive: (UInt16, Data, UInt32?) -> SyncStep
    ) async -> SyncSummary? {
        var step = first
        while case .send(let request) = step {
            do {
                let (status, body, retryAfter) = try await ask(request, of: server)
                step = receive(status, body, retryAfter)
            } catch {
                // What was done so far is kept; the next sync goes on.
                if reportsProblems {
                    problem = Self.words(for: error)
                }
                break
            }
        }
        switch step {
        case .finished(let summary):
            return summary
        case .failed(let message, let needsSignIn):
            if needsSignIn {
                saveToken(nil)
            }
            if reportsProblems || needsSignIn {
                problem = message
            }
            return nil
        case .send:
            return nil
        }
    }

    // MARK: Sharing

    /// How a flight is shared, if the server has it.
    func sharing(of flight: String) -> FlightSharing? {
        try? core.flightSharing(id: flight)
    }

    /// Changes who may see a flight, and returns how it is shared now.
    func share(_ flight: String, with visibility: FlightVisibility) async -> FlightSharing? {
        guard let account, !isWorking else { return nil }
        isWorking = true
        defer { isWorking = false }
        do {
            let request = try core.shareRequest(id: flight, visibility: visibility)
            let (status, body, _) = try await ask(request, of: account.server)
            return try core.shared(id: flight, status: status, body: body)
        } catch {
            problem = Self.words(for: error)
            return nil
        }
    }

    /// Tells whoever runs the pilot's server that a flight, a pilot's page
    /// or a live track should not be there, and returns in a line what
    /// became of the report. Nothing of it is kept on this iPhone.
    func report(_ subject: ReportSubject, reason: ReportReason, text: String) async throws -> String {
        guard let account, isSignedIn else {
            throw URLError(.userAuthenticationRequired)
        }
        let request = try reportRequest(subject: subject, reason: reason, text: text)
        let (status, body, _) = try await ask(request, of: account.server, timeout: 30)
        return try reportSent(status: status, body: body)
    }

    // MARK: The cockpit's requests

    /// The server's answer to a request of the cockpit's.
    struct Answer: Sendable {
        let status: UInt16
        let body: Data
        let retryAfter: UInt32?
    }

    /// Carries a request of the cockpit's to the pilot's server: the
    /// positions of a flight shared while it is under way. `nil` when no
    /// answer came. It waits twenty seconds for one and no longer: the
    /// next positions are due by then.
    func carry(_ request: ServerRequest) async -> Answer? {
        guard let account else { return nil }
        do {
            let (status, body, retryAfter) = try await ask(request, of: account.server, timeout: 20)
            return Answer(status: status, body: body, retryAfter: retryAfter)
        } catch let error as URLError where error.code == .userAuthenticationRequired {
            // No token to send it with: as good as the server's refusal.
            return Answer(status: 401, body: Data(), retryAfter: nil)
        } catch {
            return nil
        }
    }

    /// Sends a request of another part of the app to the pilot's server,
    /// with the session's token, and returns the answer: the groups and
    /// their plans go this way. Throws when the iPhone is not signed in
    /// and when no answer came.
    func answer(to request: ServerRequest) async throws -> Answer {
        guard let account, isSignedIn else {
            throw URLError(.userAuthenticationRequired)
        }
        let (status, body, retryAfter) = try await ask(request, of: account.server, timeout: 30)
        return Answer(status: status, body: body, retryAfter: retryAfter)
    }

    // MARK: Requests

    /// Sends one of the core's requests to the server, with the session
    /// token where the request needs it and nowhere else.
    private func ask(
        _ request: ServerRequest, of server: String, timeout: TimeInterval = 120
    ) async throws -> (UInt16, Data, UInt32?) {
        guard let url = URL(string: server + request.path) else {
            throw URLError(.badURL)
        }
        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = request.method
        urlRequest.timeoutInterval = timeout
        urlRequest.httpBody = request.body
        if let contentType = request.contentType {
            urlRequest.setValue(contentType, forHTTPHeaderField: "Content-Type")
        }
        urlRequest.setValue("application/json", forHTTPHeaderField: "Accept")
        if request.needsToken {
            guard let token = readToken() else {
                throw URLError(.userAuthenticationRequired)
            }
            urlRequest.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        let (status, body, retryAfter) = try await transport(urlRequest)
        return (UInt16(clamping: status), body, retryAfter)
    }

    static func words(for error: any Error) -> String {
        if error is CoreError {
            return error.words
        }
        if (error as? URLError)?.code == .userAuthenticationRequired {
            return "This iPhone is no longer signed in. Sign in again."
        }
        return "The server could not be reached. \(error.localizedDescription)"
    }

    /// Keeps the session token in the Keychain, or removes it.
    nonisolated static func keep(_ token: String?) {
        guard let token else {
            SessionStore.remove()
            return
        }
        _ = SessionStore.save(token)
    }

    /// Sends with the system's networking, keeping nothing: answers about
    /// an account are not for a cache, and there are no cookies.
    nonisolated static func send(_ request: URLRequest) async throws -> (Int, Data, UInt32?) {
        let (data, response) = try await URLSession(configuration: .ephemeral).data(for: request)
        let http = response as? HTTPURLResponse
        let retryAfter = http?.value(forHTTPHeaderField: "Retry-After").flatMap { UInt32($0) }
        return (http?.statusCode ?? 0, data, retryAfter)
    }
}

extension FlightVisibility {
    /// The setting in words, for a list to choose from.
    var title: String {
        switch self {
        case .private: "Only me"
        case .unlisted: "Anyone with the link"
        case .public: "Everyone"
        }
    }

    /// A name for the setting that tests find its button by.
    var key: String {
        switch self {
        case .private: "private"
        case .unlisted: "unlisted"
        case .public: "public"
        }
    }

    /// What the setting means.
    var explanation: String {
        switch self {
        case .private: "The flight is kept on Venti for your devices. Nobody else can see it."
        case .unlisted: "Whoever has the link can see the flight. It is listed nowhere."
        case .public: "Anyone can see the flight, and it is listed on your page."
        }
    }
}
