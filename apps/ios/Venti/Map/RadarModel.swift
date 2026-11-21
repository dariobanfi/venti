import Foundation
import Observation
import SwiftUI
import VentiCore

/// A frame's picture as the map draws it: the server's picture in the
/// app's colours, and the four edges it is pinned to the map by.
struct RadarPicture: Equatable {
    /// The radar source the frame is of.
    let source: String
    /// The frame's time, which with the source says which picture it is.
    let timeUnixS: Int64
    /// The picture, a PNG.
    let png: Data
    let west: Double
    let south: Double
    let east: Double
    let north: Double

    /// Which picture it is, so that the map draws it again only when it
    /// is another.
    var id: String { "\(source) \(timeUnixS) \(west) \(south) \(east) \(north)" }
}

/// The rain radar of the map tab: the last frames Venti holds of what a
/// weather service's radars saw (`GET /v1/radar`), and the picture of the
/// frame chosen.
///
/// What to ask, how to read the answer, every word about a frame and the
/// picture in the app's colours are the core's (`radarPath`, `radar`,
/// `radarFrameImage`). This class asks while the map tab's layer is on,
/// again every five minutes, and reads the answer it has again every
/// minute, so that a frame's age in words stays true. It keeps the
/// pictures it fetched for this run of the app. Of the layers of an answer
/// it shows the first: Venti has one radar source today.
@MainActor
@Observable
final class RadarModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    enum State: Equatable {
        /// Nothing asked yet, or no Venti to ask.
        case idle
        /// Asked, and no list yet.
        case loading
        /// There is a list of frames.
        case loaded
        /// This Venti has no rain radar, in the core's words.
        case none(String)
        /// No list: refused or not reached, in words for the pilot.
        case failed(String)
    }

    /// How often Venti is asked for the list while the layer is on: a
    /// source publishes a frame every five minutes.
    static let askEvery: TimeInterval = 5 * 60
    /// How often the answer at hand is read again, for the ages in words.
    static let readEvery = Duration.seconds(60)
    /// How long a frame stays while the frames play: four a second.
    static let playStep = Duration.milliseconds(250)
    /// The status a Venti without a rain radar answers with (`no_radar`).
    static let noRadarStatus = 501

    private(set) var state = State.idle
    /// The radar as the core read it, once there is a list.
    private(set) var view: RadarView?
    /// The picture on the map: the chosen frame's once it is there, and
    /// until then the one before it, with its own time.
    private(set) var picture: RadarPicture?
    /// Why the chosen frame's picture is not on the map, in words.
    private(set) var pictureProblem: String?
    /// Whether a picture is being fetched.
    private(set) var isFetching = false
    /// Whether the frames are playing.
    private(set) var isPlaying = false
    /// The time of the frame the pilot chose; `nil` for the newest, which
    /// a new frame then takes the place of.
    private(set) var chosenTime: Int64?

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }
    /// The time by the server's clock, seconds since 1970-01-01 UTC, which
    /// the frames' ages are counted against. The app sets this.
    @ObservationIgnored var clock: () async -> Int64 = { Int64(Date().timeIntervalSince1970) }
    /// The iPhone's time, by which it is decided when to ask again.
    @ObservationIgnored var now: () -> Date = { Date() }
    /// The colours of the six steps of rain, the first step first, and of
    /// what the radar did not see, each `0xRRGGBBAA`, for a key.
    @ObservationIgnored var colours: ([RadarKey]) -> (rain: [UInt32], unseen: UInt32) = RadarColours.palette(for:)

    @ObservationIgnored private let transport: Transport
    /// The answer as it came, to read again when the clock has moved on.
    @ObservationIgnored private var answer: (status: UInt16, body: Data)?
    @ObservationIgnored private var askedAt: Date?
    @ObservationIgnored private var askedServer: URL?
    /// The pictures of this run of the app, in the app's colours, by the
    /// frame's path.
    @ObservationIgnored private var pictures: [String: Data] = [:]
    @ObservationIgnored private var playing: Task<Void, Never>?

    init(transport: @escaping Transport = RadarModel.fetch) {
        self.transport = transport
    }

    /// The layer shown: the first of the answer.
    var layer: RadarLayerView? { view?.layers.first }

    /// The frame chosen: the pilot's, or the newest.
    var frame: RadarFrameView? {
        guard let layer else { return nil }
        return layer.frames.first { $0.timeUnixS == chosenTime } ?? layer.frames.last
    }

    /// The chosen frame by its place among the layer's frames.
    var chosenIndex: Int? {
        guard let layer, let frame else { return nil }
        return layer.frames.firstIndex { $0.timeUnixS == frame.timeUnixS }
    }

    /// The frame whose picture is on the map.
    var shownFrame: RadarFrameView? {
        guard let layer, let picture, picture.source == layer.source else { return nil }
        return layer.frames.first { $0.timeUnixS == picture.timeUnixS }
    }

    /// What to write beside the pictures: the line of the frame on the
    /// map, the chosen one's while none is.
    var label: String {
        (shownFrame ?? frame)?.label ?? ""
    }

    /// For as long as the layer is on and the map in front: asks at once,
    /// reads the answer again every minute, and asks again every five.
    func keepCurrent() async {
        await load()
        while !Task.isCancelled {
            try? await Task.sleep(for: Self.readEvery)
            if Task.isCancelled {
                return
            }
            if isDue {
                await load()
            } else {
                await readAgain()
            }
        }
    }

    /// Whether it is time to ask again. A Venti that said it has no radar
    /// is not asked every five minutes.
    var isDue: Bool {
        guard let askedAt, askedServer == server() else { return true }
        if case .none = state {
            return false
        }
        return now().timeIntervalSince(askedAt) >= Self.askEvery
    }

    /// Asks for the list of frames, and then for the chosen frame's
    /// picture.
    func load() async {
        guard let server = server() else {
            forget()
            askedAt = nil
            askedServer = nil
            state = .idle
            return
        }
        if askedServer != server {
            // Another server's rain is not this one's.
            forget()
        }
        askedServer = server
        askedAt = now()
        if view == nil {
            state = .loading
        }
        do {
            guard let url = URL(string: server.absoluteString + radarPath()) else {
                state = view == nil ? .failed("Venti could not be asked for the rain radar.") : .loaded
                return
            }
            let (status, body) = try await transport(url)
            // The pilot may have changed the address while this was under way.
            guard server == self.server() else { return }
            do {
                let read = try await self.read(status: UInt16(clamping: status), body: body)
                answer = (UInt16(clamping: status), body)
                show(read)
                state = .loaded
            } catch let error as CoreError where status == Self.noRadarStatus {
                forget()
                state = .none(error.words)
                return
            }
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            guard server == self.server() else { return }
            if view != nil {
                // The frames fetched earlier stay, and grow old in words.
                await readAgain()
                state = .loaded
            } else {
                state = .failed(
                    error is CoreError
                        ? error.words
                        : "Venti could not be reached for the rain radar. \(error.localizedDescription)"
                )
            }
            return
        }
        await showPicture()
    }

    /// Reads the answer at hand again by the clock as it is now: the
    /// frames' ages, and whether the newest is still the rain of now.
    func readAgain() async {
        guard let answer, let read = try? await read(status: answer.status, body: answer.body) else { return }
        show(read)
    }

    /// Chooses a frame by its place among the layer's frames, and puts
    /// its picture on the map. The newest is no choice: a newer frame
    /// takes its place when it comes.
    func choose(_ index: Int) async {
        guard let layer, layer.frames.indices.contains(index) else { return }
        chosenTime = index == layer.frames.count - 1 ? nil : layer.frames[index].timeUnixS
        await showPicture()
    }

    /// Fetches the chosen frame's picture again, after it could not be had.
    func tryPictureAgain() async {
        await showPicture()
    }

    /// Plays the frames from the oldest to the newest, four a second, and
    /// stops at the newest; a second call while they play stops them. The
    /// pictures are all fetched before the first is shown.
    func play() {
        if isPlaying {
            stop()
            return
        }
        guard let layer, layer.frames.count > 1 else { return }
        isPlaying = true
        playing = Task {
            defer {
                // Unless stopped, when another playing may have begun.
                if !Task.isCancelled {
                    isPlaying = false
                }
            }
            isFetching = true
            for frame in layer.frames {
                _ = try? await coloured(frame)
            }
            isFetching = false
            for index in layer.frames.indices {
                if Task.isCancelled {
                    return
                }
                await choose(index)
                if index < layer.frames.count - 1 {
                    try? await Task.sleep(for: Self.playStep)
                }
            }
        }
    }

    /// Stops the frames where they are.
    func stop() {
        playing?.cancel()
        playing = nil
        isPlaying = false
    }

    /// Fetches with the system's own networking and its cache: a frame of
    /// a moment does not change, and the server says so.
    nonisolated static func fetch(_ url: URL) async throws -> (Int, Data) {
        var request = URLRequest(url: url)
        request.timeoutInterval = 20
        let (data, response) = try await URLSession.shared.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, data)
    }

    private func read(status: UInt16, body: Data) async throws -> RadarView {
        let nowUnixS = await clock()
        let offset = TimeZone.current.secondsFromGMT(for: Date(timeIntervalSince1970: TimeInterval(nowUnixS)))
        return try radar(status: status, body: body, utcOffsetS: Int32(offset), nowUnixS: nowUnixS)
    }

    private func show(_ read: RadarView) {
        if view != read {
            view = read
        }
        // A frame the server has dropped is no longer chosen.
        if let chosenTime, layer?.frames.contains(where: { $0.timeUnixS == chosenTime }) != true {
            self.chosenTime = nil
        }
    }

    /// Puts the chosen frame's picture on the map. A picture that cannot
    /// be had leaves the one before it there, with its own time.
    private func showPicture() async {
        guard let layer, let frame else {
            picture = nil
            pictureProblem = nil
            return
        }
        do {
            if pictures[frame.path] == nil {
                isFetching = true
            }
            defer { isFetching = false }
            let png = try await coloured(frame)
            // Another frame may have been chosen meanwhile.
            guard self.layer?.source == layer.source, self.frame?.timeUnixS == frame.timeUnixS else { return }
            let shown = RadarPicture(
                source: layer.source, timeUnixS: frame.timeUnixS, png: png,
                west: layer.west, south: layer.south, east: layer.east, north: layer.north
            )
            if picture != shown {
                picture = shown
            }
            pictureProblem = nil
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            guard self.frame?.timeUnixS == frame.timeUnixS else { return }
            pictureProblem =
                error is CoreError
                ? error.words
                : "Venti could not be reached for the radar picture. \(error.localizedDescription)"
        }
    }

    /// A frame's picture in the app's colours: kept from earlier in this
    /// run of the app, or fetched and coloured by the core.
    private func coloured(_ frame: RadarFrameView) async throws -> Data {
        if let kept = pictures[frame.path] {
            return kept
        }
        guard let server = server(), let url = URL(string: server.absoluteString + frame.path), let view else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        guard status == 200 else {
            // The core's words for what a server says in place of a picture.
            _ = try radar(status: UInt16(clamping: status), body: body, utcOffsetS: 0, nowUnixS: 0)
            throw URLError(.badServerResponse)
        }
        let palette = colours(view.key)
        let png = try radarFrameImage(picture: body, rain: palette.rain, unseen: palette.unseen)
        guard server == self.server() else { throw CancellationError() }
        pictures[frame.path] = png
        return png
    }

    /// Forgets what was another server's.
    private func forget() {
        stop()
        view = nil
        answer = nil
        picture = nil
        pictureProblem = nil
        chosenTime = nil
        pictures = [:]
    }
}

