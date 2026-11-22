import SwiftUI
import VentiCore

// Pilots among each other, on the screens: the follow button and the
// counts on a pilot's page, the feed of the pilots one follows, the
// cheers and the comments of a public flight, and what the pilot was
// told. Every line, count and refusal is the core's (`social.rs`); these
// views lay them out and hold no rule.

/// On a pilot's page, over the totals: how many follow them and whom they
/// follow, and for a signed-in pilot on another pilot's page the button
/// that follows them. Nothing until Venti has said how it stands.
struct FollowSection: View {
    let handle: String
    @Binding var follows: FollowList?

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var isSending = false
    @State private var problem: String?

    var body: some View {
        if let follows {
            let social = Social(account: account, wind: wind)
            Section {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: VentiSpace.s3) {
                        counts(of: follows)
                        Spacer(minLength: 0)
                        button(for: follows, social)
                    }
                    VStack(alignment: .leading, spacing: VentiSpace.s2) {
                        counts(of: follows)
                        button(for: follows, social)
                    }
                }
            } footer: {
                if let problem {
                    Text(problem)
                        .foregroundStyle(.venti(.danger))
                        .accessibilityIdentifier("pilot-follow-problem")
                }
            }
        }
    }

    private func counts(of follows: FollowList) -> some View {
        Text(verbatim: "\(follows.followersLine) · \(follows.followingLine)")
            .foregroundStyle(.venti(.textSecondary))
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityIdentifier("pilot-follows")
    }

    /// The button, for a signed-in pilot on another pilot's page.
    @ViewBuilder
    private func button(for follows: FollowList, _ social: Social) -> some View {
        if social.isSignedIn, social.ownHandle != handle,
           let following = follows.youFollow, let title = follows.button
        {
            Button(title) {
                press(following: following, social)
            }
            .buttonStyle(.venti(following ? .quiet : .primary))
            .disabled(isSending)
            .accessibilityIdentifier("pilot-follow")
        }
    }

    private func press(following: Bool, _ social: Social) {
        guard !isSending else { return }
        isSending = true
        problem = nil
        Task {
            defer { isSending = false }
            do {
                let state = try await social.follow(handle, !following)
                follows?.youFollow = state.following
                follows?.button = state.button
                follows?.followers = state.followers
                follows?.followersLine = state.followersLine
            } catch {
                problem = Social.words(for: error)
            }
        }
    }
}

/// The public flights of the pilots the signed-in pilot follows, the
/// newest first: the row of everyone's flights, and under it the core's
/// lines of who cheered and how many wrote.
struct FollowingFeed: View {
    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var rows: [FeedRow] = []
    @State private var next: String?
    @State private var isLoading = true
    @State private var problem: String?
    /// What the core says in place of an empty feed.
    @State private var emptyLine = ""

