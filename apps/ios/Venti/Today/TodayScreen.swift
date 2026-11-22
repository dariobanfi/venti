import SwiftUI
import VentiCore

/// The Today tab, as the design language's Today screen has it: the day in
/// a line over the valley, and under it the take-offs around with what the
/// forecast says about flying from each. The take-off that looks best is
/// a card with its readings, its thermals and its hours; the others are
/// rows with their status and the kind of day, the pilot's own first.
/// Which take-offs these are, their order and which one looks best are
/// the core's.
///
/// What a forecast says about a take-off is the core's reading of a weather
/// model, and the screen says so at its foot: the model, when the server
/// last looked for a newer run, and that the take-off itself decides.
struct TodayScreen: View {
    @Environment(TodayModel.self) private var today
    @Environment(WindModel.self) private var wind
    @Environment(KeptSitesModel.self) private var kept
    @Environment(AccountModel.self) private var account

    /// Which of the forecast's days is shown: 0 is today.
    @State private var day = 0
    /// Whether the pilot has chosen the day. Until then an evening shows
    /// tomorrow, since today has no flying hours left.
    @State private var hasChosenDay = false
    @State private var position = PilotPosition()
    @State private var isShowingSettings = false
    @State private var isShowingAssistant = false
    /// The core's offer to add a place the pilot took off at and no site
    /// names, among the places around the take-offs listed: one at most.
    @State private var offer: CandidateOffer?
    /// The core's line once the place was added from here.
    @State private var offerSaid: String?
    /// Foehn, as the core reads Venti's pressures across the Alps: a line
    /// over the take-offs only while the core has one, and the view
    /// behind it.
    @State private var foehn = FoehnModel()
    @State private var isShowingFoehn = false

    /// Where the take-offs are looked for until the pilot's position is
    /// known: the middle of the first region, between the Tegernsee and
    /// the Achensee.
    static let home = Coordinate(lat: 47.6, lon: 11.75)

