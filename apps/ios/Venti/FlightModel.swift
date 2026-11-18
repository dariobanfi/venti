import Foundation
import Observation
import VentiCore

/// Opens a flight through the core and holds what the screen shows.
///
/// The model contains no flying logic. It asks the core for a `FlightView`
/// and hands it to the screen.
@MainActor
@Observable
final class FlightModel {
    enum State: Sendable {
        case loading
        case loaded(FlightView)
        case failed(String)
    }

    private(set) var state: State = .loading
    private let core: Core

    init(core: Core = Core()) {
        self.core = core
    }

    /// Opens the sample flight bundled with the app.
    func loadSample() async {
        guard let url = Bundle.main.url(forResource: "sample", withExtension: "igc"),
              let text = try? String(contentsOf: url, encoding: .utf8)
        else {
            state = .failed("The sample flight is missing from the app.")
            return
        }
        await open(igcText: text)
    }

    /// Reads a flight file. Parsing runs off the main thread.
    func open(igcText: String) async {
        let core = self.core
        state = await Task.detached(priority: .userInitiated) { () -> State in
            do {
                return .loaded(try core.openFlight(igcText: igcText))
            } catch {
                return .failed(error.localizedDescription)
            }
        }.value
    }
}