    var body: some View {
        VentiList {
            Section {
                ForEach(rows, id: \.flight.id) { row in
                    NavigationLink(value: row.flight) {
                        FeedRowView(row: row)
                    }
                    .accessibilityIdentifier("feed-row")
                    .onAppear {
                        // The last row in view: fetch what follows it.
                        if row.flight.id == rows.last?.flight.id, next != nil, !isLoading {
                            Task {
                                await load(fromTheTop: false)
                            }
                        }
                    }
                }
                if isLoading {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
        }
        .listStyle(.insetGrouped)
        .overlay {
            if rows.isEmpty, !isLoading {
                if let problem {
                    // Something went wrong: said plainly, without art.
                    ContentUnavailableView {
                        Label {
                            Text(Social.words.feedFailed)
                        } icon: {
                            Image("icon-caution")
                        }
                    } description: {
                        Text(problem)
                    }
                    .accessibilityIdentifier("feed-problem")
                } else {
                    VentiEmpty(title: Social.words.feedFollowing, text: emptyLine)
                        .accessibilityIdentifier("feed-empty")
                }
            }
        }
        .refreshable {
            await load(fromTheTop: true)
        }
        .task {
            await load(fromTheTop: true)
        }
    }

    private func load(fromTheTop: Bool) async {
        isLoading = true
        defer { isLoading = false }
        do {
            let page = try await Social(account: account, wind: wind).feed(before: fromTheTop ? nil : next)
            rows = fromTheTop ? page.rows : rows + page.rows
            next = page.next
            emptyLine = page.emptyLine
            problem = nil
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            problem = Social.words(for: error)
        }
    }
}

/// A flight of the feed: the public flight's row, and what pilots made of
/// it in a line.
private struct FeedRowView: View {
    let row: FeedRow

    private var made: String {
        [row.cheersLine, row.commentsLine].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// The line stands in the column of the words, under the date.
    var body: some View {
        PublicFlightRow(flight: row.flight, note: made)
    }
}

/// Under the head of a public flight's page, floating over its map: the
/// cheer button for a signed-in pilot on another pilot's flight, the
/// core's line of who cheered, and the entry that opens the comments.
/// Nothing of it shows until Venti has said how the flight stands.
struct FlightSocialBar: View {
    let flight: PublicFlight

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var cheers: CheerList?
    @State private var comments: CommentList?
    @State private var problem: String?
    @State private var isSending = false
    @State private var showsComments = false

    private var social: Social {
        Social(account: account, wind: wind)
    }

    var body: some View {
        let social = social
        VStack(spacing: 0) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: VentiSpace.s2) {
                    cheerButton(social)
                    cheersLine(social)
                    commentsEntry
                }
                VStack(spacing: 0) {
                    HStack(spacing: VentiSpace.s2) {
                        cheerButton(social)
                        commentsEntry
                    }
                    cheersLine(social)
                }
            }
            if let problem {
                SocialPill(words: problem)
                    .accessibilityIdentifier("flight-cheer-problem")
            }
        }
        .task(id: "\(flight.id) \(social.isSignedIn)") {
            let social = self.social
            cheers = try? await social.cheers(flight.id)
            comments = try? await social.comments(flight.id, of: flight.handle)
        }
        .sheet(isPresented: $showsComments) {
            if let comments {
                CommentsSheet(
                    flight: flight,
                    list: Binding { self.comments ?? comments } set: { self.comments = $0 }
                )
                .presentationDetents([.medium, .large])
            }
        }
    }

    /// Whether the pilot may cheer: signed in, and the flight another's.
    private func cheered(_ social: Social) -> Bool? {
        guard social.isSignedIn, social.ownHandle != flight.handle else { return nil }
        return cheers?.cheered
    }

    @ViewBuilder
    private func cheerButton(_ social: Social) -> some View {
        if let cheers, let cheered = cheered(social) {
            Button {
                press(cheered: cheered, social)
            } label: {
                SocialPill(words: cheers.button, isOn: cheered)
            }
            .buttonStyle(.plain)
            .disabled(isSending)
            .accessibilityAddTraits(cheered ? .isSelected : [])
            .accessibilityIdentifier("flight-cheer")
        }
    }

    /// Who cheered, in the core's line. Beside the button, a line of the
    /// pilot's own cheer alone ("You cheered") would only say again what
    /// "Cheered" says.
    @ViewBuilder
    private func cheersLine(_ social: Social) -> some View {
        if let cheers, !cheers.line.isEmpty, !(cheered(social) == true && cheers.count == 1) {
            SocialPill(words: cheers.line)
                .accessibilityIdentifier("flight-cheers-line")
        }
    }

    @ViewBuilder private var commentsEntry: some View {
        if let comments {
            Button {
                showsComments = true
            } label: {
                SocialPill(words: comments.countLine.isEmpty ? Social.words.commentsTitle : comments.countLine)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("flight-comments")
        }
    }

    private func press(cheered: Bool, _ social: Social) {
        guard !isSending else { return }
        isSending = true
        problem = nil
        Task {
            defer { isSending = false }
            do {
                let state = try await social.cheer(flight.id, !cheered)
                cheers?.cheered = state.cheered
                cheers?.button = state.button
                cheers?.count = state.cheers
                cheers?.line = state.line
            } catch {
                problem = Social.words(for: error)
            }
        }
    }
}

/// A pill floating over a map: the chip's look, on the primary ground
/// while `isOn`. 36 high as the pack draws a chip, 48 to the finger.
private struct SocialPill: View {
    let words: String
    var isOn = false

    var body: some View {
        Text(words)
            .font(.ventiType(.label))
            .lineLimit(1)
            .padding(.horizontal, 14)
            .frame(height: 36)
            .foregroundStyle(.venti(isOn ? .onPrimary : .text))
            .ventiFloating(in: .capsule, ground: isOn ? .primary : .surface)
            .frame(minHeight: VentiSize.touch)
            .contentShape(.rect)
    }
}

/// The comments under a public flight, the oldest first, and for a
/// signed-in pilot the field that writes one. A comment is plain text:
/// shown as text, never as markup, and no link is made of it.
struct CommentsSheet: View {
    let flight: PublicFlight
    @Binding var list: CommentList

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(\.dismiss) private var dismiss

