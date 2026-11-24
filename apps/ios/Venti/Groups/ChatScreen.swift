import PhotosUI
import SwiftUI
import VentiCore

/// A group's chat: the messages, the newest at the bottom, older pages as
/// the pilot scrolls up, what is new every five seconds while the screen
/// is open; the field to write in with up to four photos; links to
/// Venti's flights, sites, open plans and pilots shown as cards that open
/// them. A long press on a message removes it (its author, the group's
/// owner and admins) or reports it.
struct GroupChatScreen: View {
    let id: String
    let name: String

    @Environment(GroupsModel.self) private var groups
    @Environment(UnreadModel.self) private var unread

    @State private var chat: ChatModel?
    @State private var detail: CommunityDetail?
    @State private var isJoining = false
    @State private var joinProblem: String?
    @State private var isAddingPlan = false
    @State private var reporting: Reported?
    @State private var actionProblem: String?

    private var words: CommunityWords { groups.words }

    var body: some View {
        Group {
            if let chat {
                messages(chat)
            } else {
                GroupsLoading()
            }
        }
        .background(.venti(.bg))
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let chat, let detail {
                if detail.canPost {
                    ChatComposer(chat: chat)
                } else if detail.canJoin {
                    joinBar
                }
            }
        }
        .navigationTitle(detail?.group.name ?? name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if detail?.canPost == true {
                ToolbarItem(placement: .primaryAction) {
                    Button("New plan", systemImage: "calendar.badge.plus") {
                        isAddingPlan = true
                    }
                    .accessibilityIdentifier("chat-new-plan")
                }
            }
            ToolbarItem(placement: .primaryAction) {
                NavigationLink {
                    GroupPage(id: id)
                } label: {
                    Label("About the group", systemImage: "info.circle")
                }
                .accessibilityIdentifier("chat-info")
            }
        }
        .sheet(isPresented: $isAddingPlan) {
            PlanForm(editing: nil, inGroup: id)
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        .task(id: id) {
            let chat = chat ?? groups.chat(id)
            self.chat = chat
            async let read: Void = loadDetail()
            // The board, for the plan form's groups.
            async let board: Void = groups.load()
            // Runs until the screen goes, which cancels it.
            await chat.run()
            _ = await (read, board)
        }
        // While the chat is in front, a push of its own messages shows
        // no banner over it (`AlertNotices.Centre`).
        .onAppear {
            AlertNotices.taps.chatInFront = id
        }
        .onDisappear {
            if AlertNotices.taps.chatInFront == id {
                AlertNotices.taps.chatInFront = nil
            }
            Task {
                await unread.read()
                await groups.loadChats()
            }
        }
    }

    private func messages(_ chat: ChatModel) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: VentiSpace.s3) {
                    if chat.log.older != nil {
                        ProgressView()
                            .frame(maxWidth: .infinity, minHeight: VentiSize.touch)
                            .onAppear {
                                Task { await chat.loadOlder() }
                            }
                            .accessibilityIdentifier("chat-older")
                    }
                    if chat.hasLoaded, chat.log.messages.isEmpty {
                        GroupsLine(words: words.emptyChat)
                            .frame(maxWidth: .infinity, alignment: .center)
                            .padding(.vertical, VentiSpace.s6)
                            .accessibilityIdentifier("chat-empty")
                    }
                    ForEach(chat.log.messages, id: \.id) { message in
                        MessageRow(
                            message: message, groupId: id,
                            canRemove: message.removedWords == nil && (message.isMine || detail?.canModerate == true),
                            remove: { remove(message, in: chat) },
                            report: { reporting = $0 }
                        )
                        .id(message.id)
                        .onAppear {
                            if detail?.canPost == true {
                                chat.saw(message)
                            }
                        }
                    }
                    if let problem = chat.problem ?? actionProblem {
                        GroupsLine(words: problem, isProblem: true)
                            .accessibilityIdentifier("chat-problem")
                    }
                }
                .padding(.horizontal, VentiSpace.s4)
                .padding(.vertical, VentiSpace.s3)
            }
            .defaultScrollAnchor(.bottom)
            .scrollDismissesKeyboard(.interactively)
            .onChange(of: chat.log.messages.last?.id) { _, last in
                guard let last else { return }
                withAnimation {
                    proxy.scrollTo(last, anchor: .bottom)
                }
            }
            .overlay {
                if !chat.hasLoaded, chat.problem == nil {
                    GroupsLoading()
                }
            }
        }
    }

    private var joinBar: some View {
        VStack(spacing: VentiSpace.s2) {
            if let joinProblem {
                GroupsLine(words: joinProblem, isProblem: true)
            }
            Button {
                join()
            } label: {
                Text(words.join).frame(maxWidth: .infinity)
            }
            .buttonStyle(.venti(.primary))
            .disabled(isJoining)
            .accessibilityIdentifier("chat-join")
        }
        .padding(VentiSpace.s3)
        .background(.venti(.surface))
        .overlay(alignment: .top) {
            Rectangle().fill(.venti(.border)).frame(height: 1)
        }
    }

    private func loadDetail() async {
        detail = try? await groups.detail(id)
    }

    private func join() {
        isJoining = true
        joinProblem = nil
        Task {
            defer { isJoining = false }
            do {
                try await groups.enter(id)
                await loadDetail()
                // Joining counts what was there as read.
                await chat?.loadNewest()
            } catch {
                joinProblem = AccountModel.words(for: error)
            }
        }
    }

    private func remove(_ message: ChatMessage, in chat: ChatModel) {
        actionProblem = nil
        Task {
            do {
                try await chat.remove(message)
            } catch {
                actionProblem = AccountModel.words(for: error)
            }
        }
    }
}