    private var place: Coordinate {
        position.place ?? Self.home
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                // The core's order for the chosen day, asked once for
                // all that is drawn from it.
                let shown = arrangement
                VStack(spacing: 0) {
                    header(shown)
                    content(shown)
                        .padding(.horizontal, VentiSpace.s4)
                        .padding(.vertical, VentiSpace.s4)
                }
            }
            .background(.venti(.bg))
            .ignoresSafeArea(.container, edges: .top)
            .refreshable {
                await today.load(around: place)
                await askForFoehn(anew: true)
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Assistant", systemImage: "sparkles") {
                        isShowingAssistant = true
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Settings", systemImage: "gearshape") {
                        isShowingSettings = true
                    }
                }
            }
            .navigationDestination(for: SiteView.self) { site in
                // The page speaks of the day chosen here.
                SiteScreen(
                    site: site, credits: today.listed?.credits, asOfUnixS: today.sitesAsOfUnixS,
                    dayStartUnixS: days.indices.contains(day) ? days[day].startUnixS : nil
                )
            }
            // The settings and the assistant are pages of this tab: the
            // tab bar stays, and they keep their place while the pilot
            // looks at another tab.
            .navigationDestination(isPresented: $isShowingSettings) {
                SettingsScreen()
            }
            .navigationDestination(isPresented: $isShowingAssistant) {
                AssistantScreen()
            }
        }
        // Asked when the tab is first shown, when the pilot's position
        // arrives, when the server's address changes, when the pilot has
        // kept or forgotten a site, and again after a while.
        .task(id: Asked(place: place, server: wind.server, kept: kept.changes)) {
            position.readIfAllowed()
            if !today.isFresh(for: place) {
                await today.load(around: place)
            }
        }
        // The offer, asked for when the take-offs listed or the account
        // change.
        .task(id: OfferAsked(
            ids: today.places.map(\.id), server: wind.server, account: account.isSignedIn,
            takes: wind.takesSitesFromPilots
        )) {
            await askForAnOffer()
        }
        // Foehn, asked for when Today has its take-offs, whose places the
        // core's line is about, and when the server's address changes.
        .task(id: FoehnAsked(ids: today.places.map(\.id), server: wind.server)) {
            if !today.places.isEmpty {
                await askForFoehn(anew: false)
            }
        }
        .sheet(isPresented: $isShowingFoehn) {
            FoehnSheet(model: foehn) {
                Task { await askForFoehn(anew: true) }
            }
        }
        .onChange(of: today.places) {
            guard !hasChosenDay else { return }
            // The first day that has flying hours left: in the evening
            // that is tomorrow.
            day = days.firstIndex { !$0.hours.isEmpty } ?? 0
        }
    }

    /// What decides that foehn is read again.
    private struct FoehnAsked: Equatable {
        let ids: [String]
        let server: URL?
    }

    /// Asks Venti for foehn and has the core read it for the take-offs
    /// listed, by Venti's clock: the server's time where it was read, 0
    /// where not, never the iPhone's.
    private func askForFoehn(anew: Bool) async {
        await foehn.load(
            from: wind.server, places: today.places.map(\.site.position),
            now: { await wind.serverNowIfRead() }, anew: anew
        )
    }

    /// What decides that the offer is asked for again.
    private struct OfferAsked: Equatable {
        let ids: [String]
        let server: URL?
        let account: Bool
        let takes: Bool
    }

    /// The box around the take-offs listed, with a little room around.
    static func box(around places: [Coordinate]) -> Bounds? {
        guard let first = places.first else { return nil }
        var box = Bounds(south: first.lat, west: first.lon, north: first.lat, east: first.lon)
        for place in places {
            box.south = min(box.south, place.lat)
            box.north = max(box.north, place.lat)
            box.west = min(box.west, place.lon)
            box.east = max(box.east, place.lon)
        }
        let margin = 0.05
        return Bounds(
            south: box.south - margin, west: box.west - margin, north: box.north + margin, east: box.east + margin
        )
    }

    /// Asks Venti for the places around the take-offs listed and the core
    /// for its offer among them. Nothing where the core gives no path for
    /// the box (wider than Venti answers for) or offers nothing.
    private func askForAnOffer() async {
        guard let server = wind.server, let box = Self.box(around: today.places.map(\.site.position)),
              let answer = try? await CandidatesModel.fetch(box, from: server, transport: WindModel.fetch)
        else {
            offer = nil
            return
        }
        offer = await account.candidateOffers(answer, takesSitesFromPilots: wind.takesSitesFromPilots).first
    }

    /// What decides that the server is asked again.
    private struct Asked: Equatable {
        let place: Coordinate
        let server: URL?
        let kept: Int
    }

    /// The take-offs as the screen lays them out on the chosen day.
    struct Arrangement {
        /// The take-off that looks best, on the card; before any has a
        /// forecast for the day, the first of the list.
        var card: TodayModel.Place?
        /// The pilot's own take-offs and the others, as rows in the
        /// core's order, without the one on the card.
        var kept: [TodayModel.Place] = []
        var near: [TodayModel.Place] = []
        /// Whether the two groups are named: where both have take-offs.
        var namesGroups = false
    }

    /// The core's order of the take-offs on the chosen day
    /// (`todayOrder`), laid out: the best on the card, the others in
    /// their two groups.
    private var arrangement: Arrangement {
        Self.arrange(today.places) { outlookDay(of: $0) }
    }

    static func arrange(
        _ places: [TodayModel.Place], dayOf: (TodayModel.Place) -> OutlookDay?
    ) -> Arrangement {
        let order = todayOrder(
            takeoffs: places.map { place in
                let day = dayOf(place)
                return TodayStanding(
                    id: place.id, isKept: place.isKept, distanceM: place.distanceM,
                    verdict: day?.verdict, thermals: day?.thermals
                )
            }
        )
        let ordered = order.ids.compactMap { id in places.first { $0.id == id } }
        let card = ordered.first { $0.id == order.best } ?? ordered.first
        let rows = ordered.filter { $0.id != card?.id }
        return Arrangement(
            card: card,
            kept: rows.filter(\.isKept),
            near: rows.filter { !$0.isKept },
            namesGroups: places.contains(where: \.isKept) && places.contains { !$0.isKept }
        )
    }

    /// What a take-off's forecast says of the chosen day: the day that
    /// begins at the same moment, wherever it stands in that forecast.
    private func outlookDay(of place: TodayModel.Place) -> OutlookDay? {
        guard days.indices.contains(day) else { return nil }
        let chosen = days[day].startUnixS
        return place.outlook?.days.first { $0.startUnixS == chosen }
    }

    /// What stands in place of a day's line for a take-off that has none:
    /// that the server is still being asked, or that its forecast does
    /// not reach the day.
    static func waitingWords(for place: TodayModel.Place) -> String {
        place.isAsked ? "No forecast for this day" : "Reading the forecast"
    }

    /// As wide as the segmented control has to be for every name to show
    /// whole: the widest name in each segment, with the control's padding.
    /// Not drawn and not read out.
    private func widestRow(_ days: [OutlookDay]) -> some View {
        let widest = days.map(\.name).max { $0.count < $1.count } ?? ""
        return HStack(spacing: 0) {
            ForEach(days.indices, id: \.self) { _ in
                Text(widest)
                    .font(.ventiType(.label))
                    .lineLimit(1)
                    .fixedSize()
                    .padding(.horizontal, VentiSpace.s1)
            }
        }
        .padding(3)
        .hidden()
        .accessibilityHidden(true)
    }

    /// The pack's segmented control over these days.
    private func daySwitch(_ days: [OutlookDay]) -> some View {
        VentiSegmented(
            options: days.enumerated().map { (value: $0.offset, name: $0.element.name) },
            selection: Binding(
                get: { day },
                set: { chosen in
                    hasChosenDay = true
                    day = chosen
                }
            )
        )
    }

    /// The days the forecasts reach over, four at most: as many as the
    /// pack's segmented control gives each its share of the width.
    private var days: [OutlookDay] {
        Array((today.places.compactMap(\.outlook).map(\.days).max { $0.count < $1.count } ?? []).prefix(4))
    }

    // MARK: The head

    /// The valley, with the day and its line. The art is the design
    /// pack's and is light in both appearances, so its words are in the
    /// pack's ink whatever the iPhone is set to.
    private func header(_ shown: Arrangement) -> some View {
        // The art fills a frame as wide as the screen and is cut at its
        // edges: laid out by itself it would be as wide as it is drawn,
        // and the page with it.
        Color.clear
            .frame(height: 230)
            .frame(maxWidth: .infinity)
            .overlay {
                Image("art-valley-strip")
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            }
            .clipped()
            .accessibilityHidden(true)
            .overlay(alignment: .top) {
                VStack(spacing: 2) {
                    Text(dayLine)
                        .font(.ventiType(.label))
                    Text(headline(shown))
                        .font(.ventiType(.hero))
                        .multilineTextAlignment(.center)
                        .minimumScaleFactor(0.7)
                        .lineLimit(2)
                        .accessibilityIdentifier("today-headline")
                    if let kind = kindOfDay(shown) {
                        Text(kind)
                            .font(.ventiType(.label))
                            .accessibilityIdentifier("today-kind-of-day")
                    }
                }
                .foregroundStyle(VentiBrand.ink)
                .padding(.horizontal, VentiSpace.s4)
                .padding(.top, 76)
            }
    }

    /// "Saturday, near you", or the day alone while the take-offs are
    /// those around the middle of the map.
    private var dayLine: String {
        let start = days.indices.contains(day)
            ? Date(timeIntervalSince1970: TimeInterval(days[day].startUnixS)) : Date()
        let name = start.formatted(.dateTime.weekday(.wide))
        return position.place == nil ? name : "\(name), near you"
    }

    /// The day in a line: the best take-off's. Before there is one, what
    /// the screen is waiting for.
    private func headline(_ shown: Arrangement) -> String {
        switch today.state {
        case .failed: return "No forecast"
        case .idle, .loading: return "Reading the forecast"
        case .loaded:
            if today.places.isEmpty {
                return "No take-offs near"
            }
            guard let best = shown.card else { return "Reading the forecast" }
            return outlookDay(of: best)?.headline
                ?? (today.places.allSatisfy(\.isAsked) ? "No forecast" : "Reading the forecast")
        }
    }

    /// The kind of day the best take-off's thermals make, "Cross-country
    /// day", where the forecast has an estimate.
    private func kindOfDay(_ shown: Arrangement) -> String? {
        guard case .loaded = today.state, let best = shown.card,
              let words = outlookDay(of: best)?.thermalWords, !words.isEmpty
        else { return nil }
        return words
    }

    // MARK: The list

    @ViewBuilder private func content(_ shown: Arrangement) -> some View {
        switch today.state {
        case .failed(let message):
            Explained(title: "The forecast could not be read", words: message, action: "Try again") {
                Task { await today.load(around: place) }
            }
        case .idle, .loading:
            ProgressView()
                .padding(.top, VentiSpace.s6)
        case .loaded:
            if today.places.isEmpty {
                Explained(
                    title: "No take-offs within 30 km",
                    words: position.place == nil
                        ? "These would be the take-offs around the Tegernsee. Venti knows none there yet."
                        : "Venti knows no take-off around where you are.",
                    action: nil
                ) {}
            } else {
                VStack(alignment: .leading, spacing: VentiSpace.s4) {
                    if days.count > 1 {
                        // Four days where their names fit whole, as at
                        // the usual text sizes; three where a larger text
                        // would cut "Tomorrow" short.
                        ViewThatFits(in: .horizontal) {
                            // The segments share the width equally, so what
                            // has to fit is the widest name as often as
                            // there are days: a row of it, unseen, beside.
                            ZStack {
                                widestRow(days)
                                daySwitch(days)
                            }
                            daySwitch(Array(days.prefix(3)))
                        }
                        .accessibilityIdentifier("today-days")
                    }
                    if let problem = today.problem {
                        // Venti gave no take-offs: the pilot's own are
                        // listed, from the iPhone.
                        Text(problem)
                            .font(.ventiType(.caption))
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityIdentifier("today-problem")
                    }
                    // Foehn, in the core's one line, only while it has
                    // one: no line is not "no foehn", and nothing stands
                    // in its place.
                    if let line = foehn.line {
                        FoehnTodayLine(line: line) {
                            isShowingFoehn = true
                        }
                    }
                    let words = today.listed
                    let cardIsKept = shown.card?.isKept ?? false
                    if shown.namesGroups, cardIsKept, let words {
                        GroupTitle(words: words.keptTitle)
                            .accessibilityIdentifier("today-kept-title")
                    }
                    if let first = shown.card {
                        NavigationLink(value: first.site) {
                            TakeoffCard(place: first, day: outlookDay(of: first))
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("today-card")
                    }
                    if !shown.kept.isEmpty || !(words?.keptMore ?? "").isEmpty {
                        VStack(alignment: .leading, spacing: 0) {
                            if shown.namesGroups, !cardIsKept, !shown.kept.isEmpty, let words {
                                GroupTitle(words: words.keptTitle)
                                    .accessibilityIdentifier("today-kept-title")
                            }
                            rows(shown.kept)
                            if let more = words?.keptMore, !more.isEmpty {
                                Text(more)
                                    .font(.ventiType(.caption))
                                    .foregroundStyle(.venti(.textSecondary))
                                    .padding(.top, VentiSpace.s2)
                                    .accessibilityIdentifier("today-kept-more")
                            }
                        }
                    }
                    if !shown.near.isEmpty {
                        VStack(alignment: .leading, spacing: 0) {
                            if shown.namesGroups, let words {
                                GroupTitle(words: words.nearTitle)
                                    .accessibilityIdentifier("today-near-title")
                            }
                            rows(shown.near)
                        }
                    }
                    // A place the pilot took off at and no site names, as
                    // the core offers it: under the take-offs.
                    if let offer {
                        CandidateOfferView(
                            offer: offer,
                            onAdded: { written in
                                offerSaid = written.words
                                self.offer = nil
                            },
                            onSetAside: {
                                self.offer = nil
                            }
                        )
                        .ventiCard()
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("today-candidate-offer")
                    } else if let offerSaid {
                        Label(offerSaid, systemImage: "checkmark.circle")
                            .font(.ventiType(.caption))
                            .foregroundStyle(.venti(.textSecondary))
                    }
                    if position.place == nil, !position.isDenied {
                        Button {
                            position.ask()
                        } label: {
                            Text("Use my position")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.venti(.quiet))
                        .accessibilityHint("Shows the take-offs around you")
                    }
                    foot(shown)
                    // The way to the foehn view, whether there is a line
                    // or not: the page's last row, in small print.
                    FoehnFootRow(title: foehn.title) {
                        isShowingFoehn = true
                    }
                }
            }
        }
    }

    /// What the reading is, whose data it is, and how fresh: the design
    /// language asks a forecast for its update time and its source.
    private func foot(_ shown: Arrangement) -> some View {
        let outlook = today.places.compactMap(\.outlook).first
        // The chosen day's own model, where the best take-off has the day:
        // a day after the finest model's run ends is a coarser one's.
        let read = shown.card.flatMap { outlookDay(of: $0) }
        // Whose data the forecasts are, and whose the take-offs: each
        // source once.
        let sources = today.credits + (today.listed?.credits ?? []).filter { site in
            !today.credits.contains { $0.source == site.source }
        }
        // What the wind near by is and is not, while some take-off's day
        // has its line: the note of the first that has. The core names the
        // squares it read for that take-off.
        let nearbyNote = today.places
            .first { !(outlookDay(of: $0)?.nearby ?? "").isEmpty }?.outlook?.nearbyNote ?? ""
        return VStack(alignment: .leading, spacing: VentiSpace.s2) {
            if let outlook, !outlook.note.isEmpty {
                let note = read.map(\.note).flatMap { $0.isEmpty ? nil : $0 } ?? outlook.note
                let run = read.map(\.runUnixS).flatMap { $0 == 0 ? nil : $0 } ?? outlook.runUnixS
                Text(
                    "\(note) Run of \(Format.time(unixSeconds: run)), looked up at \(Format.time(unixSeconds: outlook.fetchedUnixS))."
                )
            }
            if !nearbyNote.isEmpty {
                Text(nearbyNote)
                    .accessibilityIdentifier("today-nearby-note")
            }
            Text("Flyable, marginal and not flyable are what the forecast's wind, gusts, rain and cloud say by fixed limits, and the kind of day is what its heat and temperatures say of thermals. They are not advice to fly.")
            ForEach(sources, id: \.source) { credit in
                Text(credit.text)
            }
        }
        .font(.ventiType(.caption))
        .foregroundStyle(.venti(.textSecondary))
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityIdentifier("today-foot")
    }
}

