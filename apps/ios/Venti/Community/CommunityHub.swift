import SwiftUI
import VentiCore

/// Community: pilots among each other, in one place (docs/plan.md, the
/// owner's testing session of 2026-11-22, item 7), as the web's
/// `/community/` has it. In this order: the plans of the pilot's groups
/// for the coming days, each a card with its day and time large, its
/// place, its group, the faces of who is in and "I am in"; the flights of
/// the pilots they follow, each a card with the track's shape; the pilots
/// near the iPhone's position, or the map's middle where the position is
/// not allowed; and what the pilot was told. The field over it finds a
/// pilot by name, and what it found stands in place of the rest.
///
/// Signed out, the pilots near and finding a pilot are there, with the way
/// to an account for the rest. Every part asks for itself when the page
/// shows and on a pull; nothing is kept.
struct CommunityHub: View {
    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(GroupsModel.self) private var groups
    @Environment(CommunityModel.self) private var community

    @State private var typed = ""
    @State private var search = PilotSearch()

    @State private var feed: Feed?
    @State private var feedProblem: String?
    @State private var told: Notifications?
    @State private var toldProblem: String?
    @State private var near: PilotsNear?
    @State private var nearProblem: String?
    /// Whether the pilots near are of the iPhone's position, or of the
    /// map's middle.
    @State private var nearIsAroundMe = false
    @State private var position = PilotPosition()
    @State private var isAddingPlan = false

    /// How many of the feed's flights and of the lines told stand here;
    /// the rest are a tap away.
    private static let feedCards = 6
    private static let toldLines = 5

    private var social: Social {
        Social(account: account, wind: wind)
    }