// MARK: A message

/// One message: who wrote it and when, its text, its pictures, its cards
/// and, for a plan of the group, the plan with "I'm in". The pilot's own
/// stand at the right on the primary ground.
struct MessageRow: View {
    let message: ChatMessage
    let groupId: String
    let canRemove: Bool
    let remove: () -> Void
    let report: (Reported) -> Void

    @Environment(AccountModel.self) private var account
    @State private var isConfirmingRemoval = false

    private var when: String {
        Self.unixSeconds(message.postedAt).map { Format.dayAndTime(unixSeconds: $0) } ?? ""
    }

    var body: some View {
        let mine = message.isMine
        VStack(alignment: mine ? .trailing : .leading, spacing: VentiSpace.s1) {
            Text(mine ? when : "\(message.authorName) · \(when)")
                .font(.venti(.caption))
                .foregroundStyle(.venti(.textSecondary))
            if let removed = message.removedWords {
                Text(removed)
                    .italic()
                    .foregroundStyle(.venti(.textMuted))
                    .padding(.horizontal, VentiSpace.s3)
                    .padding(.vertical, VentiSpace.s2)
                    .overlay {
                        RoundedRectangle(cornerRadius: VentiRadius.m).strokeBorder(.venti(.border), lineWidth: 1)
                    }
            } else {
                if !message.text.isEmpty {
                    Text(verbatim: message.text)
                        .foregroundStyle(.venti(mine ? .onPrimary : .text))
                        .textSelection(.enabled)
                        .padding(.horizontal, VentiSpace.s3)
                        .padding(.vertical, VentiSpace.s2)
                        .background(.venti(mine ? .primary : .surface), in: .rect(cornerRadius: VentiRadius.m))
                        .overlay {
                            if !mine {
                                RoundedRectangle(cornerRadius: VentiRadius.m).strokeBorder(.venti(.border), lineWidth: 1)
                            }
                        }
                }
                if !message.pictures.isEmpty {
                    ChatPictures(paths: message.pictures)
                }
                ForEach(Array(message.cards.enumerated()), id: \.offset) { _, card in
                    ChatCard(card: card)
                }
                if let plan = message.plan {
                    PlanCard(plan: plan, myName: account.account?.displayName)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: mine ? .trailing : .leading)
        .padding(mine ? .leading : .trailing, VentiSpace.s6)
        .contentShape(.rect)
        .contextMenu {
            if canRemove {
                Button("Remove", systemImage: "trash", role: .destructive) {
                    isConfirmingRemoval = true
                }
                .accessibilityIdentifier("message-remove")
            }
            if !mine, message.removedWords == nil {
                Button("Report this message", systemImage: "exclamationmark.bubble") {
                    report(.message(message.id, in: groupId))
                }
                .accessibilityIdentifier("message-report")
                ForEach(Array(message.pictureIds.enumerated()), id: \.offset) { index, picture in
                    Button(
                        message.pictureIds.count == 1 ? "Report the picture" : "Report picture \(index + 1)",
                        systemImage: "photo"
                    ) {
                        report(.messagePicture(picture, in: groupId))
                    }
                }
            }
        }
        .confirmationDialog("Remove this message?", isPresented: $isConfirmingRemoval, titleVisibility: .visible) {
            Button("Remove", role: .destructive, action: remove)
        } message: {
            Text("It keeps its place in the chat, without its words, pictures and links.")
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-message")
    }

    /// The seconds since 1970 of an RFC 3339 time, as the server writes
    /// them.
    nonisolated static func unixSeconds(_ text: String) -> Int64? {
        let parser = ISO8601DateFormatter()
        parser.formatOptions = [.withInternetDateTime]
        if let date = parser.date(from: text) {
            return Int64(date.timeIntervalSince1970)
        }
        parser.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return parser.date(from: text).map { Int64($0.timeIntervalSince1970) }
    }
}

/// A message's pictures, side by side two at a time.
private struct ChatPictures: View {
    let paths: [String]

    @Environment(WindModel.self) private var wind
    @State private var shown: ShownPicture?

    var body: some View {
        let columns = [GridItem(.flexible(), spacing: VentiSpace.s2), GridItem(.flexible(), spacing: VentiSpace.s2)]
        LazyVGrid(columns: paths.count == 1 ? [GridItem(.flexible())] : columns, spacing: VentiSpace.s2) {
            ForEach(paths, id: \.self) { path in
                let url = PictureAddress.url(path, on: wind.server)
                Button {
                    shown = url.map(ShownPicture.init)
                } label: {
                    AsyncImage(url: url) { phase in
                        if let image = phase.image {
                            image.resizable().scaledToFill()
                        } else {
                            Rectangle().fill(.venti(.surfaceRaised))
                        }
                    }
                    .frame(minHeight: VentiSpace.s8 * 2, maxHeight: VentiSpace.s8 * 2)
                    .frame(maxWidth: .infinity)
                    .clipShape(.rect(cornerRadius: VentiRadius.m))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Picture")
                .accessibilityIdentifier("chat-picture")
            }
        }
        .frame(maxWidth: .infinity)
        .sheet(item: $shown) { shown in
            ChatPictureSheet(url: shown.url)
        }
    }
}

/// A picture opened on its own.
private struct ShownPicture: Identifiable {
    let url: URL
    var id: String { url.absoluteString }
}

/// A picture of a chat on its own, as large as the screen.
private struct ChatPictureSheet: View {
    let url: URL

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            AsyncImage(url: url) { phase in
                if let image = phase.image {
                    image.resizable().scaledToFit()
                } else {
                    ProgressView()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(.venti(.bg))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) { dismiss() }
                }
            }
        }
    }
}

// MARK: Cards

/// A link of a message as a card that opens what it names: a flight with
/// its pilot, take-off, distance, day and track drawn small; a site; an
/// open plan with "I'm in"; a pilot. What is gone says so.
struct ChatCard: View {
    let card: CardView

