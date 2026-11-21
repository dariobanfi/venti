import SwiftUI
import VentiCore

/// What the pilot asks of the flying sites, as the app keeps it: which
/// sites, the wind a take-off is to work in, the height take-offs lie at
/// or above, and whether the places from pilots are among them. What it
/// leaves of the sites, and every word about it, is the core's
/// (`filterSites`, `sitesFilterChoices`, `siteWords`); this is the choice
/// alone.
struct SitesChoice: Hashable {
    /// The wind asked of take-offs.
    enum Wind: Hashable {
        /// No wind is asked.
        case any
        /// A direction the pilot chose: where the wind comes from, degrees.
        case from(Double)
        /// The wind the station nearest to the map's middle measured last.
        case measuredNow
    }

    var shown: SitesShown = .all
    var wind: Wind = .any
    /// Take-offs from this height up, metres; `nil` for any.
    var fromM: Double?
    /// Whether the places from pilots are asked for too: the crowdsourced
    /// layer, which no map shows until its pilot switches it on.
    var withPilots = false

    init() {}

    /// The choice as the pilot left it.
    init(keptIn store: UserDefaults) {
        switch store.string(forKey: Preferences.mapSitesShown) {
        case "takeoffs": shown = .takeoffs
        case "landings": shown = .landings
        default: shown = .all
        }
        switch store.string(forKey: Preferences.mapSitesWind) {
        case "measured":
            wind = .measuredNow
        case let kept?:
            wind = Double(kept).map(Wind.from) ?? .any
        case nil:
            wind = .any
        }
        let from = store.double(forKey: Preferences.mapSitesFromM)
        fromM = from > 0 ? from : nil
        withPilots = store.bool(forKey: Preferences.mapSitesFromPilots)
    }

    /// Keeps the choice for the next launch.
    func keep(in store: UserDefaults) {
        let shown = switch shown {
        case .all: "all"
        case .takeoffs: "takeoffs"
        case .landings: "landings"
        }
        store.set(shown, forKey: Preferences.mapSitesShown)
        let wind = switch wind {
        case .any: "any"
        case .from(let degrees): String(degrees)
        case .measuredNow: "measured"
        }
        store.set(wind, forKey: Preferences.mapSitesWind)
        store.set(fromM ?? 0, forKey: Preferences.mapSitesFromM)
        store.set(withPilots, forKey: Preferences.mapSitesFromPilots)
    }

    /// The choice as the core takes it. "Measured now" hands over the
    /// direction the map has, and asks about no wind where it has none.
    func filter(measuredFromDeg: Double?) -> SitesFilter {
        let (fromDeg, windIs): (Double?, SitesWindIs) = switch wind {
        case .any: (nil, .chosen)
        case .from(let degrees): (degrees, .chosen)
        case .measuredNow: (measuredFromDeg, .measuredNow)
        }
        return SitesFilter(shown: shown, windFromDeg: fromDeg, windIs: windIs, takeoffsFromM: fromM)
    }
}

/// The choices of the filter and their words, as the core has them.
enum SitesChoices {
    static let core = sitesFilterChoices()
    /// The words of the crowdsourced layer's switch, and of a site's page.
    static let words = siteWords()

    /// The winds to choose among: any, the wind measured now where the
    /// map has one, and the sixteen points of the compass.
    ///
    /// "Forecast now" is not offered: the app has no forecast at hand for
    /// the map's middle.
    static func winds(measuredFromDeg: Double?) -> [(value: SitesChoice.Wind, name: String)] {
        var winds: [(value: SitesChoice.Wind, name: String)] = [(.any, core.anyWind)]
        if measuredFromDeg != nil, let measured = core.windsNow.first(where: { $0.windIs == .measuredNow }) {
            winds.append((.measuredNow, measured.words))
        }
        return winds + core.winds.map { (.from($0.fromDeg), $0.words) }
    }

    /// Whether the wind and the height are asked under a choice of sites.
    static func asksTakeoffs(_ shown: SitesShown) -> Bool {
        core.shown.first { $0.shown == shown }?.asksTakeoffs ?? true
    }
}

/// Where the filter of the sites is chosen: which sites, the wind, the
/// height, then what the filter is and what it leaves, and the core's
/// note on what a fit says and does not say. The map and the list behind
/// it change as it is chosen.
struct SitesFilterForm: View {
    @Environment(SitesModel.self) private var sites