extension TodayScreen {
    /// Take-offs as rows, with a line between two.
    @ViewBuilder fileprivate func rows(_ places: [TodayModel.Place]) -> some View {
        ForEach(places) { place in
            NavigationLink(value: place.site) {
                TakeoffRow(place: place, day: outlookDay(of: place))
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("today-row")
            if place.id != places.last?.id {
                Rectangle()
                    .fill(.venti(.border))
                    .frame(height: 1)
            }
        }
    }
}

/// What a group of take-offs is called, in the core's words: "Your
/// take-offs", "Within 30 km".
private struct GroupTitle: View {
    let words: String

    var body: some View {
        Text(words)
            .font(.ventiType(.label))
            .foregroundStyle(.venti(.textSecondary))
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityAddTraits(.isHeader)
    }
}

/// A state of the screen in words, with the one thing to do about it,
/// under the pack's empty valley.
private struct Explained: View {
    let title: String
    let words: String
    let action: String?
    let act: () -> Void

    var body: some View {
        VStack(spacing: VentiSpace.s3) {
            Text(title)
                .font(.ventiType(.heading))
            Text(words)
                .multilineTextAlignment(.center)
                .foregroundStyle(.venti(.textSecondary))
            if let action {
                Button(action: act) {
                    Text(action)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.primary))
            }
        }
        .padding(.top, VentiSpace.s5)
        .frame(maxWidth: .infinity)
    }
}