    private var isFinding: Bool {
        !typed.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// A plan needs a group to be posted to.
    private var canPlan: Bool {
        social.isSignedIn && !groups.groups.isEmpty
    }

    var body: some View {
        let isSignedIn = social.isSignedIn
        ScrollView {
            VStack(alignment: .leading, spacing: VentiSpace.s5) {
                if isFinding {
                    found
                } else {
                    if isSignedIn {
                        plans
                        following
                    } else {
                        signIn
                    }
                    pilotsNear
                    if isSignedIn {
                        notifications
                    }
                }
            }
            .padding(.horizontal, VentiSpace.s4)
            .padding(.vertical, VentiSpace.s3)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(.venti(.bg))
        .searchable(
            text: $typed, placement: .navigationBarDrawer(displayMode: .always),
            prompt: pilotsWords().searchPlaceholder
        )
        .autocorrectionDisabled()
        .textInputAutocapitalization(.never)
        .onChange(of: typed) { _, typed in
            search.search(typed, in: community)
        }
        .toolbar {
            if canPlan, !isFinding {
                ToolbarItem(placement: .primaryAction) {
                    Button("New plan", systemImage: "plus") {
                        isAddingPlan = true
                    }
                    .accessibilityIdentifier("community-new-plan")
                }
            }
        }
        .sheet(isPresented: $isAddingPlan) {
            PlanForm(editing: nil)
        }
        .refreshable {
            await load(isSignedIn: isSignedIn)
        }
        .task(id: isSignedIn) {
            position.readIfAllowed()
            await load(isSignedIn: isSignedIn)
        }
        // The position came after the map's middle was asked for.
        .onChange(of: position.place) { _, place in
            guard place != nil, !nearIsAroundMe else { return }
            Task { await loadNear() }
        }
    }

    private func load(isSignedIn: Bool) async {
        async let near: Void = loadNear()
        if isSignedIn {
            async let board: Void = groups.load()
            async let feed: Void = loadFeed()
            async let told: Void = loadTold()
            _ = await (board, feed, told)
        } else {
            feed = nil
            told = nil
        }
        await near
    }

    // MARK: Plans

    @ViewBuilder private var plans: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            HubHeader(title: "Plans") {
                if let board = groups.board {
                    NavigationLink {
                        GroupsList()
                    } label: {
                        Text(board.groups.isEmpty ? "Make or join a group" : "Groups")
                    }
                    .accessibilityIdentifier("community-groups")
                }
            }
            if let board = groups.board {
                if let problem = groups.problem {
                    HubLine(words: problem)
                }
                let cards = board.days.flatMap(\.plans)
                if board.groups.isEmpty {
                    HubLine(words: "You are in no group yet. A group is a few pilots who tell each other where they mean to fly; only its members see it.")
                } else if cards.isEmpty {
                    HubLine(words: "Nobody in your groups has posted a plan for today or a later day.")
                        .accessibilityIdentifier("community-plans-none")
                } else {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(alignment: .top, spacing: VentiSpace.s3) {
                            ForEach(cards, id: \.id) { plan in
                                PlanCard(plan: plan, myName: account.account?.displayName)
                                    .containerRelativeFrame(.horizontal) { width, _ in
                                        cards.count == 1 ? width : width * 0.82
                                    }
                            }
                        }
                        .scrollTargetLayout()
                    }
                    .scrollTargetBehavior(.viewAligned)
                    .scrollClipDisabled()
                }
                if board.isCutShort {
                    HubLine(words: "Your groups hold more plans than fit here: the later days are missing.")
                }
            } else if let problem = groups.problem {
                HubProblem(words: problem) {
                    Task { await groups.load() }
                }
            } else {
                HubLoading()
            }
        }
    }

    // MARK: The feed

    @ViewBuilder private var following: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            HubHeader(title: Social.words.feedFollowing) {
                if !(feed?.rows.isEmpty ?? true) {
                    NavigationLink(value: CommunityRoute.following) {
                        Text("All")
                    }
                    .accessibilityIdentifier("community-following-all")
                }
            }
            if let feed {
                if feed.rows.isEmpty {
                    HubLine(words: feed.emptyLine)
                        .accessibilityIdentifier("feed-empty")
                } else {
                    ForEach(feed.rows.prefix(Self.feedCards), id: \.flight.id) { row in
                        NavigationLink(value: row.flight) {
                            FeedCard(row: row)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("feed-card")
                    }
                }
            } else if let feedProblem {
                HubProblem(words: "\(Social.words.feedFailed) \(feedProblem)") {
                    Task { await loadFeed() }
                }
            } else {
                HubLoading()
            }
        }
    }

    private func loadFeed() async {
        do {
            feed = try await social.feed(before: nil)
            feedProblem = nil
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            feedProblem = Social.words(for: error)
        }
    }

    // MARK: Pilots near

    @ViewBuilder private var pilotsNear: some View {
        let words = pictureWords()
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            HubHeader(title: words.nearTitle) {}
            if let near {
                Text(verbatim: "\(near.words) · \(nearIsAroundMe ? "around you" : "around the map's middle")")
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("near-words")
                if near.pilots.isEmpty {
                    HubLine(words: near.empty)
                        .accessibilityIdentifier("near-empty")
                } else {
                    HubRows {
                        ForEach(Array(near.pilots.enumerated()), id: \.element.handle) { index, pilot in
                            if index > 0 {
                                HubDivider()
                            }
                            NavigationLink {
                                PilotFlightsScreen(pilot: PilotRoute(handle: pilot.handle, name: pilot.name))
                            } label: {
                                NearPilotRow(pilot: pilot)
                            }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("near-pilot")
                        }
                    }
                }
            } else if let nearProblem {
                HubProblem(words: nearProblem) {
                    Task { await loadNear() }
                }
            } else {
                HubLoading()
            }
        }
    }

    /// The pilots near the iPhone, where it may say where it is; else near
    /// the middle of the map as the map tab last showed it.
    private func loadNear() async {
        let mine = position.isAllowed ? position.place : nil
        let place = mine ?? wind.mapCentre
        do {
            near = try await community.pilotsNear(place)
            nearIsAroundMe = mine != nil
            nearProblem = nil
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            nearProblem = error is CoreError ? error.words : pilotsWords().unreachable
        }
    }

    // MARK: Notifications

    @ViewBuilder private var notifications: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            HubHeader(title: Social.words.notificationsTitle) {
                if !(told?.lines.isEmpty ?? true) {
                    NavigationLink {
                        NotificationsScreen()
                    } label: {
                        Text("All")
                    }
                    .accessibilityIdentifier("community-notifications-all")
                }
            }
            if let told {
                if told.lines.isEmpty {
                    HubLine(words: told.emptyLine)
                } else {
                    // The newest on top; the core hands them over the
                    // oldest first.
                    let newest = Array(told.lines.reversed().prefix(Self.toldLines))
                    HubRows {
                        ForEach(Array(newest.enumerated()), id: \.offset) { index, line in
                            if index > 0 {
                                HubDivider()
                            }
                            NavigationLink {
                                if let flight = line.flightId {
                                    PublicFlightByAddress(id: flight)
                                } else {
                                    PilotFlightsScreen(pilot: PilotRoute(handle: line.handle, name: "@\(line.handle)"))
                                }
                            } label: {
                                HStack(spacing: VentiSpace.s3) {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(verbatim: line.line)
                                            .foregroundStyle(.venti(.text))
                                            .multilineTextAlignment(.leading)
                                        Text(line.age)
                                            .font(.venti(.footnote))
                                            .foregroundStyle(.venti(.textSecondary))
                                    }
                                    Spacer(minLength: 0)
                                    HubChevron()
                                }
                                .padding(.horizontal, VentiSpace.s4)
                                .padding(.vertical, VentiSpace.s3)
                                .frame(minHeight: VentiSize.touch)
                                .contentShape(.rect)
                            }
                            .buttonStyle(.plain)
                            .accessibilityElement(children: .combine)
                            .accessibilityIdentifier("community-notification")
                        }
                    }
                }
            } else if let toldProblem {
                HubProblem(words: "\(Social.words.notificationsFailed) \(toldProblem)") {
                    Task { await loadTold() }
                }
            } else {
                HubLoading()
            }
        }
    }

    private func loadTold() async {
        do {
            told = try await social.notifications()
            toldProblem = nil
        } catch is CancellationError {
        } catch {
            if (error as? URLError)?.code == .cancelled { return }
            toldProblem = Social.words(for: error)
        }
    }

    // MARK: Signed out, and finding a pilot

    private var signIn: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            Text("With an account you plan flying days with your groups, follow pilots and see what they fly, and hear who cheered your flights.")
                .foregroundStyle(.venti(.text))
                .fixedSize(horizontal: false, vertical: true)
            NavigationLink {
                AccountScreen()
            } label: {
                Text("Sign in")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.venti(.primary))
            .accessibilityIdentifier("community-sign-in")
        }
        .ventiCard()
    }

    /// What the field found: a row for each pilot with their face, which
    /// opens their page, and the core's line about the result.
    @ViewBuilder private var found: some View {
        let pilots = search.found?.pilots ?? []
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            if !pilots.isEmpty {
                HubRows {
                    ForEach(Array(pilots.enumerated()), id: \.element.handle) { index, pilot in
                        if index > 0 {
                            HubDivider()
                        }
                        NavigationLink {
                            PilotFlightsScreen(pilot: PilotRoute(handle: pilot.handle, name: pilot.name))
                        } label: {
                            HStack(spacing: VentiSpace.s3) {
                                PilotFace(name: pilot.name, picturePath: pilot.picturePath, size: VentiSpace.s6)
                                Text(pilot.words)
                                    .foregroundStyle(.venti(.text))
                                    .multilineTextAlignment(.leading)
                                Spacer(minLength: 0)
                                HubChevron()
                            }
                            .padding(.horizontal, VentiSpace.s4)
                            .padding(.vertical, VentiSpace.s2)
                            .frame(minHeight: VentiSize.touch)
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("found-pilot")
                    }
                }
            }
            if let words = search.words ?? search.found?.words {
                Text(words)
                    .font(.venti(pilots.isEmpty ? .body : .footnote))
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("pilots-found-words")
            }
            if search.isSearching {
                HubLoading()
            }
        }
    }
}

