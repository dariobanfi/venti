import Foundation
import Observation
import VentiCore

/// Foehn, as Venti reads it from the pressure on both sides of the Alps'
/// main ridge: one line for Today when the core has one, and the view
/// behind it.
///
/// What the pressures say, every word of it, and whether Today gets a line
/// are the core's (`foehn`). This class asks the server at the core's path,
/// hands the answer to the core with the places of Today's take-offs and
/// Venti's time, and keeps what came.
@MainActor
@Observable
final class FoehnModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    typealias State = AskingState

    /// How long an answer is read again before the server is asked anew:
    /// the stations report every ten minutes.
    static let keepFor: TimeInterval = 10 * 60

    /// What the view is called until the core has read an answer and says
    /// so itself (`FoehnView.title`): the core has the word only with an
    /// answer, and the way to the view must be there without one.
    static let titleUntilRead = "Foehn"

    private(set) var state = State.idle
    /// The core's reading of the last answer. `nil` until there is one,
    /// and again when the server could not be asked: a reading that could
    /// not be brought up to date is not shown as the present one.
    private(set) var view: FoehnView?

    @ObservationIgnored private let transport: Transport
    /// The server's answer as it came, to be read again for other places
    /// and a later moment without asking.
    @ObservationIgnored private var answer: (server: URL, status: Int, body: Data, at: Date)?

    init(transport: @escaping Transport = WindModel.fetch) {
        self.transport = transport
    }

    /// The line for Today, while the core gives one.
    var line: FoehnLine? {
        state == .loaded ? view?.todayLine : nil
    }

    /// The view's title: the core's, once it has read an answer.
    var title: String {
        view?.title ?? Self.titleUntilRead
    }

    /// Asks Venti for foehn and has the core read the answer for these
    /// places, by Venti's clock (`now`: the server's time where it was
    /// read, else 0, never the iPhone's). An answer of the last minutes is
    /// read again without asking, unless `anew`.
    func load(from server: URL?, places: [Coordinate], now: () async -> Int64, anew: Bool = false) async {
        guard let server else {
            view = nil
            answer = nil
            state = .failed("Venti could not be reached.")
            return
        }
        if !anew, let answer, answer.server == server, Date().timeIntervalSince(answer.at) < Self.keepFor {
            read(status: answer.status, body: answer.body, places: places, nowUnixS: await now())
            return
        }
        if view == nil {
            state = .loading
        }
        do {
            guard let url = URL(string: server.absoluteString + foehnPath()) else { throw URLError(.badURL) }
            let (status, body) = try await transport(url)
            let nowUnixS = await now()
            view = try foehn(status: UInt16(clamping: status), body: body, places: places, nowUnixS: nowUnixS)
            answer = (server, status, body, Date())
            state = .loaded
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            view = nil
            answer = nil
            state = .failed(
                error is CoreError
                    ? error.words
                    : "Venti could not be reached for foehn. \(error.localizedDescription)"
            )
        }
    }

    private func read(status: Int, body: Data, places: [Coordinate], nowUnixS: Int64) {
        do {
            view = try foehn(status: UInt16(clamping: status), body: body, places: places, nowUnixS: nowUnixS)
            state = .loaded
        } catch {
            view = nil
            state = .failed(error.words)
        }
    }
}
