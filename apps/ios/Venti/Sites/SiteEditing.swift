import Foundation
import VentiCore

/// Adding, correcting and confirming a flying site of Venti's own two
/// tiers, as the screens ask for it.
///
/// What a site has to be, what is sent and what the answers mean is the
/// core's (`site_edit.rs`, `site_offers.rs`). This sends the core's
/// requests with the account's token, as the groups' and the plans' are
/// sent, reads a site, its history and its confirmations without one, and
/// holds no rule.
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
        /// Whether it was refused because someone else changed the site
        /// meanwhile, as the core reads the answer: the form then goes on
        /// from the site as it stands, with the core's merge.
        var wasChanged = false
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

    /// A page of a site's revisions, from the first on; `after` asks for
    /// the page after the one that ended there, and `before` is that
    /// page's last revision, so that the core says what the first of this
    /// page changed.
    func history(
        _ id: String, after: UInt32? = nil, before: SiteRevisionView? = nil
    ) async throws -> SiteHistory {
        let (status, body) = try await get(try siteHistoryPath(id: id, after: after))
        return try siteHistoryPage(status: status, body: body, before: before)
    }

    /// The whole of a site's history, the first revision first. The
    /// core's pages run from the first revision on, and it names the
    /// revision that undoes the last change, or brings a removed site
    /// back, on the last of them: all pages are read, a site's history
    /// being short, each with the last revision of the page before so
    /// that the core says what its first one changed, and the answer is
    /// the last page with the revisions of all.
    func wholeHistory(_ id: String) async throws -> SiteHistory {
        var page = try await history(id)
        var revisions = page.revisions
        var pages = 1
        while let after = page.nextAfter, pages < Self.pagesAtMost {
            page = try await history(id, after: after, before: revisions.last)
            revisions += page.revisions
            pages += 1
        }
        page.revisions = revisions
        return page
    }

    /// How many pages of a history are read before the list stops.
    private static let pagesAtMost = 20

    /// Who says they took off at a site, as the core reads the list for
    /// that site.
    func confirmations(of site: SiteView) async throws -> SiteConfirmations {
        let (status, body) = try await get(try siteConfirmationsPath(id: site.id))
        return try siteConfirmationsOf(site: site, status: status, body: body)
    }

    /// Sends "I took off here and it was good", as the core made the
    /// request, and returns the site with it counted and what to say.
    func confirm(_ request: ServerRequest, day: String) async throws -> SiteWritten {
        let answer = try await send(request)
        return try siteConfirmed(status: answer.status, body: answer.body, day: day)
    }

    /// Takes the pilot's confirmation of a site back, and returns what
    /// became of it in a line.
    func withdraw(_ id: String) async throws -> String {
        let answer = try await send(try siteWithdrawRequest(id: id))
        return try siteWithdrawn(status: answer.status, body: answer.body)
    }

    private func written(_ request: ServerRequest) async throws -> SiteWritten {
        let answer = try await send(request)
        do {
            return try siteWritten(status: answer.status, body: answer.body)
        } catch {
            throw Refused(
                words: AccountModel.words(for: error),
                inTheWay: siteInTheWay(status: answer.status, body: answer.body),
                wasChanged: siteWasChanged(status: answer.status, body: answer.body)
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
}

/// A day as the core takes one, "2026-11-21", on this iPhone's calendar.
enum SiteDay {
    private static var calendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .current
        return calendar
    }

    /// The day a moment falls on here.
    static func day(unixSeconds: Int64) -> String {
        let parts = calendar.dateComponents(
            [.year, .month, .day], from: Date(timeIntervalSince1970: TimeInterval(unixSeconds))
        )
        return String(format: "%04d-%02d-%02d", parts.year ?? 0, parts.month ?? 0, parts.day ?? 0)
    }

    /// How far this iPhone's clock is ahead of UTC at noon of a day,
    /// seconds: what the core counts a flight to the pilot's day by.
    static func utcOffsetS(on day: String) -> Int32 {
        let numbers = day.split(separator: "-").compactMap { Int($0) }
        guard numbers.count == 3,
              let noon = calendar.date(
                  from: DateComponents(year: numbers[0], month: numbers[1], day: numbers[2], hour: 12)
              )
        else { return Int32(clamping: TimeZone.current.secondsFromGMT()) }
        return Int32(clamping: TimeZone.current.secondsFromGMT(for: noon))
    }
}