    @Environment(OpenPlansModel.self) private var openPlans
    @Environment(GroupsModel.self) private var groups
    /// "I'm in" as the pilot last said it here, until the chat reads the
    /// card anew.
    @State private var amIn: Bool?
    @State private var isSending = false

    var body: some View {
        switch card.kind {
        case "flight":
            NavigationLink {
                PublicFlightByAddress(id: card.id)
            } label: {
                lines(identifier: "chat-card-flight") {
                    TrackThumbnail(outline: card.track)
                }
            }
            .buttonStyle(.plain)
        case "site":
            NavigationLink {
                SiteByID(id: card.id, name: card.title)
            } label: {
                lines(identifier: "chat-card-site") {
                    sign("icon-site")
                }
            }
            .buttonStyle(.plain)
        case "open_plan":
            VStack(alignment: .leading, spacing: VentiSpace.s2) {
                NavigationLink {
                    OpenPlanScreen(id: card.id)
                } label: {
                    lines(identifier: "chat-card-open-plan") {
                        sign("icon-takeoff")
                    }
                }
                .buttonStyle(.plain)
                let isIn = amIn ?? card.amIn
                Button {
                    toggle(isIn)
                } label: {
                    Label {
                        Text(groups.words.imIn)
                    } icon: {
                        if isIn {
                            ChipTick()
                        }
                    }
                }
                .buttonStyle(.venti(isIn ? .primary : .quiet))
                .disabled(isSending)
                .accessibilityAddTraits(isIn ? .isSelected : [])
                .accessibilityIdentifier("chat-card-in")
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        case "pilot":
            NavigationLink {
                PilotFlightsScreen(pilot: PilotRoute(handle: card.id, name: card.title))
            } label: {
                lines(identifier: "chat-card-pilot") {
                    PilotFace(name: card.title, picturePath: card.picturePath, size: VentiSpace.s7)
                }
            }
            .buttonStyle(.plain)
        default:
            Text(card.title)
                .font(.venti(.footnote))
                .foregroundStyle(.venti(.textSecondary))
                .padding(VentiSpace.s3)
                .overlay {
                    RoundedRectangle(cornerRadius: VentiRadius.m).strokeBorder(.venti(.border), lineWidth: 1)
                }
                .accessibilityIdentifier("chat-card-gone")
        }
    }

    private func sign(_ name: String) -> some View {
        Image(name)
            .resizable()
            .frame(width: VentiSize.icon, height: VentiSize.icon)
            .foregroundStyle(.venti(.textMuted))
            .frame(width: VentiSpace.s7, height: VentiSpace.s7)
            .background(.venti(.surfaceRaised), in: .rect(cornerRadius: VentiRadius.s))
            .accessibilityHidden(true)
    }

    private func lines(identifier: String, @ViewBuilder leading: () -> some View) -> some View {
        HStack(spacing: VentiSpace.s3) {
            leading()
            VStack(alignment: .leading, spacing: 2) {
                Text(card.title)
                    .font(.venti(.body, .semibold))
                    .foregroundStyle(.venti(.text))
                Text(card.subtitle)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
            }
            .multilineTextAlignment(.leading)
            Spacer(minLength: 0)
            Image(systemName: "chevron.forward")
                .font(.venti(.footnote, .semibold))
                .foregroundStyle(.venti(.textMuted))
                .accessibilityHidden(true)
        }
        .padding(VentiSpace.s3)
        .frame(maxWidth: .infinity, minHeight: VentiSize.touch, alignment: .leading)
        .background(.venti(.surface), in: .rect(cornerRadius: VentiRadius.m))
        .overlay {
            RoundedRectangle(cornerRadius: VentiRadius.m).strokeBorder(.venti(.border), lineWidth: 1)
        }
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(identifier)
    }

    private func toggle(_ isIn: Bool) {
        guard !isSending else { return }
        isSending = true
        Task {
            defer { isSending = false }
            if let plan = try? await openPlans.setIn(card.id, !isIn) {
                amIn = plan.amIn
            }
        }
    }
}

/// A site named by its identifier, as a card of a chat names it: read from
/// Venti, then shown as any site is.
struct SiteByID: View {
    let id: String
    let name: String

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var page: SitePage?
    @State private var problem: String?