    @State private var text = ""
    @State private var isSending = false
    @State private var problem: String?
    @State private var reporting: Reported?
    /// The comment the pilot is asked about before it is removed.
    @State private var removing: CommentRow?

    private var social: Social {
        Social(account: account, wind: wind)
    }

    var body: some View {
        let social = social
        let words = Social.words
        NavigationStack {
            VentiList {
                if !list.comments.isEmpty {
                    Section {
                        ForEach(list.comments, id: \.id) { comment in
                            CommentView(
                                comment: comment,
                                // Another pilot's comment, for a pilot who
                                // has an account to report from.
                                canReport: social.isSignedIn && comment.handle != social.ownHandle,
                                isEnabled: !isSending,
                                onRemove: { removing = comment },
                                onReport: { reporting = .comment(comment.id, under: flight.id) }
                            )
                        }
                    } header: {
                        Text(list.countLine)
                            .textCase(nil)
                            .accessibilityIdentifier("comments-count")
                    }
                }
                if let closed = list.closedLine {
                    Section {
                        Text(closed)
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityIdentifier("comments-closed")
                    }
                } else if social.isSignedIn {
                    let draft = commentDraft(text: text)
                    Section {
                        // The button beside the field, so that the
                        // keyboard never lies over it.
                        HStack(alignment: .bottom, spacing: VentiSpace.s3) {
                            TextField(words.commentPlaceholder, text: $text, axis: .vertical)
                                .lineLimit(1...6)
                                .accessibilityIdentifier("comment-text")
                            Button(words.commentSend) {
                                post(draft, social)
                            }
                            .buttonStyle(.borderless)
                            .font(.ventiType(.label))
                            .disabled(!draft.canSend || isSending)
                            .accessibilityIdentifier("comment-send")
                        }
                        .frame(minHeight: VentiSize.touch)
                    } footer: {
                        VStack(alignment: .leading, spacing: VentiSpace.s2) {
                            if let problem {
                                Text(problem)
                                    .foregroundStyle(.venti(.danger))
                                    .accessibilityIdentifier("comment-problem")
                            }
                            Text(draft.fault ?? draft.counter)
                                .foregroundStyle(.venti(draft.fault == nil ? .textSecondary : .danger))
                                .monospacedDigit()
                                .accessibilityIdentifier("comment-counter")
                            Text(draft.notice)
                                .accessibilityIdentifier("comment-notice")
                        }
                    }
                } else if let problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                    }
                }
            }
            .navigationTitle(words.commentsTitle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                    .accessibilityIdentifier("comments-close")
                }
            }
            .sheet(item: $reporting) { reported in
                ReportSheet(reported: reported)
            }
            // Asked first, in the core's words: a removed comment is gone.
            .confirmationDialog(
                words.commentRemoveQuestion,
                isPresented: Binding { removing != nil } set: { isShown in
                    if !isShown {
                        removing = nil
                    }
                },
                titleVisibility: .visible,
                presenting: removing
            ) { comment in
                Button(words.commentRemove, role: .destructive) {
                    remove(comment, social)
                }
                .accessibilityIdentifier("comment-remove-confirm")
                Button(commonWords().cancel, role: .cancel) {}
            }
        }
    }

    private func post(_ draft: CommentDraft, _ social: Social) {
        guard draft.canSend, !isSending else { return }
        isSending = true
        problem = nil
        Task {
            defer { isSending = false }
            do {
                let posted = try await social.post(draft.text, under: flight.id)
                list.comments.append(posted.comment)
                list.count = posted.count
                list.countLine = posted.countLine
                text = ""
            } catch {
                problem = Social.words(for: error)
            }
        }
    }

    private func remove(_ comment: CommentRow, _ social: Social) {
        guard !isSending else { return }
        isSending = true
        problem = nil
        Task {
            defer { isSending = false }
            do {
                try await social.remove(comment.id, under: flight.id)
                // The count and its line are the core's: read anew.
                list = try await social.comments(flight.id, of: flight.handle)
            } catch {
                problem = Social.words(for: error)
            }
        }
    }
}

