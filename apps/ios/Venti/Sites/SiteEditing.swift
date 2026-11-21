import Foundation
import VentiCore

/// Adding and correcting a flying site of Venti's own layer, as the
/// screens ask for it.
///
/// What a site has to be, what is sent and what the answers mean is the
/// core's (`site_edit.rs`). This sends the core's requests with the
/// account's token, as the groups' and the plans' are sent, reads a site
/// and its history without one, and holds no rule.
@MainActor
struct SiteEditing {
    /// Sends a signed request and returns the answer.
    typealias Send = @MainActor (ServerRequest) async throws -> AccountModel.Answer

    /// The Venti to read from. `nil` without one.
    let server: URL?
    let send: Send
    let fetch: SitesModel.Transport

    init(server: URL?, send: @escaping Send, fetch: @escaping SitesModel.Transport = SiteEditing.fresh) {
        self.server = server
        self.send = send
        self.fetch = fetch
    }

    /// With the app's account and the Venti the map shows.
    init(account: AccountModel, wind: WindModel) {
        self.init(server: wind.server, send: { try await account.answer(to: $0) })
    }

    /// A refusal of something a pilot sent, in the core's words, with the
    /// site that stands in the way where that is why.
    struct Refused: Error, Equatable {
        let words: String
        let inTheWay: SiteInTheWay?
    }

    /// Sends a draft: a new site, or the correction of the site the draft
    /// was made from.
    func write(_ draft: SiteDraftView, comment: String) async throws -> SiteWritten {
        let request = draft.siteId == nil
            ? try siteAddRequest(draft: draft, comment: comment)
            : try siteChangeRequest(draft: draft, comment: comment)
        return try await written(request)
    }

    /// Puts a revision of a site back: that is also how the last change is
    /// undone and a removed site brought back.
    func putBack(_ id: String, revision: UInt32, comment: String) async throws -> SiteWritten {
        try await written(try siteRestoreRequest(id: id, revision: revision, comment: comment))
    }

    /// Removes a site from the map and returns what became of it in a line.
    func remove(_ id: String, comment: String) async throws -> String {
        let answer = try await send(try siteRemoveRequest(id: id, comment: comment))
        return try siteRemoved(status: answer.status, body: answer.body)
    }

    /// One site as it stands now.
    func page(_ id: String) async throws -> SitePage {
        let (status, body) = try await get(try sitePath(id: id))
        return try sitePage(status: status, body: body)
    }

    /// A site's revisions, newest first; `after` asks for the ones before
    /// those already shown.
    func history(_ id: String, after: UInt32? = nil) async throws -> SiteHistory {
        let (status, body) = try await get(try siteHistoryPath(id: id, after: after))
        return try siteHistory(status: status, body: body)
    }

    private func written(_ request: ServerRequest) async throws -> SiteWritten {
        let answer = try await send(request)
        do {
            return try siteWritten(status: answer.status, body: answer.body)
        } catch {
            throw Refused(
                words: AccountModel.words(for: error),
                inTheWay: siteInTheWay(status: answer.status, body: answer.body)
            )
        }
    }

    private func get(_ path: String) async throws -> (UInt16, Data) {
        guard let server, let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await fetch(url)
        return (UInt16(clamping: status), body)
    }

    /// Fetches from Venti itself, around the system's cache: a site and
    /// its history are read to be changed, and the map's sites are asked
    /// for again right after a change, when an answer the cache may still
    /// hand out is known to be old.
    nonisolated static func fresh(_ url: URL) async throws -> (Int, Data) {
        var request = URLRequest(url: url)
        request.timeoutInterval = 20
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await URLSession.shared.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, data)
    }

    /// What went wrong, in words for the pilot: the core's, or that Venti
    /// was not reached.
    static func words(for error: any Error) -> String {
        (error as? Refused)?.words ?? AccountModel.words(for: error)
    }
}

extension SiteDraftView {
    /// The height as a form's field holds it: whole metres, or nothing.
    var heightText: String {
        elevationM.map { Format.metres($0).replacingOccurrences(of: "−", with: "-") } ?? ""
    }

    /// Takes the height from a form's field. An empty field is a site
    /// whose height the pilot does not know; anything that is no number is
    /// handed to the core as one, for its check to say what a height is.
    mutating func setHeight(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ",", with: ".")
        elevationM = text.isEmpty ? nil : Double(text) ?? .nan
    }

    /// Ticks a wind direction, or takes the tick off.
    mutating func toggleWind(_ fromDeg: Double) {
        if let index = windFromDeg.firstIndex(of: fromDeg) {
            windFromDeg.remove(at: index)
        } else {
            windFromDeg.append(fromDeg)
        }
    }

    /// Ticks a use, or takes the tick off.
    mutating func toggleUse(_ kind: SiteKind) {
        if let index = uses.firstIndex(of: kind) {
            uses.remove(at: index)
        } else {
            uses.append(kind)
        }
    }

    /// The draft on a site that someone else changed meanwhile: what the
    /// pilot entered stays, and everything the pilot left as it was is as
    /// the site stands now, so that sending again neither loses the
    /// pilot's entries nor takes the other pilot's change back.
    ///
    /// - Parameters:
    ///   - begun: the draft as the form opened with it.
    ///   - now: the draft of the site as it stands now.
    func carried(from begun: SiteDraftView, onto now: SiteDraftView) -> SiteDraftView {
        var result = now
        func carry<Value: Equatable>(_ field: WritableKeyPath<SiteDraftView, Value>) {
            if self[keyPath: field] != begun[keyPath: field] {
                result[keyPath: field] = self[keyPath: field]
            }
        }
        carry(\.name)
        carry(\.elevationM)
        carry(\.uses)
        carry(\.windFromDeg)
        carry(\.description)
        carry(\.fee)
        carry(\.website)
        carry(\.noFlightTime)
        carry(\.guestGuidelines)
        // A place the pilot moved goes through the core, which knows what
        // else of a draft goes with the old place.
        if position != begun.position {
            result = siteDraftMoved(draft: result, place: position)
        }
        return result
    }
}