extension VentiStatus {
    /// The pack's status for what a forecast says about a take-off.
    init(_ day: OutlookDay) {
        let kind: Kind =
            switch day.verdict {
            case .flyable: .good
            case .marginal: .caution
            case .notFlyable: .danger
            }
        self.init(kind: kind, word: day.status)
    }
}

/// The first take-off: its status, three readings, and its hours.
private struct TakeoffCard: View {
    let place: TodayModel.Place
    let day: OutlookDay?

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            HStack(alignment: .firstTextBaseline) {
                Text(place.site.name)
                    .font(.ventiType(.heading))
                    .lineLimit(2)
                Spacer(minLength: VentiSpace.s2)
                if let day {
                    VentiStatus(day)
                }
            }
            if let day {
                if !day.why.isEmpty {
                    Text(day.why)
                        .foregroundStyle(.venti(.textSecondary))
                }
                // The wind near by where it is much stronger: a note under
                // the day's own reason, and no part of the status.
                NearbyLine(words: day.nearbyShort)
                if day.thermals != nil {
                    ThermalLine(day: day)
                }
                HStack(alignment: .top, spacing: VentiSpace.s2) {
                    Metric(
                        caption: day.windFrom.isEmpty ? "Wind" : "Wind from \(day.windFrom)",
                        value: day.windKmh.isEmpty ? "– –" : day.windKmh, unit: "km/h"
                    )
                    Metric(caption: "Gusts", value: day.gustKmh.isEmpty ? "– –" : day.gustKmh, unit: "km/h")
                    Metric(
                        caption: "Cloud base",
                        value: day.cloudBaseM.map(Format.metres) ?? "– –", unit: "m"
                    )
                }
                if !day.hours.isEmpty {
                    HourStrip(hours: day.hours)
                    Text(
                        HourStrip.showsThermals(day.hours)
                            ? todayWords().hourStripNoteWithThermals
                            : todayWords().hourStripNote
                    )
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textSecondary))
                }
            } else {
                Text(TodayScreen.waitingWords(for: place))
                    .foregroundStyle(.venti(.textSecondary))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .ventiCard()
        .contentShape(.rect)
    }
}

