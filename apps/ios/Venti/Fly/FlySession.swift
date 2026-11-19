import Foundation
import Observation
import UIKit
import VentiCore

/// One flying session as the cockpit screen sees it.
///
/// It owns the phone's sensors and passes what they report to the core's
/// session; five times a second it asks the core what to show and what the
/// vario should sound like. It contains no flying logic.
///
/// A session is either live, fed by the sensors and recorded, or a replay
/// of a recorded flight, fed by the core and not recorded.
@MainActor
@Observable
final class FlySession {
    enum Mode: Equatable {
        case live
        /// Replaying a flight at this many times real speed.
        case replay(speed: Double)
    }

    /// What is running, if anything.
    private(set) var mode: Mode?
    /// What the cockpit shows. Present while a session runs.
    private(set) var view: CockpitView?
    /// Something the pilot must be told, such as location being denied.
    var problem: String?
    var isMuted = false {
        didSet { sound.isMuted = isMuted }
    }

    let vario = VarioLink()

    @ObservationIgnored private let dataFolder: URL
    @ObservationIgnored private var session: FlightSession?
    @ObservationIgnored private var replay: Replay?
    @ObservationIgnored private var ticker: Task<Void, Never>?
    @ObservationIgnored private let location = LocationProvider()
    @ObservationIgnored private let barometer = BarometerProvider()
    @ObservationIgnored private let sound = VarioSound()

    /// How often the screen and the vario tone are refreshed.
    private static let refreshInterval = Duration.milliseconds(200)

    init(dataFolder: URL) {
        self.dataFolder = dataFolder
    }

    /// Switches the sensors on and starts recording.
    func startFlight() {
        guard mode == nil else { return }
        let session = FlightSession(dataFolder: dataFolder.path)
        do {
            try session.startRecording()
        } catch {
            problem = error.localizedDescription
            return
        }

        location.onSample = { sample in session.onLocation(sample: sample) }
        location.onProblem = { [weak self] problem in self?.problem = problem }
        barometer.onPressure = { time, pressure in session.onPressure(timeUnixS: time, pressureHpa: pressure) }
        vario.onBytes = { bytes in
            session.onInstrumentBytes(timeUnixS: Date().timeIntervalSince1970, bytes: bytes)
        }
        location.start()
        barometer.start()
        vario.reconnectToRememberedVario()

        begin(session, mode: .live)
    }

    /// Plays a recorded flight through the cockpit, to try it on the ground.
    func startReplay(igcText: String, speed: Double) {
        guard mode == nil else { return }
        do {
            replay = try Replay(igcText: igcText)
        } catch {
            problem = error.localizedDescription
            return
        }
        begin(FlightSession(dataFolder: dataFolder.path), mode: .replay(speed: speed))
    }

    /// Ends the session. For a live flight, returns the recording to put
    /// into the logbook, if any position was recorded.
    func finish() -> PendingRecording? {
        guard let session else { return nil }
        ticker?.cancel()
        ticker = nil
        location.stop()
        barometer.stop()
        vario.onBytes = nil
        sound.stop()
        UIApplication.shared.isIdleTimerDisabled = false

        var recording: PendingRecording?
        do {
            recording = try session.stopRecording()
        } catch {
            // The file is still in the data folder and is picked up at the
            // next launch.
            problem = error.localizedDescription
        }
        self.session = nil
        replay = nil
        mode = nil
        view = nil
        return recording
    }

    private func begin(_ session: FlightSession, mode: Mode) {
        self.session = session
        self.mode = mode
        // A cockpit that dims and locks is no use.
        UIApplication.shared.isIdleTimerDisabled = true
        if !LaunchOptions.isUnderTest {
            sound.start()
        }
        refresh(elapsed: 0)

        ticker = Task { [weak self] in
            let clock = ContinuousClock()
            var last = clock.now
            while !Task.isCancelled {
                try? await Task.sleep(for: Self.refreshInterval)
                let now = clock.now
                let elapsed = now - last
                last = now
                self?.refresh(elapsed: Double(elapsed.components.seconds) + Double(elapsed.components.attoseconds) / 1e18)
            }
        }
    }

    private func refresh(elapsed: Double) {
        guard let session else { return }
        let now: Double
        if let replay, case .replay(let speed) = mode {
            _ = replay.advance(session: session, seconds: elapsed * speed)
            now = replay.now()
        } else {
            now = Date().timeIntervalSince1970
        }
        let view = session.view(nowUnixS: now)
        self.view = view
        sound.play(view.tone)
    }
}