// MARK: The cards

/// One plan as a card: its day and time at the top, the time large; the
/// place with the pack's site sign; its group; the faces of who is in
/// with the core's sentence; and "I am in", pressed while the pilot is
/// in. The upper part opens the plan's page, where everything else is.
struct PlanCard: View {
    let plan: PlanView
    /// The name the pilot of this iPhone is shown by, for their face.
    let myName: String?

    @Environment(GroupsModel.self) private var groups
    @ScaledMetric(relativeTo: .body) private var sign = VentiSpace.s5

    @State private var isSending = false
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            NavigationLink {
                PlanScreen(id: plan.id)
            } label: {
                VStack(alignment: .leading, spacing: VentiSpace.s2) {
                    HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                        Text(PlanWords.title(ofDay: plan.day))
                            .font(.venti(.subheadline, .semibold))
                            .foregroundStyle(.venti(.link))
                        Spacer(minLength: 0)
                        if let time = plan.meetAt {
                            Text(time)
                                .font(.ventiType(.instrumentM))
                                .foregroundStyle(.venti(.text))
                        }
                    }
                    HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                        Image("icon-site")
                            .resizable()
                            .frame(width: sign, height: sign)
                            .foregroundStyle(.venti(.textMuted))
                            .alignmentGuide(.firstTextBaseline) { $0[.bottom] * 0.8 }
                            .accessibilityHidden(true)
                        Text(plan.place)
                            .font(.ventiType(.heading))
                            .foregroundStyle(.venti(.text))
                            .multilineTextAlignment(.leading)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if !plan.groupName.isEmpty {
                        Text(plan.groupName)
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textSecondary))
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("plan-card")