    var body: some View {
        Group {
            if let page {
                SiteScreen(site: page.site, credits: page.credit.map { [$0] })
            } else if let problem {
                ContentUnavailableView {
                    Label {
                        Text(name)
                    } icon: {
                        Image("icon-caution")
                    }
                } description: {
                    Text(problem)
                }
            } else {
                ProgressView()
            }
        }
        .task(id: id) {
            do {
                page = try await SiteEditing(account: account, wind: wind).page(id)
            } catch {
                problem = SiteEditing.words(for: error)
            }
        }
    }
}

// MARK: Writing

/// The field a message is written in, the photos chosen for it (four at
/// most, sent one by one before the message) and "Send".
struct ChatComposer: View {
    let chat: ChatModel

    @Environment(GroupsModel.self) private var groups

    @State private var text = ""
    @State private var items: [PhotosPickerItem] = []
    /// The chosen photos, each made a JPEG of at most 1,000 pixels.
    @State private var photos: [Data] = []
    @State private var isSending = false
    @State private var problem: String?

    private static let mostPictures = 4

    private var canSend: Bool {
        !isSending && (!text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !photos.isEmpty)
    }

    /// How many links of the text become cards, in a line.
    private var linksLine: String? {
        let count = messageLinks(text: text).count
        return switch count {
        case 0: nil
        case 1: "The link shows as a card."
        default: "\(count) links show as cards."
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            if let problem {
                GroupsLine(words: problem, isProblem: true)
                    .accessibilityIdentifier("chat-send-problem")
            }
            if !photos.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: VentiSpace.s2) {
                        ForEach(Array(photos.enumerated()), id: \.offset) { index, photo in
                            if let image = UIImage(data: photo) {
                                Image(uiImage: image)
                                    .resizable()
                                    .scaledToFill()
                                    .frame(width: VentiSpace.s8, height: VentiSpace.s8)
                                    .clipShape(.rect(cornerRadius: VentiRadius.s))
                                    .overlay(alignment: .topTrailing) {
                                        Button("Take the photo out", systemImage: "xmark.circle.fill") {
                                            photos.remove(at: index)
                                        }
                                        .labelStyle(.iconOnly)
                                        .foregroundStyle(.venti(.onPrimary), .venti(.primary))
                                        .frame(width: VentiSize.touch, height: VentiSize.touch, alignment: .topTrailing)
                                    }
                            }
                        }
                    }
                }
            }
            if let linksLine {
                Text(linksLine)
                    .font(.venti(.caption))
                    .foregroundStyle(.venti(.textSecondary))
            }
            HStack(alignment: .bottom, spacing: VentiSpace.s2) {
                PhotosPicker(
                    selection: $items, maxSelectionCount: max(1, Self.mostPictures - photos.count),
                    matching: .images
                ) {
                    Image(systemName: "photo.on.rectangle")
                        .font(.venti(.body, .semibold))
                        .foregroundStyle(.venti(.link))
                        .frame(width: VentiSize.touch, height: VentiSize.touch)
                }
                .disabled(photos.count >= Self.mostPictures || isSending)
                .accessibilityLabel("Add photos")
                .accessibilityIdentifier("chat-photos")
                TextField(groups.words.messagePlaceholder, text: $text, axis: .vertical)
                    .lineLimit(1...5)
                    .padding(.horizontal, VentiSpace.s3)
                    .padding(.vertical, VentiSpace.s2)
                    .frame(minHeight: VentiSize.touch)
                    .background(.venti(.bg), in: .rect(cornerRadius: VentiRadius.m))
                    .overlay {
                        RoundedRectangle(cornerRadius: VentiRadius.m).strokeBorder(.venti(.border), lineWidth: 1)
                    }
                    .accessibilityIdentifier("chat-text")
                Button {
                    send()
                } label: {
                    Image(systemName: "arrow.up")
                        .font(.venti(.body, .semibold))
                }
                .buttonStyle(.venti(.primary))
                .frame(minWidth: VentiSize.touch, minHeight: VentiSize.touch)
                .disabled(!canSend)
                .accessibilityLabel(groups.words.send)
                .accessibilityIdentifier("chat-send")
            }
        }
        .padding(.horizontal, VentiSpace.s3)
        .padding(.vertical, VentiSpace.s2)
        .background(.venti(.surface))
        .overlay(alignment: .top) {
            Rectangle().fill(.venti(.border)).frame(height: 1)
        }
        .onChange(of: items) { _, items in
            guard !items.isEmpty else { return }
            Task {
                for item in items where photos.count < Self.mostPictures {
                    if let data = try? await item.loadTransferable(type: Data.self), let jpeg = PictureUpload.jpeg(data) {
                        photos.append(jpeg)
                    } else {
                        problem = PictureUpload.unreadable
                    }
                }
                self.items = []
            }
        }
    }

    private func send() {
        guard canSend else { return }
        isSending = true
        problem = nil
        let (said, chosen) = (text, photos)
        Task {
            defer { isSending = false }
            do {
                var pictures: [String] = []
                for photo in chosen {
                    pictures.append(try await chat.sendPicture(photo).id)
                }
                try await chat.post(text: said, pictures: pictures)
                text = ""
                photos = []
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}