    var body: some View {
        let choices = SitesChoices.core
        let filtered = sites.filtered
        VentiList {
            Section {
                VentiSegmented(options: choices.shown.map { ($0.shown, $0.words) }, selection: shown)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("sites-shown")
            } header: {
                Text(choices.shownTitle)
                    .textCase(nil)
            }
            // Asked of take-offs only: under "Landings" neither is offered.
            if SitesChoices.asksTakeoffs(sites.choice.shown) {
                Section {
                    VentiSegmented(
                        options: SitesChoices.winds(measuredFromDeg: sites.measuredFromDeg), selection: wind
                    )
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("sites-wind")
                } header: {
                    Text(choices.windTitle)
                        .textCase(nil)
                }
                Section {
                    VentiSegmented(options: choices.heights.map { ($0.fromM, $0.words) }, selection: height)
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("sites-height")
                } header: {
                    Text(choices.heightTitle)
                        .textCase(nil)
                }
            }
            Section {
                Text(filtered.filterWords)
                    .font(.venti(.body, .semibold))
                    .accessibilityIdentifier("sites-filter-on")
                Text(filtered.words)
                    .accessibilityIdentifier("sites-filter-words")
                // How many of them are places from pilots, while that
                // layer is on and has any here.
                if !filtered.fromPilotsWords.isEmpty {
                    Text(filtered.fromPilotsWords)
                        .font(.venti(.footnote))
                        .foregroundStyle(.venti(.textSecondary))
                        .accessibilityIdentifier("sites-filter-from-pilots")
                }
                if !filtered.unsaidTitle.isEmpty {
                    VStack(alignment: .leading, spacing: VentiSpace.s1) {
                        Text(filtered.unsaidTitle)
                            .accessibilityIdentifier("sites-filter-unsaid")
                        Text(filtered.unsaidNote)
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textSecondary))
                    }
                }
            } footer: {
                Text(choices.note)
            }
            // At the foot: the crowdsourced layer, off until the pilot
            // switches it on, in the core's words.
            Section {
                Toggle(SitesChoices.words.pilotsLayer, isOn: withPilots)
                    .accessibilityIdentifier("sites-from-pilots")
            } footer: {
                Text(SitesChoices.words.pilotsLayerAbout)
                    .accessibilityIdentifier("sites-from-pilots-about")
            }
        }
        .accessibilityIdentifier("sites-filter-form")
        .navigationTitle("Filter the sites")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var shown: Binding<SitesShown> {
        Binding {
            sites.choice.shown
        } set: { shown in
            var choice = sites.choice
            choice.shown = shown
            sites.choose(choice)
        }
    }

    /// The wind chosen. "Measured now" without a station to read it from
    /// asks about no wind, and stands as "Any".
    private var wind: Binding<SitesChoice.Wind> {
        Binding {
            sites.choice.wind == .measuredNow && sites.measuredFromDeg == nil ? .any : sites.choice.wind
        } set: { wind in
            var choice = sites.choice
            choice.wind = wind
            sites.choose(choice)
        }
    }

    private var withPilots: Binding<Bool> {
        Binding {
            sites.choice.withPilots
        } set: { withPilots in
            var choice = sites.choice
            choice.withPilots = withPilots
            sites.choose(choice)
        }
    }

    private var height: Binding<Double?> {
        Binding {
            sites.choice.fromM
        } set: { fromM in
            var choice = sites.choice
            choice.fromM = fromM
            sites.choose(choice)
        }
    }
}

/// The filter's sheet over the map: the map stays in view above it and
/// changes as the pilot chooses.
struct SitesFilterSheet: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            SitesFilterForm()
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") {
                            dismiss()
                        }
                    }
                }
        }
    }
}

/// What is on and what it leaves, while a filter is on: the filter in a
/// line, the count of what fits, and the count of the take-offs that say
/// nothing, all in the core's words. A tap opens the filter.
struct SitesFilterLine: View {
    let filtered: SitesFiltered
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                Text(filtered.filterWords)
                    .font(.ventiType(.label))
                    .foregroundStyle(.venti(.text))
                Text(Self.leaves(filtered))
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textSecondary))
            }
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, minHeight: VentiSize.touch, alignment: .leading)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityHint("Opens the filter of the sites")
        .accessibilityIdentifier("sites-filter-line")
    }

    /// What the filter leaves, under it how many of those are places
    /// from pilots, and what says nothing.
    static func leaves(_ filtered: SitesFiltered) -> String {
        [filtered.words, filtered.fromPilotsWords, filtered.unsaidTitle]
            .filter { !$0.isEmpty }.joined(separator: "\n")
    }
}

/// The sign of the filter of the sites. The design pack has no icon for a
/// filter; this is the system's, as the round buttons without a pack icon
/// have theirs.
enum SitesFilterIcon {
    static let name = "line.3.horizontal.decrease"
}
