import Foundation
import Observation
import VentiCore

/// The messages of a chat as the screen shows them, the oldest first, and
/// where to read on from: the page before (`older`) and what is new
/// (`since`). Pages and what is new are laid in by their place in the
/// chat (`cursor`), each message once, as it was last read: a message
/// removed or a plan's card changed is shown as it is now.
struct ChatLog: Equatable {
    private(set) var messages: [ChatMessage] = []
    /// What to hand the core for the page before; `nil` at the first
    /// message, and before the first page.
    private(set) var older: String?
    /// What to hand the core for what is new; `nil` before the first page.
    private(set) var since: String?

    /// The newest page, which the chat opens on: what was there before
    /// goes.
    mutating func takeNewest(_ page: ChatPage) {
        messages = []
        lay(page.messages)
        older = page.older
        since = page.since
    }

    /// The page before the oldest shown.
    mutating func takeOlder(_ page: ChatPage) {
        lay(page.messages)
        older = page.older
    }

    /// What was posted or changed since the last answer.
    mutating func takeNew(_ page: ChatPage) {
        lay(page.messages)
        since = page.since
    }

    /// One message, as posting or removing it answered.
    mutating func take(_ message: ChatMessage) {
        lay([message])
    }

    /// Lays messages in by their place, each once, as last read.
    private mutating func lay(_ given: [ChatMessage]) {
        for message in given {
            if let at = messages.firstIndex(where: { $0.id == message.id }) {
                messages[at] = message
            } else {
                messages.append(message)
            }
        }
        messages.sort { Self.place($0.cursor) < Self.place($1.cursor) }
    }

    /// A cursor's place in the chat: the server writes it as a number.
    nonisolated static func place(_ cursor: String) -> UInt64 {
        UInt64(cursor) ?? 0
    }
}

/// One group's chat while its screen is open: the newest page, older
/// pages as the pilot scrolls up, and what is new every five seconds
/// (`docs/api.md`, "Groups as communities with a chat": there is no
/// stream). It marks the chat read up to the newest message shown. The
/// core puts every request together and reads every answer; nothing is
/// kept on the iPhone.
@MainActor
@Observable
final class ChatModel {
    /// Sends a request with the session's token and returns the answer.
    typealias Send = @MainActor (ServerRequest) async throws -> AccountModel.Answer

    /// How long the chat waits between two questions of what is new.
    static let pollSeconds: UInt64 = 5

    let group: String
    private(set) var log = ChatLog()
    /// Whether the first page has come.
    private(set) var hasLoaded = false
    private(set) var isLoadingOlder = false
    /// Why the chat could not be read, in words for the pilot; the
    /// messages read before stay.
    private(set) var problem: String?
    /// How the pilot stands to the group since the chat was last marked
    /// read: what the server says is left unread.
    private(set) var standing: CommunityStanding?

    /// The place up to which the chat was marked read, or is being.
    @ObservationIgnored private var markedUpTo: UInt64 = 0
    @ObservationIgnored private var toMark: String?
    @ObservationIgnored private var isMarking = false

    @ObservationIgnored private let core: Core
    @ObservationIgnored private let send: Send

    init(group: String, core: Core, send: @escaping Send) {
        self.group = group
        self.core = core
        self.send = send
    }

    /// The first page, then what is new every five seconds until the
    /// task that runs it is cancelled: the screen's, when it goes.
    func run() async {
        await loadNewest()
        while !Task.isCancelled {
            do {
                try await Task.sleep(nanoseconds: Self.pollSeconds * 1_000_000_000)
            } catch {
                return
            }
            await loadNew()
        }
    }

    /// The newest page.
    func loadNewest() async {
        let (core, group) = (core, group)
        do {
            let page = try await ask({ try core.messagesRequest(id: group, before: nil) }) {
                try core.chatPage(status: $0, body: $1)
            }
            log.takeNewest(page)
            hasLoaded = true
            problem = nil
        } catch {
            note(error)
        }
    }

    /// The page before the oldest shown, where there is one.
    func loadOlder() async {
        guard let before = log.older, !isLoadingOlder else { return }
        isLoadingOlder = true
        defer { isLoadingOlder = false }
        let (core, group) = (core, group)
        do {
            let page = try await ask({ try core.messagesRequest(id: group, before: before) }) {
                try core.chatPage(status: $0, body: $1)
            }
            log.takeOlder(page)
        } catch {
            note(error)
        }
    }

    /// What was posted or changed since the last answer; again at once
    /// while the server says there is more.
    func loadNew() async {
        guard let since = log.since else {
            await loadNewest()
            return
        }
        let (core, group) = (core, group)
        var from = since
        for _ in 0..<10 {
            do {
                let page = try await ask({ try core.messagesSinceRequest(id: group, since: from) }) {
                    try core.chatPage(status: $0, body: $1)
                }
                log.takeNew(page)
                problem = nil
                guard page.more else { return }
                from = page.since
            } catch {
                note(error)
                return
            }
        }
    }

    /// Posts a message: its text, the pictures sent for it and the links
    /// chosen; the core finds Venti's links in the text.
    func post(text: String, pictures: [String], links: [LinkDraft] = []) async throws {
        let (core, group) = (core, group)
        let draft = MessageDraft(text: text, pictures: pictures, links: links)
        let message = try await ask({ try core.postMessageRequest(id: group, draft: draft) }) {
            try core.chatMessage(status: $0, body: $1)
        }
        log.take(message)
        // Posting moves the pilot's own mark.
        markedUpTo = max(markedUpTo, ChatLog.place(message.cursor))
    }

    /// Sends a picture for a message, a JPEG.
    func sendPicture(_ jpeg: Data) async throws -> SentGroupPicture {
        let (core, group) = (core, group)
        return try await ask({
            try core.sendGroupPictureRequest(id: group, picture: jpeg, mediaType: PictureUpload.mediaType)
        }) { try core.groupPictureSent(status: $0, body: $1) }
    }

    /// Removes a message: the pilot's own, or as owner or admin anyone's.
    func remove(_ message: ChatMessage) async throws {
        let (core, group) = (core, group)
        let removed = try await ask({ try core.removeMessageRequest(id: group, message: message.id) }) {
            try core.chatMessage(status: $0, body: $1)
        }
        log.take(removed)
    }

    /// The pilot was shown a message: the chat is marked read up to it,
    /// one request at a time and only forward.
    func saw(_ message: ChatMessage) {
        let place = ChatLog.place(message.cursor)
        guard place > markedUpTo, place > ChatLog.place(toMark ?? "0") else { return }
        toMark = message.cursor
        guard !isMarking else { return }
        isMarking = true
        Task {
            defer { isMarking = false }
            while let cursor = toMark {
                toMark = nil
                let place = ChatLog.place(cursor)
                guard place > markedUpTo else { continue }
                let (core, group) = (core, group)
                if let standing = try? await ask({ try core.groupReadRequest(id: group, cursorOf: cursor) }, read: {
                    try core.groupStanding(status: $0, body: $1)
                }) {
                    markedUpTo = max(markedUpTo, place)
                    self.standing = standing
                }
            }
        }
    }

    private func note(_ error: any Error) {
        if error is CancellationError || (error as? URLError)?.code == .cancelled { return }
        problem = AccountModel.words(for: error)
    }

    private func ask<Value>(
        _ request: () throws -> ServerRequest, read: (UInt16, Data) throws -> Value
    ) async throws -> Value {
        let answer = try await send(try request())
        return try read(answer.status, answer.body)
    }
}
