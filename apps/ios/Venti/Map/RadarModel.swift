import Foundation
import Observation
import SwiftUI
import VentiCore

/// One layer's picture as the map draws it: the server's picture in the
/// app's colours, and the four edges it is pinned to the map by.
struct RadarPicture: Equatable {
    /// The radar source the frame is of.
    let source: String
    /// The frame's time, which with the source says which picture it is.
    let timeUnixS: Int64
    /// Whether what the source did not see is drawn as the veil: so for
    /// the lowest picture of a moment, and clear in those above it.
    let drawsUnseen: Bool
    /// The pictures that lie over it at its moment, by their frames'
    /// paths, the next above first: where one of them sees a place this
    /// picture is clear, so under others it is another picture.
    let above: [String]
    /// The picture, a PNG.
    let png: Data
    let west: Double
    let south: Double
    let east: Double
    let north: Double

    /// Which picture it is, so that the map draws it again only when it
    /// is another.
    var id: String {
        "\(source) \(timeUnixS) \(drawsUnseen) \(west) \(south) \(east) \(north) under \(above.joined(separator: " "))"
    }
}

/// The rain radar of the map tab: the last frames Venti holds of what a
/// weather service's radars saw (`GET /v1/radar`), a layer for each
/// source, and the pictures of the moment chosen.
///
/// What to ask, how to read the answer, every word about a frame, which
/// frame of which layer lies on the map at a moment and the picture in the
/// app's colours are the core's (`radarPath`, `radar`, `RadarView.moments`,
/// `radarFrameImageUnder`). This class asks while the map tab's layer is
/// on, again every five minutes, and reads the answer it has again every
/// minute, so that a frame's age in words stays true. It keeps the
/// pictures it fetched for this run of the app as the server sent them,
/// by layer and frame; a picture in the app's colours is coloured anew
/// for each moment, as it depends on the pictures that lie over it.
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
        /// There is a list of layers and their frames.
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
    /// How long a moment stays while the moments play: four a second.
    static let playStep = Duration.milliseconds(250)
    /// The status a Venti without a rain radar answers with (`no_radar`).
    static let noRadarStatus = 501

    private(set) var state = State.idle
    /// The radar as the core read it, once there is a list.
    private(set) var view: RadarView?
    /// The pictures on the map, the first at the bottom: the chosen
    /// moment's once they are all there, and until then those of the
    /// moment before it, with its own time.
    private(set) var pictures: [RadarPicture] = []
    /// Why the chosen moment's pictures are not on the map, in words.
    private(set) var pictureProblem: String?
    /// Whether a picture is being fetched.
    private(set) var isFetching = false
    /// Whether the moments are playing.
    private(set) var isPlaying = false
    /// The time of the moment the pilot chose; `nil` for the one the card
    /// opens at, the newest, which a newer one then takes the place of.
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
    /// The pictures of this run of the app as the server sent them, by
    /// the frame's path, which names its layer and its time.
    @ObservationIgnored private var fetched: [String: Data] = [:]
    @ObservationIgnored private var playing: Task<Void, Never>?

    init(transport: @escaping Transport = RadarModel.fetch) {
        self.transport = transport
    }

    /// The moment chosen: the pilot's, or the one the card opens at.
    var moment: RadarMoment? {
        guard let view else { return nil }
        return view.moments.first { $0.timeUnixS == chosenTime } ?? Self.opening(of: view)
    }

    /// The chosen moment by its place among the moments.
    var chosenIndex: Int? {
        guard let view, let moment else { return nil }
        return view.moments.firstIndex { $0.timeUnixS == moment.timeUnixS }
    }

    /// The moment whose pictures are on the map.
    var shownMoment: RadarMoment? {
        guard let view, !pictures.isEmpty else { return nil }
        let onTheMap = pictures.map(\.id)
        // The chosen one before any other that draws the same.
        return ([moment].compactMap { $0 } + view.moments).first { drawn(at: $0).map(\.id) == onTheMap }
    }

    /// What to write beside the pictures: the line of the moment on the
    /// map, the chosen one's while the map has none.
    var label: String {
        (pictures.isEmpty ? moment : shownMoment)?.label ?? ""
    }

    /// The chosen moment for VoiceOver: its time, and the age of its
    /// topmost picture.
    var spoken: String {
        guard let moment else { return "" }
        guard let top = drawn(at: moment).last else { return moment.clock }
        return "\(moment.clock), \(top.frame.age)"
    }

    /// The layers drawn, the topmost first: those of the pictures on the
    /// map, and of the chosen moment while the map has none.
    var drawnLayers: [RadarLayerView] {
        guard let view else { return [] }
        let sources =
            pictures.isEmpty
            ? (moment.map { drawn(at: $0).map(\.layer.source) } ?? [])
            : pictures.map(\.source)
        return sources.reversed().compactMap { source in view.layers.first { $0.source == source } }
    }

    /// That the newest picture is too old to be the rain of now, in the
    /// core's words: of the topmost layer that has frames, and of the
    /// topmost while none has. `nil` while it is not.
    var caution: String? {
        guard let view, let layer = view.layers.last(where: { !$0.frames.isEmpty }) ?? view.layers.last else {
            return nil
        }
        return layer.stale ? layer.staleWords : nil
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

    /// Asks for the list of frames, and then for the chosen moment's
    /// pictures.
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
        await showPictures()
    }

    /// Reads the answer at hand again by the clock as it is now: the
    /// frames' ages, and whether the newest is still the rain of now.
    func readAgain() async {
        guard let answer, let read = try? await read(status: answer.status, body: answer.body) else { return }
        show(read)
    }

    /// Chooses a moment by its place among the moments, and puts its
    /// pictures on the map. The one the card opens at is no choice: a
    /// newer moment takes its place when it comes.
    func choose(_ index: Int) async {
        guard let view, view.moments.indices.contains(index) else { return }
        let moment = view.moments[index]
        chosenTime = moment.timeUnixS == Self.opening(of: view)?.timeUnixS ? nil : moment.timeUnixS
        await showPictures()
    }

    /// Fetches the chosen moment's pictures again, after one could not be
    /// had.
    func tryPictureAgain() async {
        await showPictures()
    }

    /// Plays the moments from the oldest to the newest, four a second,
    /// and stops at the newest; a second call while they play stops them.
    /// The pictures are all fetched before the first is shown.
    func play() {
        if isPlaying {
            stop()
            return
        }
        guard let view, view.moments.count > 1 else { return }
        let moments = view.moments
        isPlaying = true
        playing = Task {
            defer {
                // Unless stopped, when another playing may have begun.
                if !Task.isCancelled {
                    isPlaying = false
                }
            }
            isFetching = true
            for moment in moments {
                for one in drawn(at: moment) {
                    _ = try? await served(one.frame.path)
                }
            }
            isFetching = false
            for index in moments.indices {
                if Task.isCancelled {
                    return
                }
                await choose(index)
                if index < moments.count - 1 {
                    try? await Task.sleep(for: Self.playStep)
                }
            }
        }
    }

    /// Stops the moments where they are.
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
        // A moment the server has dropped is no longer chosen.
        if let chosenTime, !read.moments.contains(where: { $0.timeUnixS == chosenTime }) {
            self.chosenTime = nil
        }
    }

    /// The moment the card opens at.
    private static func opening(of view: RadarView) -> RadarMoment? {
        let first = Int(view.firstMoment)
        return view.moments.indices.contains(first) ? view.moments[first] : view.moments.last
    }

    /// One picture a moment asks for: the layer, which of its frames,
    /// whether the veil over what the source did not see is drawn, and
    /// the frames whose pictures lie over it, by their paths.
    private struct Drawn {
        let layer: RadarLayerView
        let frame: RadarFrameView
        let drawsUnseen: Bool
        var above: [String] = []

        func picture(_ png: Data) -> RadarPicture {
            RadarPicture(
                source: layer.source, timeUnixS: frame.timeUnixS, drawsUnseen: drawsUnseen, above: above, png: png,
                west: layer.west, south: layer.south, east: layer.east, north: layer.north
            )
        }

        /// The picture's own name for itself, without fetching it.
        var id: String { picture(Data()).id }
    }

    /// What a moment draws, the first at the bottom, as the core says it:
    /// each picture by its layer and its frame, and with the frames after
    /// it, which lie over it.
    private func drawn(at moment: RadarMoment) -> [Drawn] {
        guard let view else { return [] }
        var all: [Drawn] = moment.frames.compactMap { one in
            guard view.layers.indices.contains(Int(one.layer)) else { return nil }
            let layer = view.layers[Int(one.layer)]
            guard layer.frames.indices.contains(Int(one.frame)) else { return nil }
            return Drawn(layer: layer, frame: layer.frames[Int(one.frame)], drawsUnseen: one.drawsUnseen)
        }
        for place in all.indices {
            all[place].above = all[(place + 1)...].map(\.frame.path)
        }
        return all
    }

    /// Puts the chosen moment's pictures on the map, all of them at once.
    /// A picture that cannot be had leaves those of the moment before
    /// there, with their own time: a moment is not drawn in part, as its
    /// line says what lies under what.
    private func showPictures() async {
        guard let moment else {
            pictures = []
            pictureProblem = nil
            return
        }
        let wanted = drawn(at: moment)
        do {
            if wanted.contains(where: { fetched[$0.frame.path] == nil }) {
                isFetching = true
            }
            defer { isFetching = false }
            // All of them from the server first, the lowest first: a
            // picture is coloured by those that lie over it.
            for one in wanted {
                _ = try await served(one.frame.path)
            }
            var shown: [RadarPicture] = []
            for one in wanted {
                shown.append(try await picture(of: one))
            }
            // Another moment may have been chosen meanwhile.
            guard self.moment?.timeUnixS == moment.timeUnixS else { return }
            if pictures != shown {
                pictures = shown
            }
            pictureProblem = nil
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            guard self.moment?.timeUnixS == moment.timeUnixS else { return }
            pictureProblem =
                error is CoreError
                ? error.words
                : "Venti could not be reached for the radar picture. \(error.localizedDescription)"
        }
    }

    /// A frame's picture as the server sent it: kept from earlier in
    /// this run of the app, or fetched and kept.
    private func served(_ path: String) async throws -> Data {
        if let kept = fetched[path] {
            return kept
        }
        guard let server = server(), let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, answer) = try await transport(url)
        guard status == 200 else {
            // The core's words for what a server says in place of a picture.
            _ = try radar(status: UInt16(clamping: status), body: answer, utcOffsetS: 0, nowUnixS: 0)
            throw URLError(.badServerResponse)
        }
        // The pilot may have changed the address while this was under way.
        guard server == self.server() else { throw CancellationError() }
        fetched[path] = answer
        return answer
    }

    /// A layer's picture of a frame in the app's colours: the server's,
    /// coloured by the core, with what the source did not see as the veil
    /// or clear, and clear wherever a picture that lies over it at the
    /// moment sees, so that a place has the word of the topmost picture
    /// that sees it and two pictures' rain never adds up.
    private func picture(of one: Drawn) async throws -> RadarPicture {
        guard let view else {
            throw URLError(.badURL)
        }
        let body = try await served(one.frame.path)
        var above: [Data] = []
        for path in one.above {
            above.append(try await served(path))
        }
        let palette = colours(view.key)
        do {
            let png = try radarFrameImageUnder(
                picture: body, above: above, rain: palette.rain,
                unseen: one.drawsUnseen ? palette.unseen : RadarColours.clear
            )
            return one.picture(png)
        } catch {
            // What the core could not read is not kept: it is asked for
            // again.
            for path in [one.frame.path] + one.above {
                fetched[path] = nil
            }
            throw error
        }
    }

    /// Forgets what was another server's.
    private func forget() {
        stop()
        view = nil
        answer = nil
        pictures = []
        pictureProblem = nil
        chosenTime = nil
        fetched = [:]
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

    /// What a source did not see in a picture that lies over another's:
    /// nothing, so that the picture below shows through.
    static let clear: UInt32 = 0

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
