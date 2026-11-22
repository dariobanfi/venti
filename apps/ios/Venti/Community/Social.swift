import Foundation
import VentiCore

/// Pilots among each other: following a pilot, the flights of the pilots
/// one follows, cheers and comments under a public flight, and what the
/// pilot was told of them.
///
/// No rule and no word of it is here. The core makes each request and
/// reads each answer into rows and words (`social.rs`); this sends what
/// the core gives, with the session's token where the request needs one
/// and without it where it does not, and keeps nothing: every screen asks
/// when it shows.
@MainActor
struct Social {
    /// Sends a signed request and returns the answer.
    typealias Send = @MainActor (ServerRequest) async throws -> AccountModel.Answer
    /// Fetches an address without a token: what everyone may read.
    typealias Fetch = @Sendable (URL) async throws -> (Int, Data)

    /// The Venti to read from. `nil` without one.
    let server: URL?
    /// Whether the iPhone is signed in on that Venti: without it no
    /// control that needs an account is shown.
    let isSignedIn: Bool
    /// The signed-in pilot's handle.
    let ownHandle: String?
    let send: Send
    let fetch: Fetch
    /// The time now, in seconds since 1970: by it a comment is "3 h ago".
    /// Venti's own clock as it was read, advanced by the time since, and
    /// 0 while it has not been read: the core then says the day in place
    /// of an age. Never the iPhone's own clock, which can be days off.
    let now: @MainActor () async -> Int64

    init(
        server: URL?, ownHandle: String?, send: @escaping Send,
        fetch: @escaping Fetch = SiteEditing.fresh, now: @escaping @MainActor () async -> Int64
    ) {
        self.server = server
        isSignedIn = ownHandle != nil
        self.ownHandle = ownHandle
        self.send = send
        self.fetch = fetch
        self.now = now
    }

    /// With the app's account and the Venti the lists are read from. An
    /// account on another Venti than that one is as good as none here.
    init(account: AccountModel, wind: WindModel) {
        let own = account.account.flatMap { signedIn -> String? in
            guard account.isSignedIn, let shown = wind.server,
                  ReportSheet.same(signedIn.server, shown.absoluteString) else { return nil }
            return signedIn.handle
        }
        self.init(
            server: wind.server, ownHandle: own,
            send: { try await account.answer(to: $0) },
            now: { await wind.serverNowIfRead() }
        )
    }

    /// The words no answer carries: titles, the field's placeholder, the
    /// actions' names.
    nonisolated static let words = socialWords()

    /// Who follows a pilot and whom they follow, in counts, and whether
    /// the pilot of this iPhone does.
    func follows(_ handle: String) async throws -> FollowList {
        let (status, body) = try await answer(to: followersRequest(handleText: handle, after: nil, signedIn: isSignedIn))
        return try followList(status: status, body: body)
    }

    /// Follows a pilot, or follows them no more.
    func follow(_ handle: String, _ follow: Bool) async throws -> FollowState {
        let (status, body) = try await answer(to: followRequest(handleText: handle, follow: follow))
        return try followAnswer(status: status, body: body)
    }

    /// A page of the flights of the pilots one follows; `before` is the
    /// `next` of the page before.
    func feed(before: String?) async throws -> Feed {
        let (status, body) = try await answer(to: feedRequest(before: before))
        return try VentiCore.feed(status: status, body: body)
    }

    /// How many cheered a flight, and whether the pilot of this iPhone
    /// did.
    func cheers(_ flight: String) async throws -> CheerList {
        let (status, body) = try await answer(to: cheersRequest(flightId: flight, after: nil, signedIn: isSignedIn))
        return try cheerList(status: status, body: body)
    }

    /// Cheers a flight, or takes the cheer back.
    func cheer(_ flight: String, _ cheer: Bool) async throws -> CheerState {
        let (status, body) = try await answer(to: cheerRequest(flightId: flight, cheer: cheer))
        return try cheerAnswer(status: status, body: body)
    }

    /// How many pages of one list are read in a row.
    private static let pagesAtMost = 10

    /// The comments under a flight, all of its pages: the core's list
    /// with every page's comments in it. `flightHandle` is the handle of
    /// the flight's pilot, who may remove what stands under it.
    func comments(_ flight: String, of flightHandle: String?) async throws -> CommentList {
        var all: CommentList?
        var after: String?
        var pages = 0
        repeat {
            let (status, body) = try await answer(to: commentsRequest(flightId: flight, after: after))
            var page = try commentList(
                status: status, body: body, nowUnixS: await now(),
                ownHandle: ownHandle, flightHandle: flightHandle
            )
            after = page.next
            page.comments = (all?.comments ?? []) + page.comments
            all = page
            pages += 1
        } while after != nil && pages < Self.pagesAtMost
        guard let all else { throw URLError(.badServerResponse) }
        return all
    }

    /// Posts a comment under a flight.
    func post(_ text: String, under flight: String) async throws -> CommentPosted {
        let (status, body) = try await answer(to: commentRequest(flightId: flight, text: text))
        return try commentPosted(status: status, body: body, nowUnixS: await now())
    }

    /// Removes a comment: the pilot's own, or one under a flight of
    /// theirs.
    func remove(_ comment: String, under flight: String) async throws {
        let (status, body) = try await answer(to: commentRemoveRequest(flightId: flight, commentId: comment))
        try socialDone(status: status, body: body)
    }

    /// Says whether a flight of the pilot's own takes comments.
    func commentsOpen(_ flight: String, _ open: Bool) async throws -> CommentsSwitch {
        let (status, body) = try await answer(to: commentsOpenRequest(flightId: flight, open: open))
        return try commentsOpenAnswer(status: status, body: body)
    }

    /// What the pilot was told, from the beginning: the iPhone keeps no
    /// cursor, so each look reads all Venti still has, as the core hands
    /// it over.
    func notifications() async throws -> Notifications {
        var all: Notifications?
        var since: String?
        var rounds = 0
        var more = false
        repeat {
            let (status, body) = try await answer(to: notificationsRequest(since: since))
            var read = try VentiCore.notifications(status: status, body: body, nowUnixS: await now())
            if let all, !read.reset {
                read.lines = all.lines + read.lines
            }
            since = read.cursor
            more = read.more
            all = read
            rounds += 1
        } while more && rounds < Self.pagesAtMost
        guard let all else { throw URLError(.badServerResponse) }
        return all
    }

    /// Sends one of the core's requests: with the session's token where
    /// it needs one, and as anyone where it does not.
    private func answer(to request: ServerRequest) async throws -> (UInt16, Data) {
        if request.needsToken {
            let answer = try await send(request)
            return (answer.status, answer.body)
        }
        guard let server, let url = URL(string: server.absoluteString + request.path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await fetch(url)
        return (UInt16(clamping: status), body)
    }

    /// What went wrong, in words for the pilot: the core's refusal, or
    /// the core's sentence that Venti was not reached.
    static func words(for error: any Error) -> String {
        if error is CoreError {
            return error.words
        }
        if (error as? URLError)?.code == .userAuthenticationRequired {
            return AccountModel.words(for: error)
        }
        return pilotsWords().unreachable
    }
}
