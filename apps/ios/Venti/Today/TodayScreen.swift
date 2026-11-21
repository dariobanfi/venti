import SwiftUI
import VentiCore

/// The Today tab, as the design language's Today screen has it: the day in
/// a line over the valley, and under it the take-offs around with what the
/// forecast says about flying from each. The nearest take-off with the
/// best outlook is a card with its readings, its thermals and its hours;
/// the others are rows with their status and the kind of day.
///
/// What a forecast says about a take-off is the core's reading of a weather
/// model, and the screen says so at its foot: the model, when the server
/// last looked for a newer run, and that the take-off itself decides.
struct TodayScreen: View {
    @Environment(TodayModel.self) private var today
    @Environment(WindModel.self) private var wind

    /// Which of the forecast's days is shown: 0 is today.
    @State private var day = 0
    /// Whether the pilot has chosen the day. Until then an evening shows
    /// tomorrow, since today has no flying hours left.
    @State private var hasChosenDay = false
    @State private var position = PilotPosition()
    @State private var isShowingSettings = false
    @State private var isShowingAssistant = false

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
                VStack(spacing: 0) {
                    header
                    content
                        .padding(.horizontal, VentiSpace.s4)
                        .padding(.vertical, VentiSpace.s4)
                }
            }
            .background(.venti(.bg))
            .ignoresSafeArea(.container, edges: .top)
            .refreshable {
                await today.load(around: place)
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
                SiteScreen(site: site)
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
        // arrives, when the server's address changes, and again after a
        // while.
        .task(id: Asked(place: place, server: wind.server)) {
            position.readIfAllowed()
            if !today.isFresh(for: place) {
                await today.load(around: place)
            }
        }
        .onChange(of: today.places) {
            guard !hasChosenDay else { return }
            // The first day that has flying hours left: in the evening
            // that is tomorrow.
            day = days.firstIndex { !$0.hours.isEmpty } ?? 0
        }
    }

    /// What decides that the server is asked again.
    private struct Asked: Equatable {
        let place: Coordinate
        let server: URL?
    }

    /// The take-offs in the order they are shown: by what the forecast
    /// says of the chosen day, the better first, among flyable days the
    /// one with the better thermals, and among equals the nearer. One the
    /// server has not answered for yet comes last.
    private var ordered: [TodayModel.Place] {
        today.places.sorted { one, other in
            let ranks = (rank(one), rank(other))
            return ranks.0 == ranks.1 ? one.distanceM < other.distanceM : ranks.0 < ranks.1
        }
    }

    private func rank(_ place: TodayModel.Place) -> Int {
        guard let day = outlookDay(of: place) else { return 6 }
        switch day.verdict {
        case .flyable:
            switch day.thermals {
            case .big: return 0
            case .crossCountry: return 1
            case .local: return 2
            case .weak, nil: return 3
            }
        case .marginal: return 4
        case .notFlyable: return 5
        }
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
    private var header: some View {
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
                    Text(headline)
                        .font(.ventiType(.hero))
                        .multilineTextAlignment(.center)
                        .minimumScaleFactor(0.7)
                        .lineLimit(2)
                        .accessibilityIdentifier("today-headline")
                    if let kind = kindOfDay {
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
    private var headline: String {
        switch today.state {
        case .noServer: return "No forecast yet"
        case .failed: return "No forecast"
        case .idle, .loading: return "Reading the forecast"
        case .loaded:
            if today.places.isEmpty {
                return "No take-offs near"
            }
            guard let best = ordered.first else { return "Reading the forecast" }
            return outlookDay(of: best)?.headline
                ?? (today.places.allSatisfy(\.isAsked) ? "No forecast" : "Reading the forecast")
        }
    }

    /// The kind of day the best take-off's thermals make, "Cross-country
    /// day", where the forecast has an estimate.
    private var kindOfDay: String? {
        guard case .loaded = today.state, let best = ordered.first,
              let words = outlookDay(of: best)?.thermalWords, !words.isEmpty
        else { return nil }
        return words
    }

    // MARK: The list

    @ViewBuilder private var content: some View {
        switch today.state {
        case .noServer:
            Explained(
                title: "Venti is not connected",
                words: "The forecast and the take-offs come from Venti. Set its address in the settings.",
                action: "Open the settings"
            ) {
                isShowingSettings = true
            }
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
                    let shown = ordered
                    if let first = shown.first {
                        NavigationLink(value: first.site) {
                            TakeoffCard(place: first, day: outlookDay(of: first))
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("today-card")
                    }
                    VStack(spacing: 0) {
                        ForEach(shown.dropFirst()) { place in
                            NavigationLink(value: place.site) {
                                TakeoffRow(place: place, day: outlookDay(of: place))
                            }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("today-row")
                            if place.id != shown.last?.id {
                                Rectangle()
                                    .fill(.venti(.border))
                                    .frame(height: 1)
                            }
                        }
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
                    foot
                }
            }
        }
    }

    /// What the reading is, whose data it is, and how fresh: the design
    /// language asks a forecast for its update time and its source.
    private var foot: some View {
        let outlook = today.places.compactMap(\.outlook).first
        // The chosen day's own model, where the best take-off has the day:
        // a day after the finest model's run ends is a coarser one's.
        let read = ordered.first.flatMap { outlookDay(of: $0) }
        return VStack(alignment: .leading, spacing: VentiSpace.s2) {
            if let outlook, !outlook.note.isEmpty {
                let note = read.map(\.note).flatMap { $0.isEmpty ? nil : $0 } ?? outlook.note
                let run = read.map(\.runUnixS).flatMap { $0 == 0 ? nil : $0 } ?? outlook.runUnixS
                Text(
                    "\(note) Run of \(Format.time(unixSeconds: run)), looked up at \(Format.time(unixSeconds: outlook.fetchedUnixS))."
                )
            }
            Text("Flyable, marginal and not flyable are what the forecast's wind, gusts, rain and cloud say by fixed limits, and the kind of day is what its heat and temperatures say of thermals. They are not advice to fly.")
            ForEach(today.credits, id: \.source) { credit in
                Text(credit.text)
            }
        }
        .font(.ventiType(.caption))
        .foregroundStyle(.venti(.textSecondary))
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityIdentifier("today-foot")
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
                            ? "Wind in km/h at 10 m. The bar is the hour's status: tall for flyable, low for not. Under it the climb to expect in thermals, in m/s, with a bar that grows with it."
                            : "Wind in km/h at 10 m. The bar is the hour's status: tall for flyable, low for not."
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
        return hour.why.isEmpty ? "\(verdict)\(wind)\(thermals)" : "\(verdict): \(hour.why)\(wind)\(thermals)"
    }
}