            HStack(spacing: VentiSpace.s2) {
                if !plan.pilotsIn.isEmpty {
                    PlanFaces(names: plan.pilotsIn, firstIsMe: plan.amIn, myName: myName)
                }
                Text(plan.inWords)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
            }
            .accessibilityElement(children: .combine)

            Button {
                toggle()
            } label: {
                Label {
                    Text("I am in")
                } icon: {
                    if plan.amIn {
                        Image(systemName: "checkmark")
                    }
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.venti(plan.amIn ? .primary : .quiet))
            .overlay {
                if !plan.amIn {
                    Capsule().strokeBorder(.venti(.borderStrong), lineWidth: 1)
                }
            }
            .disabled(isSending)
            .accessibilityAddTraits(plan.amIn ? .isSelected : [])
            // Which plan it is, for a test among several cards.
            .accessibilityValue(plan.place)
            .accessibilityIdentifier("plan-card-in")

            if let problem {
                Text(problem)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.danger))
            }
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .ventiCard()
    }

    private func toggle() {
        guard !isSending else { return }
        isSending = true
        problem = nil
        Task {
            defer { isSending = false }
            do {
                try await groups.setIn(plan, !plan.amIn)
            } catch {
                problem = AccountModel.words(for: error)
            }
        }
    }
}

/// One flight of the feed as a card, made to be told apart at a glance
/// as a row of the logbook is: the track's shape on a little map of its
/// own, the pilot with their face and flag, where they took off and on
/// which day, the airtime and the distance large, and the core's lines of
/// cheers and comments. The whole card opens the flight.
struct FeedCard: View {
    let row: FeedRow

    private var flight: PublicFlight { row.flight }

    private var made: String {
        [row.cheersLine, row.commentsLine].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            HStack(alignment: .top, spacing: VentiSpace.s3) {
                TrackThumbnail(outline: flight.outline, large: true)
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    HStack(spacing: VentiSpace.s2) {
                        PilotFace(name: flight.pilot, picturePath: PublicFlightRow.picturePath(of: flight))
                        Text(flight.pilot)
                            .font(.venti(.headline))
                            .foregroundStyle(.venti(.text))
                            .lineLimit(1)
                        Flag(code: flight.pilotCountry)
                    }
                    if let place = PublicFlightRow.place(of: flight) {
                        Text(place)
                            .font(.venti(.body, .semibold))
                            .foregroundStyle(.venti(.text))
                            .fixedSize(horizontal: false, vertical: true)
                            .accessibilityLabel(
                                [flight.takeoffName, flight.takeoffCountry.map(Countries.name)]
                                    .compactMap { $0 }.joined(separator: ", ")
                            )
                    }
                    Text(Format.dayShort(unixSeconds: flight.startUnixS))
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.textSecondary))
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s5) {
                number(Format.hoursMinutes(seconds: flight.airtimeS), unit: "h", spoken: "Airtime")
                HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s1) {
                    if let score = flight.score, score.kind != .freeFlight {
                        Image("icon-triangle")
                            .resizable()
                            .frame(width: VentiSpace.s4, height: VentiSpace.s4)
                            .foregroundStyle(.venti(.textMuted))
                            .accessibilityLabel(score.kind == .faiTriangle ? "FAI triangle" : "Triangle")
                    }
                    number(
                        Format.kilometres(metres: flight.score?.distanceM ?? flight.trackLengthM),
                        unit: "km", spoken: "Distance"
                    )
                }
                Spacer(minLength: 0)
            }
            if !made.isEmpty {
                Text(made)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .ventiCard()
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
    }

    private func number(_ value: String, unit: String, spoken: String) -> some View {
        Text("\(Text(value).font(.ventiType(.instrumentM)))\(Text(" \(unit)").font(.venti(.subheadline, .medium)).foregroundStyle(.venti(.textSecondary)))")
            .foregroundStyle(.venti(.text))
            .lineLimit(1)
            .accessibilityLabel("\(spoken) \(value) \(unit)")
    }
}

