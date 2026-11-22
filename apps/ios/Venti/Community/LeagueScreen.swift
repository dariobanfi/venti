import SwiftUI
import VentiCore

/// The League tab: what the pilots of Venti share with everyone, and the
/// ranking of those pilots. The design language's fifth tab.
///
/// The lists are the community's own; this screen is the tab around them
/// and the switch between them. A signed-in pilot has a third: the
/// flights of the pilots they follow.
struct LeagueScreen: View {

    /// What the tab shows.
    private enum Shown: Hashable {
        case flights, following, ranking
    }

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var shown = Shown.flights

    /// The choices: the feed only for a pilot who is signed in.
    private var options: [(value: Shown, name: String)] {
        let words = Social.words
        var options = [(value: Shown.flights, name: words.feedEveryone)]
        if Social(account: account, wind: wind).isSignedIn {
            options.append((value: .following, name: words.feedFollowing))
        }
        return options + [(value: .ranking, name: words.feedRanking)]
    }

    var body: some View {
        NavigationStack {
            Group {
                switch shown {
                case .ranking: RankingList()
                // Signed out meanwhile: everyone's flights again.
                case .following where options.count == 3: FollowingFeed()
                default: PublicFlightsList()
                }
            }
            .navigationTitle("League")
            .toolbar {
                ToolbarItem(placement: .principal) {
                    VentiSegmented(options: options, selection: $shown)
                        .frame(width: options.count == 3 ? 300 : 240)
                }
            }
            .navigationDestination(for: PublicFlight.self) { flight in
                PublicFlightScreen(flight: flight)
            }
        }
    }
}