/// One comment: who wrote it and when, what it says, and what the pilot
/// may do with it.
private struct CommentView: View {
    let comment: CommentRow
    let canReport: Bool
    let isEnabled: Bool
    let onRemove: () -> Void
    let onReport: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            Text(verbatim: comment.byline)
                .font(.venti(.footnote))
                .foregroundStyle(.venti(.textSecondary))
            Text(verbatim: comment.text)
                .fixedSize(horizontal: false, vertical: true)
            if comment.canRemove || canReport {
                HStack(spacing: VentiSpace.s4) {
                    if comment.canRemove {
                        Button(Social.words.commentRemove, action: onRemove)
                            .foregroundStyle(.venti(.danger))
                            .accessibilityIdentifier("comment-remove")
                    }
                    if canReport {
                        Button(Social.words.commentReport, action: onReport)
                            .accessibilityIdentifier("comment-report")
                    }
                }
                .font(.ventiType(.label))
                // Two buttons in one row of a list: each its own.
                .buttonStyle(.borderless)
                .frame(minHeight: VentiSize.touch)
                .disabled(!isEnabled)
            }
        }
        // A group with its parts: an identifier on the stack alone would
        // be handed to the buttons in it, in place of their own.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("comment")
    }
}

/// On the pilot's own public flight, in the sheet that says who may see
/// it: whether other pilots can write under it. The switch's name and the
/// line under it are the core's; how it stands is read from the flight's
/// comments when the sheet shows.
struct CommentsSwitchSection: View {
    /// The flight's address on Venti.
    let flight: String

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var open: Bool?
    @State private var problem: String?
    @State private var isSending = false

    private var social: Social {
        Social(account: account, wind: wind)
    }

    private var stands: Binding<Bool> {
        Binding {
            open ?? true
        } set: { wanted in
            guard !isSending else { return }
            isSending = true
            problem = nil
            Task {
                defer { isSending = false }
                do {
                    open = try await social.commentsOpen(flight, wanted).open
                } catch {
                    problem = Social.words(for: error)
                }
            }
        }
    }

    var body: some View {
        // The section stands from the start, so that the question hangs
        // on something; its row comes with the answer.
        Section {
            if let open {
                Toggle(commentsSwitch(open: open).title, isOn: stands)
                    .disabled(isSending)
                    .accessibilityIdentifier("share-comments")
            }
        } footer: {
            if let open {
                Text(problem ?? commentsSwitch(open: open).line)
                    .accessibilityIdentifier("share-comments-line")
            }
        }
        .task(id: flight) {
            let social = social
            open = (try? await social.comments(flight, of: social.ownHandle))?.open
        }
    }
}

/// "Notifications", a page of the account: what the pilot was told. A
/// line about a flight opens that flight, one about a follower that
/// pilot's page. No badge and no push: it is read when the page shows.
struct NotificationsScreen: View {
    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var told: Notifications?
    @State private var problem: String?
    @State private var isLoading = true

    var body: some View {
        VentiList {
            if let told {
                Section {
                    // The newest on top; the core hands them over the
                    // oldest first.
                    ForEach(Array(told.lines.reversed().enumerated()), id: \.offset) { _, line in
                        NavigationLink {
                            if let flight = line.flightId {
                                PublicFlightByAddress(id: flight)
                            } else {
                                PilotFlightsScreen(pilot: PilotRoute(handle: line.handle, name: "@\(line.handle)"))
                            }
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(verbatim: line.line)
                                Text(line.age)
                                    .font(.venti(.footnote))
                                    .foregroundStyle(.venti(.textSecondary))
                            }
                        }
                        .accessibilityIdentifier("notification")
                    }
                }
            }
            if isLoading, told == nil {
                ProgressView()
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
            }
        }
        .listStyle(.insetGrouped)
        .overlay {
            if !isLoading, told?.lines.isEmpty ?? true {
                if let problem {
                    ContentUnavailableView {
                        Label {
                            Text(Social.words.notificationsFailed)
                        } icon: {
                            Image("icon-caution")
                        }
                    } description: {
                        Text(problem)
                    }
                    .accessibilityIdentifier("notifications-problem")
                } else if let told {
                    VentiEmpty(title: Social.words.notificationsTitle, text: told.emptyLine)
                        .accessibilityIdentifier("notifications-empty")
                }
            }
        }
        .navigationTitle(Social.words.notificationsTitle)
        .navigationBarTitleDisplayMode(.inline)
        .refreshable {
            await load()
        }
        .task {
            await load()
        }
    }

    private func load() async {
        isLoading = true
        defer { isLoading = false }
        do {
            told = try await Social(account: account, wind: wind).notifications()
            problem = nil
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            problem = Social.words(for: error)
        }
    }
}