/// A pilot near the place: their face, their name, the core's line of
/// their flights from here and the take-off they fly most, and the
/// season's totals in the core's words.
struct NearPilotRow: View {
    let pilot: NearPilotLine

    var body: some View {
        HStack(spacing: VentiSpace.s3) {
            PilotFace(
                name: pilot.name, picturePath: pilot.picturePath, size: VentiSpace.s7,
                initials: pilot.initials
            )
            VStack(alignment: .leading, spacing: 2) {
                Text(pilot.name)
                    .font(.venti(.headline))
                    .foregroundStyle(.venti(.text))
                Text(pilot.words)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
                if let season = pilot.season, !season.words.isEmpty {
                    Text(season.words)
                        .font(.venti(.footnote))
                        .foregroundStyle(.venti(.textSecondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .multilineTextAlignment(.leading)
            Spacer(minLength: 0)
            HubChevron()
        }
        .padding(.horizontal, VentiSpace.s4)
        .padding(.vertical, VentiSpace.s3)
        .frame(minHeight: VentiSize.touch)
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
    }
}

// MARK: The page's parts

/// A part's title, with what leads on from it at its right.
private struct HubHeader<Trailing: View>: View {
    let title: String
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s3) {
            Text(title)
                .font(.ventiType(.heading))
                .foregroundStyle(.venti(.text))
                .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 0)
            trailing
                .font(.venti(.subheadline, .medium))
                .foregroundStyle(.venti(.link))
                .frame(minHeight: VentiSize.touch)
        }
    }
}

/// Rows on one card, as a list's section draws them.
private struct HubRows<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(spacing: 0) {
            content
        }
        .background(.venti(.surface), in: .rect(cornerRadius: VentiRadius.m))
        .overlay {
            RoundedRectangle(cornerRadius: VentiRadius.m)
                .strokeBorder(.venti(.border), lineWidth: 1)
        }
    }
}

private struct HubDivider: View {
    var body: some View {
        Rectangle()
            .fill(.venti(.border))
            .frame(height: 1)
            .padding(.leading, VentiSpace.s4)
    }
}

private struct HubChevron: View {
    var body: some View {
        Image(systemName: "chevron.forward")
            .font(.venti(.footnote, .semibold))
            .foregroundStyle(.venti(.textMuted))
            .accessibilityHidden(true)
    }
}

/// A line where a part has nothing to show.
private struct HubLine: View {
    let words: String

    var body: some View {
        Text(words)
            .foregroundStyle(.venti(.textSecondary))
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// Why a part is not there, and "Try again".
private struct HubProblem: View {
    let words: String
    let again: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            Label {
                Text(words)
                    .fixedSize(horizontal: false, vertical: true)
            } icon: {
                Image("icon-caution")
                    .foregroundStyle(.venti(.caution))
            }
            .foregroundStyle(.venti(.textSecondary))
            Button(commonWords().tryAgain, action: again)
                .buttonStyle(.venti(.quiet))
        }
    }
}

private struct HubLoading: View {
    var body: some View {
        ProgressView()
            .frame(maxWidth: .infinity, minHeight: VentiSize.touch)
    }
}