/// The colours of the rain radar, which the core names as a scale of the
/// pack and a step of it: the light theme's, as the map is light in both
/// appearances. The app chooses no colour of its own.
enum RadarColours {
    /// The colour of a line of the key.
    static func onMap(_ key: RadarKey) -> Color {
        ThermalKeyColour.onMap(scale: key.scale, scaleStep: key.scaleStep)
    }

    /// The veil over what the radar did not see: the forecast map's veil.
    static var unseenOnMap: Color {
        ThermalKeyColour.onMap(scale: .veil, scaleStep: 1)
    }

    /// The colours as the core takes them for a picture: the six steps of
    /// rain, the first step first, and the veil, each `0xRRGGBBAA` and
    /// solid. How see-through the picture is drawn is the map layer's.
    static func palette(for key: [RadarKey]) -> (rain: [UInt32], unseen: UInt32) {
        let rain = key.sorted { $0.step < $1.step }.map { rgba(UIColor(onMap($0))) }
        return (rain, rgba(UIColor(unseenOnMap)))
    }

    /// A colour as `0xRRGGBBAA`.
    static func rgba(_ colour: UIColor) -> UInt32 {
        var (red, green, blue, alpha): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        colour.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        func byte(_ part: CGFloat) -> UInt32 {
            UInt32((min(max(part, 0), 1) * 255).rounded())
        }
        return byte(red) << 24 | byte(green) << 16 | byte(blue) << 8 | byte(alpha)
    }
}
