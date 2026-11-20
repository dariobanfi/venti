import SwiftUI
import VentiCore

/// The League tab: what the pilots of Venti share with everyone, and the
/// ranking of those pilots. The design language's fifth tab.
///
/// The lists are the community's own; this screen is the tab around them
/// and the switch between the two.
struct LeagueScreen: View {
    @Environment(WindModel.self) private var wind

    /// What the tab shows.
    private enum Shown: Hashable {
        case flights, ranking
    }

    @State private var shown = Shown.flights

    var body: some View {
        NavigationStack {
            Group {
                if wind.server == nil {
                    ContentUnavailableView {
                        Label("Venti is not connected", image: "icon-league")
                    } description: {
                        Text("Everyone's flights and the ranking come from Venti. Set its address in the settings, which open from the Today tab.")
                    }
                } else if shown == .ranking {
                    RankingList()
                } else {
                    PublicFlightsList()
                }
            }
            .navigationTitle("League")
            .toolbar {
                if wind.server != nil {
                    ToolbarItem(placement: .principal) {
                        VentiSegmented(
                            options: [
                                (value: Shown.flights, name: "Everyone"),
                                (value: Shown.ranking, name: "Ranking"),
                            ],
                            selection: $shown
                        )
                        .frame(width: 240)
                    }
                }
            }
            .navigationDestination(for: PublicFlight.self) { flight in
                PublicFlightScreen(flight: flight)
            }
        }
    }
}
