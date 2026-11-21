import SwiftUI
import VentiCore

/// What a pilot's public flights add up to, above the flights on the
/// pilot's page: the choice between all years and each season, the
/// numbers as labelled values, the twelve months as bars, and the
/// take-offs flown from most.
///
/// What is added up, every label and every line are the core's
/// (`pilots.rs`); this lays them out and holds no rule.
struct PilotTotalsSections: View {
    let statistics: PilotStatistics
    /// The year of the season shown; 0 is all years, as the core has it.
    @Binding var year: Int32

    /// The totals shown: the chosen season's, or all years'.
    static func shown(of statistics: PilotStatistics, year: Int32) -> PilotTotalsView {
        statistics.seasons.first { $0.year == year } ?? statistics.overall
    }

    var body: some View {
        let shown = Self.shown(of: statistics, year: year)
        if !statistics.seasons.isEmpty {
            Section {
                VentiSegmented(
                    options: ([statistics.overall] + statistics.seasons).map { (value: $0.year, name: $0.name) },
                    selection: $year
                )
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
                .accessibilityIdentifier("pilot-season")
            }
        }
        Section {
            ForEach(Array(shown.rows.enumerated()), id: \.offset) { _, row in
                if row.flightId.isEmpty {
                    PilotTotalLine(row: row)
                } else {
                    // The longest flight, the highest: the row opens it.
                    NavigationLink {
                        PublicFlightByAddress(id: row.flightId)
                    } label: {
                        PilotTotalLine(row: row)
                    }
                }
            }
            if shown.months.contains(where: { $0.flights > 0 }) {
                MonthBars(months: shown.months)
                    .padding(.vertical, VentiSpace.s1)
            }
        }
        Section {
            ForEach(shown.sites, id: \.id) { site in
                NavigationLink {
                    SiteByAddress(id: site.id)
                } label: {
                    Text(site.words)
                }
                .accessibilityIdentifier("pilot-site")
            }
        } footer: {
            VStack(alignment: .leading, spacing: VentiSpace.s2) {
                if !statistics.note.isEmpty {
                    Text(statistics.note)
                        .accessibilityIdentifier("pilot-totals-note")
                }
                if !statistics.terms.text.isEmpty {
                    Text(statistics.terms.text)
                }
            }
        }
    }
}

/// One number of a pilot's totals: what it is, and the number with its
/// unit, both as the core writes them.
private struct PilotTotalLine: View {
    let row: PilotTotalRow

    var body: some View {
        LabeledContent(row.label, value: row.value)
            .monospacedDigit()
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(row.label)
            .accessibilityValue(row.value)
            .accessibilityIdentifier("pilot-total")
    }
}

/// A public flight that is named by its address alone, as a row of a
/// pilot's totals names the longest: read from Venti, then shown as any
/// public flight is.
struct PublicFlightByAddress: View {
    let id: String

    @Environment(CommunityModel.self) private var community

    @State private var flight: PublicFlight?
    @State private var problem: String?

    var body: some View {
        Group {
            if let flight {
                PublicFlightScreen(flight: flight)
            } else if let problem {
                ContentUnavailableView {
                    Label {
                        Text("This flight could not be opened")
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
                flight = try await community.flight(id)
            } catch {
                problem = CommunityModel.words(for: error)
            }
        }
    }
}

/// A site that is named by its identifier alone, as a pilot's totals name
/// the take-offs flown from: read from Venti as it stands, then shown on
/// the site's own page.
struct SiteByAddress: View {
    let id: String

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
                        Text("This site could not be opened")
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
                problem = error is CoreError
                    ? error.words
                    : "Venti could not be reached for this site. \(error.localizedDescription)"
            }
        }
    }
}