/// Another take-off: its name, what limits its day, and its status.
private struct TakeoffRow: View {
    let place: TodayModel.Place
    let day: OutlookDay?

    var body: some View {
        HStack(spacing: VentiSpace.s3) {
            Image("icon-site")
                .resizable()
                .frame(width: VentiSize.icon, height: VentiSize.icon)
            VStack(alignment: .leading, spacing: 2) {
                Text(place.site.name)
                    .font(.ventiType(.heading))
                    .lineLimit(2)
                Text(detail)
                    .font(.ventiType(.label))
                    .foregroundStyle(.venti(.textSecondary))
                    .lineLimit(2)
                NearbyLine(words: day?.nearbyShort ?? "")
                if let day, !day.thermalWords.isEmpty {
                    HStack(spacing: VentiSpace.s1) {
                        ThermalMark(climbMs: day.climbMs ?? 0)
                        Text(day.thermalWords)
                            .font(.ventiType(.label))
                            .lineLimit(1)
                    }
                }
            }
            Spacer(minLength: VentiSpace.s2)
            if let day {
                VentiStatus(day)
            }
        }
        .padding(.vertical, VentiSpace.s3)
        .frame(minHeight: VentiSize.touch)
        .contentShape(.rect)
    }

    /// What limits the day; for a day nothing limits, its line.
    private var detail: String {
        guard let day else { return TodayScreen.waitingWords(for: place) }
        return day.why.isEmpty ? day.headline : day.why
    }
}

