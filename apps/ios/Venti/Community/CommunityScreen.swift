import SwiftUI
import VentiCore

/// The Community tab, the fifth (it was "League" until 2026-11-23): pilots
/// among each other first, and what the pilots of Venti share with
/// everyone. The pill over it chooses: "Community" (`CommunityHub`: the
/// plans of the pilot's groups, the flights of the pilots they follow,
/// the pilots near, finding a pilot, what they were told), "Everyone"
/// (everyone's public flights) and "Ranking". The flights of the pilots
/// followed, whole, are a tap away from the hub.
struct CommunityScreen: View {

    /// What the tab shows.
    private enum Shown: Hashable {
        case community, flights, ranking
    }

    @State private var shown = Shown.community

    private var options: [(value: Shown, name: String)] {
        let words = Social.words
        return [
            (value: .community, name: "Community"),
            (value: .flights, name: words.feedEveryone),
            (value: .ranking, name: words.feedRanking),
        ]
    }

    var body: some View {
        NavigationStack {
            Group {
                switch shown {
                case .community: CommunityHub()
                case .flights: PublicFlightsList()
                case .ranking: RankingList()
                }
            }
            .navigationTitle("Community")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    VentiSegmented(options: options, selection: $shown)
                        .frame(width: 300)
                }
            }
            .navigationDestination(for: PublicFlight.self) { flight in
                PublicFlightScreen(flight: flight)
            }
            // The flights of the pilots followed, whole: a step of the
            // same stack, so that its flights open as the hub's do.
            .navigationDestination(for: CommunityRoute.self) { route in
                switch route {
                case .following:
                    FollowingFeed()
                        .navigationTitle(Social.words.feedFollowing)
                        .navigationBarTitleDisplayMode(.inline)
                }
            }
        }
    }
}

/// A page of Community, as a step in its navigation stack.
enum CommunityRoute: Hashable {
    case following
}
