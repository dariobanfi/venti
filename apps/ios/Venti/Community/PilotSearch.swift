import Foundation
import Observation
import SwiftUI
import VentiCore

/// The search for a pilot by name, over everyone's flights.
///
/// Which text is asked for, what to ask and every line of the answer are
/// the core's. This waits until the typing rests, because Venti counts a
/// search against what one address may read in an hour, and keeps the
/// answer to the text last typed.
@MainActor
@Observable
final class PilotSearch {
    /// How long the typing rests before Venti is asked.
    static let rest = Duration.milliseconds(350)

    /// What Venti found for the text last typed. `nil` before it answered
    /// and where nothing is asked.
    private(set) var found: PilotsFound?
    /// What stands in place of pilots: the core's words for a text it does
    /// not ask for, or why Venti did not answer.
    private(set) var words: String?
    /// Whether an answer is waited for.
    private(set) var isSearching = false

    /// The question under way, for a test to wait for.
    @ObservationIgnored private(set) var waiting: Task<Void, Never>?
    /// The text the answer is wanted for.
    @ObservationIgnored private var wanted = ""

    /// The pilot typed: asks Venti for this text once the typing rests.
    /// An empty text asks nothing and shows nothing.
    func search(_ typed: String, in community: CommunityModel, after rest: Duration = PilotSearch.rest) {
        let text = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text != wanted else { return }
        wanted = text
        waiting?.cancel()
        waiting = nil
        found = nil
        words = nil
        isSearching = false
        guard !text.isEmpty else { return }
        // Whether a text is asked for is the core's, and so are the words
        // where it is not.
        do {
            _ = try pilotsSearchPath(text: text)
        } catch {
            words = error.words
            return
        }
        isSearching = true
        waiting = Task { [weak self] in
            try? await Task.sleep(for: rest)
            guard !Task.isCancelled else { return }
            await self?.ask(text, community)
        }
    }

    private func ask(_ text: String, _ community: CommunityModel) async {
        do {
            let found = try await community.pilots(named: text)
            guard text == wanted else { return }
            self.found = found
            words = nil
        } catch {
            guard text == wanted else { return }
            if error is CancellationError || (error as? URLError)?.code == .cancelled {
                return
            }
            found = nil
            words = error is CoreError
                ? error.words
                : pilotsWords().unreachable
        }
        isSearching = false
    }
}