/// The day's hours side by side: the hour, where the wind goes, its
/// speed, and a bar that is the hour's status by its height and its
/// colour. Where the forecast has an estimate of the thermals, the climb
/// to expect stands under it, with a bar in the thermal scale's colour
/// that grows with the climb.
private struct HourStrip: View {
    let hours: [OutlookHour]

    /// Whether any of the hours has thermals to climb in. A day without
    /// any says so in its line, and a row of noughts adds nothing to that.
    static func showsThermals(_ hours: [OutlookHour]) -> Bool {
        hours.contains { ($0.climbMs ?? 0) > 0 }
    }

    var body: some View {
        let showsThermals = Self.showsThermals(hours)
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(hours, id: \.timeUnixS) { hour in
                VStack(spacing: VentiSpace.s1) {
                    Text(hour.hour)
                        .font(.ventiType(.caption))
                        .foregroundStyle(.venti(.textSecondary))
                    if let from = hour.windFromDeg {
                        Image("icon-arrow-wind")
                            .resizable()
                            .frame(width: 14, height: 14)
                            // The arrow points where the wind goes.
                            .rotationEffect(.degrees(from + 180))
                    } else {
                        Color.clear
                            .frame(width: 14, height: 14)
                    }
                    Text(hour.windKmh.isEmpty ? "–" : hour.windKmh)
                        .font(.venti(.subheadline, .semibold))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                    Rectangle()
                        .fill(.venti(Self.role(hour.verdict)))
                        .frame(height: Self.height(hour.verdict))
                        .frame(height: 12, alignment: .bottom)
                    if showsThermals {
                        let band = ThermalBand(metresPerSecond: hour.climbMs ?? 0)
                        Text(hour.climbMs.map { Format.decimal($0, places: 1) } ?? "–")
                            .font(.venti(.subheadline, .semibold))
                            .monospacedDigit()
                            .lineLimit(1)
                            .minimumScaleFactor(0.6)
                            .padding(.top, VentiSpace.s1)
                        Rectangle()
                            .fill(band.color)
                            .frame(height: Self.height(band))
                            .frame(height: 12, alignment: .bottom)
                    }
                }
                .frame(maxWidth: .infinity)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(hour.hour) o'clock")
                .accessibilityValue(Self.spoken(hour))
            }
        }
    }

    static func role(_ verdict: Flyability) -> VentiColor.Role {
        switch verdict {
        case .flyable: .good
        case .marginal: .caution
        case .notFlyable: .danger
        }
    }

    /// Tall for flyable, low for not: the bar says it without its colour.
    static func height(_ verdict: Flyability) -> CGFloat {
        switch verdict {
        case .flyable: 12
        case .marginal: 7
        case .notFlyable: 3
        }
    }

    /// Taller for a stronger climb: two points for none to climb in, up
    /// to twelve for the scale's strongest step.
    static func height(_ band: ThermalBand) -> CGFloat {
        CGFloat(2 + 2 * band.step)
    }

    static func spoken(_ hour: OutlookHour) -> String {
        let verdict =
            switch hour.verdict {
            case .flyable: "Flyable"
            case .marginal: "Marginal"
            case .notFlyable: "Not flyable"
            }
        let wind = hour.windKmh.isEmpty ? "" : ", wind \(hour.windKmh) kilometres an hour"
        var thermals = ""
        if let climb = hour.climbMs {
            thermals = climb > 0
                ? ", thermals \(Format.decimal(climb, places: 1)) metres per second"
                : ", no thermals to climb in"
            if climb > 0, let top = hour.thermalTopM {
                thermals += " to \(Format.metres(top)) metres"
            }
        }
        // What is much stronger near by in the hour, in the core's line:
        // the strip has no room for it, a screen reader has.
        let line = hour.nearby?.line ?? ""
        let nearby = line.isEmpty ? "" : ". \(line)"
        return hour.why.isEmpty
            ? "\(verdict)\(wind)\(thermals)\(nearby)"
            : "\(verdict): \(hour.why)\(wind)\(thermals)\(nearby)"
    }
}
