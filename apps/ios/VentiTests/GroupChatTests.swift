import VentiCore
import XCTest

@testable import Venti

/// Groups as communities with a chat, against the real core, with the
/// server played by canned answers in the server's own format (the
/// server's golden files).
@MainActor
final class GroupChatTests: XCTestCase {
    private actor Sent {
        var requests: [URLRequest] = []
        func add(_ request: URLRequest) { requests.append(request) }
    }

    private final class TokenBox: @unchecked Sendable {
        var token: String?
    }

    private func makeCore() throws -> Core {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-chat-test-\(UUID().uuidString)", isDirectory: true)
        return try Core(dataFolder: folder.path)
    }

    private func server(noting sent: Sent) -> AccountModel.Transport {
        { request in
            await sent.add(request)
            let path = request.url?.path ?? ""
            let query = request.url?.query ?? ""
            switch (request.httpMethod ?? "", path) {
            case ("POST", "/v1/sessions"):
                return (201, Data(ChatAnswers.signIn.utf8), nil)
            case ("GET", "/v1/flights/changes"):
                return (200, Data(#"{"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}"#.utf8), nil)
            case ("GET", "/v1/groups"):
                return (200, Data(ChatAnswers.groups.utf8), nil)
            case ("POST", "/v1/groups"):
                return (201, Data(ChatAnswers.created.utf8), nil)
            case ("GET", "/v1/groups/44444444444444444444444444"):
                return (200, Data(ChatAnswers.group.utf8), nil)
            case ("GET", "/v1/groups/plans"):
                return (200, Data(#"{"groups": [], "plans": [], "truncated": false}"#.utf8), nil)
            case ("GET", "/v1/groups/invites"):
                return (200, Data(#"{"invites": []}"#.utf8), nil)
            case ("GET", "/v1/groups/find"):
                return (200, Data(ChatAnswers.found.utf8), nil)
            case ("GET", let path) where path.hasSuffix("/messages") && query.hasPrefix("since="):
                return (200, Data(ChatAnswers.removedSince.utf8), nil)
            case ("GET", let path) where path.hasSuffix("/messages"):
                return (200, Data(ChatAnswers.page.utf8), nil)
            case ("POST", let path) where path.hasSuffix("/messages"):
                return (201, Data(ChatAnswers.posted.utf8), nil)
            case ("PATCH", let path) where path.hasSuffix("/me"):
                return (200, Data(#"{"me": {"role": "member", "invited": false, "muted": false, "unread": 0, "read_up_to": "14"}}"#.utf8), nil)
            default:
                return (404, Data(#"{"error": {"code": "group_not_found", "message": "x"}}"#.utf8), nil)
            }
        }
    }

    private func signedIn() async throws -> (Core, AccountModel, Sent) {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent),
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        await account.signIn(
            server: try XCTUnwrap(URL(string: "https://venti.example.org")), handle: "demo",
            password: "demo-pilot-password"
        )
        return (core, account, sent)
    }

    private func eventually(_ what: @MainActor () async -> Bool) async -> Bool {
        for _ in 0..<50 {
            if await what() { return true }
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        return false
    }

    // MARK: The log

    private func message(_ id: String, cursor: String, text: String, removed: String? = nil) -> ChatMessage {
        ChatMessage(
            id: id, cursor: cursor, authorHandle: "mara", authorName: "Mara Muster", isMine: false, text: text,
            pictures: [], pictureIds: [], cards: [], plan: nil, postedAt: "2026-11-18T15:45:00Z",
            removedWords: removed
        )
    }

    func testTheLogLaysPagesAndWhatIsNewInByTheirPlace() {
        var log = ChatLog()
        // A page is the newest first; the log shows the oldest first.
        log.takeNewest(ChatPage(
            messages: [message("c", cursor: "12", text: "three"), message("b", cursor: "11", text: "two")],
            older: "11", since: "12", more: false
        ))
        XCTAssertEqual(log.messages.map(\.id), ["b", "c"])
        XCTAssertEqual(log.older, "11")
        XCTAssertEqual(log.since, "12")

        // The page before goes before, and says where to read on.
        log.takeOlder(ChatPage(messages: [message("a", cursor: "9", text: "one")], older: nil, since: "12", more: false))
        XCTAssertEqual(log.messages.map(\.id), ["a", "b", "c"])
        XCTAssertNil(log.older)

        // What is new: a removal keeps its place, a new message comes last.
        log.takeNew(ChatPage(
            messages: [message("b", cursor: "11", text: "", removed: "Removed by its author"), message("d", cursor: "13", text: "four")],
            older: nil, since: "13", more: false
        ))
        XCTAssertEqual(log.messages.map(\.id), ["a", "b", "c", "d"])
        XCTAssertEqual(log.messages[1].removedWords, "Removed by its author")
        XCTAssertEqual(log.since, "13")

        // A message posted here and then read again is there once; one
        // posted elsewhere a moment before it stands before it.
        log.take(message("f", cursor: "15", text: "mine"))
        log.takeNew(ChatPage(
            messages: [message("e", cursor: "14", text: "theirs"), message("f", cursor: "15", text: "mine")],
            older: nil, since: "15", more: false
        ))
        XCTAssertEqual(log.messages.map(\.id), ["a", "b", "c", "d", "e", "f"])

        // The newest page again starts the log anew.
        log.takeNewest(ChatPage(messages: [message("f", cursor: "15", text: "mine")], older: "15", since: "15", more: false))
        XCTAssertEqual(log.messages.map(\.id), ["f"])
    }

    // MARK: The chat

    func testAChatReadsItsPagesPostsAndMarksWhatWasShownRead() async throws {
        let (core, account, sent) = try await signedIn()
        let chat = ChatModel(group: "0000000000000000000000g002", core: core) { try await account.answer(to: $0) }

        await chat.loadNewest()
        XCTAssertTrue(chat.hasLoaded)
        XCTAssertNil(chat.problem)
        XCTAssertEqual(chat.log.messages.map(\.cursor), ["13", "14"])
        let first = try XCTUnwrap(chat.log.messages.first)
        XCTAssertEqual(first.authorName, "Mara Muster")
        let card = try XCTUnwrap(first.cards.first)
        XCTAssertEqual(card.kind, "flight")
        XCTAssertEqual(card.track.count, 3)
        XCTAssertEqual(MessageRow.unixSeconds(first.postedAt), 1_795_016_700)

        // What is new is asked with the answer's `since`.
        await chat.loadNew()
        let asked = await sent.requests.last
        XCTAssertEqual(asked?.url?.query, "since=14")
        XCTAssertEqual(chat.log.messages.map(\.id).count, 2)
        XCTAssertNotNil(chat.log.messages.last?.removedWords)

        // A post: Venti's links in the text become links of the request.
        try await chat.post(text: "Look: https://venti.fly/pilots/mara", pictures: [])
        let posts = await sent.requests.filter { $0.httpMethod == "POST" }
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: try XCTUnwrap(posts.last?.httpBody)) as? [String: Any])
        let links = try XCTUnwrap(body["links"] as? [[String: Any]])
        XCTAssertEqual(links.first?["kind"] as? String, "pilot")
        XCTAssertEqual(links.first?["id"] as? String, "mara")
        XCTAssertEqual(chat.log.messages.last?.text, "Hello from the iPhone")

        // Posting moved the pilot's own mark: a message before it that is
        // shown marks nothing.
        chat.saw(try XCTUnwrap(chat.log.messages.first { $0.cursor == "14" }))
        try? await Task.sleep(nanoseconds: 100_000_000)
        let read = await sent.requests.filter { $0.httpMethod == "PATCH" }.count
        XCTAssertEqual(read, 0)

        // A message posted after the mark is marked read when shown.
        let later = ChatMessage(
            id: "later", cursor: "99", authorHandle: "mara", authorName: "Mara Muster", isMine: false, text: "x",
            pictures: [], pictureIds: [], cards: [], plan: nil, postedAt: "2026-11-18T18:00:00Z", removedWords: nil
        )
        chat.saw(later)
        chat.saw(later)
        let marked = await eventually { await sent.requests.contains { $0.httpMethod == "PATCH" } }
        XCTAssertTrue(marked)
        let patches = await sent.requests.filter { $0.httpMethod == "PATCH" }
        XCTAssertEqual(patches.count, 1)
        let patch = try XCTUnwrap(JSONSerialization.jsonObject(with: try XCTUnwrap(patches.first?.httpBody)) as? [String: Any])
        XCTAssertEqual(patch["read_up_to"] as? String, "99")
        XCTAssertEqual(chat.standing?.unread, 0)
    }

    func testThePilotsGroupsAreCardsAndThoseNearAreFoundByPlace() async throws {
        let (core, account, sent) = try await signedIn()
        let groups = GroupsModel(core: core, account: account)
        await groups.loadChats()
        let chats = try XCTUnwrap(groups.chats)
        XCTAssertEqual(chats.map(\.name), ["Mustergruppe"])
        XCTAssertEqual(chats.first?.summary, "3 members")
        XCTAssertEqual(chats.first?.myRole, .owner)
        XCTAssertEqual(chats.first?.lastMessage, "Jonas Beispiel: a plan")
        XCTAssertNil(chats.first?.unreadBadge)

        let found = try await groups.find(near: Coordinate(lat: 48.137, lon: 11.575), text: "")
        XCTAssertEqual(found.first?.group.name, "Munich International Paragliders")
        XCTAssertEqual(found.first?.distance, "Here")
        XCTAssertEqual(found.first?.isMember, false)
        let finding = await sent.requests.last { $0.url?.path == "/v1/groups/find" }
        XCTAssertEqual(finding?.url?.query, "lat=48.1370&lon=11.5750")
    }

    func testAGroupIsMadeWithItsHomeAndOpensWithItsMembers() async throws {
        let (core, account, sent) = try await signedIn()
        let groups = GroupsModel(core: core, account: account)
        let made = try await groups.create(
            CommunityDraft(
                name: " Mustergruppe ", description: "",
                home: HomeDraft(center: Coordinate(lat: 47.7, lon: 12.2), radiusKm: 30, place: "Chiemgau")
            ),
            isPublic: false, picture: nil
        )
        XCTAssertEqual(made.group.name, "Mustergruppe")
        XCTAssertEqual(made.members.map(\.roleWords), ["Owner", "", ""])
        XCTAssertTrue(made.canModerate)
        let posted = await sent.requests.last { $0.httpMethod == "POST" && $0.url?.path == "/v1/groups" }
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: try XCTUnwrap(posted?.httpBody)) as? [String: Any])
        XCTAssertEqual(body["name"] as? String, "Mustergruppe")
        XCTAssertEqual(body["public"] as? Bool, false)
        let home = try XCTUnwrap(body["home"] as? [String: Any])
        XCTAssertEqual(home["place"] as? String, "Chiemgau")
        XCTAssertEqual(home["radius_km"] as? Double, 30)

        // A home the server would refuse is refused before it is sent.
        let count = await sent.requests.count
        do {
            _ = try await groups.create(
                CommunityDraft(name: "X", description: "", home: HomeDraft(center: Coordinate(lat: 47.7, lon: 12.2), radiusKm: 500, place: "")),
                isPublic: true, picture: nil
            )
            XCTFail("A home of 500 km was sent")
        } catch {
            XCTAssertTrue(AccountModel.words(for: error).contains("1 to 100 km"), AccountModel.words(for: error))
        }
        let after = await sent.requests.count
        XCTAssertEqual(after, count)
    }

    // MARK: The tab's badge

    func testTheTabsBadgeIsTheNotificationsAndTheGroupsTogether() async throws {
        var body = #"{"notifications": 4, "alarms": 1, "total": 5, "groups": 0}"#
        let unread = UnreadModel(send: { _ in AccountModel.Answer(status: 200, body: Data(body.utf8), retryAfter: nil) },
                                 isSignedIn: { true })
        await unread.read()
        XCTAssertEqual(unread.badge, "5")
        XCTAssertEqual(unread.label, "5 new")

        body = #"{"notifications": 4, "alarms": 1, "total": 5, "groups": 3}"#
        await unread.read()
        XCTAssertEqual(unread.badge, "8")
        XCTAssertEqual(unread.label, "8 new")

        body = #"{"notifications": 0, "alarms": 0, "total": 0, "groups": 120}"#
        await unread.read()
        XCTAssertEqual(unread.badge, "99+")

        XCTAssertNil(UnreadModel.badge(of: 0))
        XCTAssertEqual(UnreadModel.badge(of: 99), "99")
    }
}

private enum ChatAnswers {
    static let signIn = """
        {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z"},
         "session": {"id": "session0000000000000000000a", "created_at": "2026-11-18T19:05:00Z",
                     "last_used_at": "2026-11-18T19:05:00Z", "current": true},
         "token": "venti_test_token"}
        """

    static let groups = """
        {"groups": [{"id": "44444444444444444444444444", "name": "Mustergruppe", "created_at": "2026-10-19T19:05:00Z",
                     "keeper": {"handle": "demo", "display_name": "Demo Pilot"}, "member_count": 3, "public": false}],
         "chats": [{"group": "44444444444444444444444444",
                    "me": {"role": "owner", "invited": false, "muted": false, "unread": 0, "read_up_to": "6"},
                    "last_message": {"id": "88888888888888888888888888", "group": "44444444444444444444444444",
                                     "cursor": "6", "author": {"handle": "jonas", "display_name": "Jonas Beispiel"},
                                     "kind": "plan", "text": "", "created_at": "2026-11-18T18:05:00Z"}}]}
        """

    static let created = """
        {"group": {"id": "44444444444444444444444444", "name": "Mustergruppe", "created_at": "2026-10-19T19:05:00Z",
                   "keeper": {"handle": "demo", "display_name": "Demo Pilot"}, "member_count": 1, "public": false},
         "invite_code": "7k3m-9q2x-4r8t-1v6w"}
        """

    static let group = """
        {"group": {"id": "44444444444444444444444444", "name": "Mustergruppe", "created_at": "2026-10-19T19:05:00Z",
                   "keeper": {"handle": "demo", "display_name": "Demo Pilot"}, "member_count": 3, "public": false},
         "members": [{"handle": "demo", "display_name": "Demo Pilot"}, {"handle": "mara", "display_name": "Mara Muster"},
                     {"handle": "jonas", "display_name": "Jonas Beispiel"}],
         "admins": [], "me": {"role": "owner", "invited": false, "muted": false, "unread": 0, "read_up_to": "6"}}
        """

    static let found = """
        {"groups": [{"group": {"id": "0000000000000000000000g001", "name": "Munich International Paragliders",
                               "created_at": "2026-09-19T19:05:00Z", "keeper": {"handle": "mara", "display_name": "Mara Muster"},
                               "member_count": 3, "public": true, "description": "Pilots in and around Munich.",
                               "home": {"center": {"lat": 48.137, "lon": 11.575}, "radius_km": 40.0, "place": "Munich"}},
                     "distance_km": 0.5, "member": false}]}
        """

    static let timo = """
        {"id": "0000000000000000000g002m01", "group": "0000000000000000000000g002", "cursor": "14",
         "author": {"handle": "timo", "display_name": "Timo Entwurf"}, "kind": "text",
         "text": "Nice line! How was the wind on top?", "created_at": "2026-11-18T17:35:00Z"}
        """

    static let page = """
        {"messages": [\(timo),
          {"id": "0000000000000000000g002m00", "group": "0000000000000000000000g002", "cursor": "13",
           "author": {"handle": "mara", "display_name": "Mara Muster"}, "kind": "text",
           "text": "My flight from yesterday, the inversion broke late.",
           "cards": [{"kind": "flight", "id": "fbyw03atd9qdt5bfe177ra1k94",
                      "pilot": {"handle": "mara", "display_name": "Mara Muster", "picture": "/v1/pictures/0000000000000000000000p001"},
                      "distance_km": 12.0, "date": "2026-08-16",
                      "track": [[47.6642, 11.7967], [47.6697, 11.78986], [47.67054, 11.78942]]}],
           "created_at": "2026-11-18T15:45:00Z"}],
         "since": "14", "more": false}
        """

    static let removedSince = """
        {"messages": [{"id": "0000000000000000000g002m01", "group": "0000000000000000000000g002", "cursor": "14",
                       "author": {"handle": "timo", "display_name": "Timo Entwurf"}, "kind": "text", "text": "",
                       "created_at": "2026-11-18T17:35:00Z",
                       "removed": {"at": "2026-11-18T18:00:00Z", "by": "author"}}],
         "since": "15", "more": false}
        """

    static let posted = """
        {"message": {"id": "0000000000000000000g002m02", "group": "0000000000000000000000g002", "cursor": "16",
                     "author": {"handle": "demo", "display_name": "Demo Pilot"}, "kind": "text",
                     "text": "Hello from the iPhone", "created_at": "2026-11-18T18:10:00Z"}}
        """
}
