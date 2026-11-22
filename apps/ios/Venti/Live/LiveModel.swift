import Foundation
import Observation
import UIKit
import VentiCore

/// Who is in the air: the gliders whose pilots show their flight to
/// everyone, from the Venti server the pilot has set, for the part of the
/// map in view, and beside them the aircraft another network hears (the
/// Open Glider Network), never one record of both.
///
/// What to ask the server and how to read its answers is the core's, and
/// so is the layer that puts the two side by side (`liveLayer`). This
/// class sends the requests, and again every quarter of a minute while the
/// map is on screen.
@MainActor
@Observable
final class LiveModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// How often the server is asked again, seconds. It asks for ten to
    /// fifteen.
    static let askEvery = 15.0

    /// A map that shows more than this many degrees of longitude asks for
    /// no gliders: the Alps from end to end are a crowd, not pilots.
    static let widestMap = 6.0

    /// The gliders in the part of the map in view.
    private(set) var gliders: [LiveGlider] = []
    /// When the answer came that they are from. A position's age runs on
    /// from there, on this iPhone's clock.
    private(set) var answeredAt = Date()
    /// Whether the server has more than it lists at once.
    private(set) var isTruncated = false
    /// Venti's pilots and the other networks' aircraft as the map draws
    /// them, each network with its credit, and how many in words: the
    /// core's `liveLayer` over what is drawn at the map's zoom.
    private(set) var layer = LiveModel.emptyLayer
    /// Whether the map shows too much for the other networks to be asked:
    /// the map then says the core's `othersTooWideWords()`.
    private(set) var othersTooWide = false
    /// The flight being followed: the glider whose page is open, with its
    /// track so far.
    private(set) var followed: LiveFollowView?
    /// When the answer came that it is from.
    private(set) var followedAt = Date()
    /// Why a flight cannot be followed, in words.
    private(set) var followProblem: String?
    /// The glider the map stays on, by its identifier: each new position
    /// of it re-centres the map. Switched on on the glider's page, and off
    /// by the pilot, by the map moved by hand, by the page closing, and
    /// when the glider lands or goes quiet.
    var keptOnMap: String?

    /// Where the map is kept: the position of the glider it stays on, as
    /// the newest answer has it, while that glider flies and is heard of.
    var keptOnPosition: Coordinate? {
        guard let id = keptOnMap, let glider = glider(id), !glider.landed, !glider.quiet else { return nil }
        return glider.position
    }

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }

    @ObservationIgnored private let transport: Transport
    /// The area last asked for, and from which server.
    @ObservationIgnored private var asked: (area: Bounds, server: URL, venti: Bool)?
    @ObservationIgnored private var waiting: Task<Void, Never>?
    /// What the other networks answered last, all of it.
    @ObservationIgnored private var heard = LiveOthers(networks: [], listedMinutes: 10)
    /// The map's zoom, as it last reported it.
    @ObservationIgnored private var zoom = 0.0

    /// From this zoom on the map draws every kind of aircraft of the
    /// other networks; below it only paragliders and hang gliders.
    static let everyKindFromZoom = 10.0

    static let emptyLayer = LiveLayer(venti: [], others: [], credits: [], truncated: false, countWords: "")

    init(transport: @escaping Transport = LiveModel.fetch) {
        self.transport = transport
    }

    /// The map says at which zoom it is: what of the other networks it
    /// draws hangs on it.
    func zoomed(to zoom: Double) {
        guard zoom != self.zoom else { return }
        self.zoom = zoom
        relayer()
    }

    /// The map says which part of it is in view.
    func show(_ region: Bounds) {
        // The core says how much of the map the other networks are asked
        // for; Venti's own pilots are asked for on a smaller one.
        let tooWide = liveOthersPath(
            south: region.south, west: region.west, north: region.north, east: region.east
        ) == nil
        if tooWide != othersTooWide {
            othersTooWide = tooWide
        }
        let asksVenti = region.east - region.west <= Self.widestMap
        guard let server = server(), asksVenti || !tooWide else {
            forget()
            return
        }
        if !asksVenti, !gliders.isEmpty {
            gliders = []
            isTruncated = false
            relayer()
        }
        // Inside what was asked for: the next round brings it up to date.
        if let asked, asked.server == server, asked.venti == asksVenti, asked.area.contains(region) {
            return
        }
        waiting?.cancel()
        waiting = Task { [weak self] in
            // The map reports every pause of a drag; wait until it rests.
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            // A little more than is in view, so that a small drag needs no
            // new request; on a map too wide for Venti's own pilots only
            // what is in view, which the core may still ask the other
            // networks for.
            await self?.load(asksVenti ? region.grown(by: 0.25) : region, from: server, venti: asksVenti)
        }
    }

    /// Asks again every quarter of a minute until the task that runs this
    /// is cancelled, which is when the map leaves the screen.
    func keepCurrent() async {
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(Self.askEvery))
            guard !Task.isCancelled else { return }
            if let asked, let server = server(), asked.server == server {
                await load(asked.area, from: server, venti: asked.venti)
            }
        }
    }

    /// Asks the server who is in the air in an area: Venti's own pilots,
    /// unless `venti` is false, and the aircraft of the other networks.
    /// An answer that does not come leaves the gliders where they were:
    /// their ages go on counting, which is what is known about them.
    func load(_ area: Bounds, from server: URL, venti: Bool = true) async {
        if venti {
            await loadVenti(area, from: server)
        }
        asked = (area, server, venti)
        await loadOthers(area, from: server)
        relayer()
    }

    private func loadVenti(_ area: Bounds, from server: URL) async {
        do {
            let path = try liveMapPath(south: area.south, west: area.west, north: area.north, east: area.east)
            guard let url = URL(string: server.absoluteString + path) else { return }
            let (status, body) = try await transport(url)
            let map = try liveMap(status: UInt16(clamping: status), body: body)
            // The pilot may have changed the server while this was under way.
            guard server == self.server() else { return }
            gliders = map.gliders
            isTruncated = map.truncated
            answeredAt = Date()
        } catch {
            // A server from before live tracks, or none in reach: nothing
            // new is known.
        }
    }

    /// Asks the server which aircraft the other networks hear in an area.
    /// An answer that does not come leaves them as they were; a box the
    /// core asks nothing for has none.
    private func loadOthers(_ area: Bounds, from server: URL) async {
        guard let path = liveOthersPath(south: area.south, west: area.west, north: area.north, east: area.east)
        else {
            heard = LiveOthers(networks: [], listedMinutes: heard.listedMinutes)
            return
        }
        do {
            guard let url = URL(string: server.absoluteString + path) else { return }
            let (status, body) = try await transport(url)
            let others = try liveOthers(status: UInt16(clamping: status), body: body)
            guard server == self.server() else { return }
            heard = others
        } catch {
            // A Venti without the other networks, or none in reach.
        }
    }

    /// The layer anew, from Venti's pilots and what the other networks
    /// heard, at the map's zoom.
    private func relayer() {
        let drawn = Self.drawn(heard, atZoom: zoom)
        let next = liveLayer(venti: LiveMap(gliders: gliders, truncated: isTruncated), others: drawn)
        if next != layer {
            layer = next
        }
    }

    /// What of the other networks' aircraft a map draws at a zoom: the
    /// paragliders and hang gliders always, the other kinds from
    /// `everyKindFromZoom` on. A network the server is not connected to
    /// just now lists only what it heard in the last `listedMinutes`,
    /// each with its age in words: that is drawn too.
    static func drawn(_ others: LiveOthers, atZoom zoom: Double) -> LiveOthers {
        var drawn = others
        drawn.networks = others.networks.map { network in
            var network = network
            if zoom < everyKindFromZoom {
                network.aircraft = network.aircraft.filter(\.isFreeFlight)
            }
            return network
        }
        return drawn
    }

    /// An aircraft of another network as the newest answer has it, with
    /// that network's credit.
    func other(_ id: String) -> (aircraft: OtherAircraftView, credit: Credit?)? {
        for network in heard.networks {
            if let aircraft = network.aircraft.first(where: { $0.id == id }) {
                return (aircraft, network.credit)
            }
        }
        return nil
    }

    /// Who is in the air around a place: for the cockpit, which asks by
    /// itself while a flight is on. Nothing of the answer is kept here.
    /// `nil` without a server, and when no answer came: the cockpit then
    /// leaves the gliders it has where they were.
    func around(_ place: Coordinate, withinDeg reach: Double = 0.35) async -> [LiveGlider]? {
        guard let server = server(),
              let path = try? liveMapPath(
                  south: place.lat - reach, west: place.lon - reach * 1.5,
                  north: place.lat + reach, east: place.lon + reach * 1.5
              ),
              let url = URL(string: server.absoluteString + path),
              let (status, body) = try? await transport(url),
              let map = try? liveMap(status: UInt16(clamping: status), body: body)
        else { return nil }
        return map.gliders
    }

    /// The other networks' aircraft around the glider, for the cockpit:
    /// the core's path (30 km around it) and its reading of the answer.
    /// `nil` when there is no answer.
    func othersForCockpit(around place: Coordinate) async -> LiveOthers? {
        guard let server = server(),
              let path = cockpitOthersPath(lat: place.lat, lon: place.lon),
              let url = URL(string: server.absoluteString + path),
              let (status, body) = try? await transport(url)
        else { return nil }
        return try? liveOthers(status: UInt16(clamping: status), body: body)
    }

    /// How often a followed flight is asked for again, seconds. The
    /// server asks for five to ten.
    static let followEvery = 10.0

    /// Follows one glider's flight until the task that runs this is
    /// cancelled: the whole track first, then what arrives.
    func follow(_ id: String) async {
        followed = nil
        followProblem = nil
        if keptOnMap != id {
            keptOnMap = nil
        }
        defer {
            followed = nil
            if keptOnMap == id {
                keptOnMap = nil
            }
        }
        guard let server = server(), let flight = try? LiveFollow(id: id) else { return }
        while !Task.isCancelled {
            var more = false
            do {
                guard let url = URL(string: server.absoluteString + flight.path()) else { return }
                let (status, body) = try await transport(url)
                let view = try flight.receive(status: UInt16(clamping: status), body: body)
                guard !Task.isCancelled else { return }
                followed = view
                followedAt = Date()
                followProblem = nil
                // A glider that landed or went quiet has no new position
                // for the map to stay on.
                if keptOnMap == id, view.glider.landed || view.glider.quiet {
                    keptOnMap = nil
                }
                more = view.more
            } catch {
                // Gone from the server: said, and not asked for again. Any
                // other failure leaves the track as it was.
                if flight.isOver() {
                    followProblem = error.words
                    if keptOnMap == id {
                        keptOnMap = nil
                    }
                    return
                }
            }
            if !more {
                try? await Task.sleep(for: .seconds(Self.followEvery))
            }
        }
    }

    /// A glider as the newest answer about it has it: the one that
    /// follows its flight, if one does, or else the map's.
    func glider(_ id: String) -> LiveGlider? {
        if let followed, followed.glider.id == id {
            return followed.glider
        }
        return gliders.first { $0.id == id }
    }

    /// How old a glider's position is now, seconds.
    func age(of glider: LiveGlider, at now: Date = Date()) -> Double {
        let since = followed?.glider == glider ? followedAt : answeredAt
        return glider.ageS + max(now.timeIntervalSince(since), 0)
    }

    /// The same in words: "Now", "45 s ago", "Landed 9 min ago".
    func ageInWords(of glider: LiveGlider, at now: Date = Date()) -> String {
        liveAgeInWords(ageS: age(of: glider, at: now), landed: glider.landed)
    }

    private func forget() {
        waiting?.cancel()
        asked = nil
        if !gliders.isEmpty {
            gliders = []
            isTruncated = false
        }
        heard = LiveOthers(networks: [], listedMinutes: heard.listedMinutes)
        relayer()
    }

    /// Fetches with the system's networking, and never an answer kept from
    /// before: a position that is kept is a wrong one.
    nonisolated static func fetch(_ url: URL) async throws -> (Int, Data) {
        var request = URLRequest(url: url)
        request.timeoutInterval = 10
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await URLSession.shared.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, data)
    }
}

/// The pictures the map draws other pilots with.
@MainActor
enum LiveSymbols {
    static let side: CGFloat = 30
    static let flying = "live-flying"
    static let quiet = "live-quiet"
    static let landed = "live-landed"
    /// An aircraft of another network: the same wing, in its own colour.
    static let other = "live-other"

    /// A picture with the name the map knows it by.
    struct Picture {
        let name: String
        let image: UIImage
    }

    /// Green for a glider that is flying: none of the colours of wind,
    /// airspace or the pilot's own tracks. Grey for one that is not, or
    /// that nothing was heard of for a while.
    static var all: [Picture] {
        [
            Picture(name: flying, image: wing(color: Palette.goodOnMap)),
            Picture(name: quiet, image: wing(color: Palette.quietOnMap)),
            Picture(name: landed, image: disc(color: Palette.quietOnMap)),
            Picture(name: other, image: wing(color: Palette.otherOnMap)),
        ]
    }

    /// Which picture a glider is drawn with.
    static func name(for glider: LiveGlider) -> String {
        if glider.landed {
            return landed
        }
        return glider.quiet ? quiet : flying
    }

    /// A paraglider seen from above: its wing as a crescent, the pilot as
    /// a dot behind it. Drawn flying north; the map turns it.
    static func wing(color: UIColor) -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { context in
            let canvas = context.cgContext
            let crescent = UIBezierPath()
            // The leading edge from tip over the middle to tip, then the
            // trailing edge back, flatter.
            crescent.addArc(
                withCenter: CGPoint(x: 15, y: 21), radius: 13,
                startAngle: 3.578, endAngle: 5.847, clockwise: true
            )
            crescent.addArc(
                withCenter: CGPoint(x: 15, y: 37.06), radius: 24.56,
                startAngle: 5.213, endAngle: 4.211, clockwise: false
            )
            crescent.close()
            let pilot = UIBezierPath(ovalIn: CGRect(x: 12.4, y: 17.4, width: 5.2, height: 5.2))

            canvas.setShadow(
                offset: CGSize(width: 0, height: 0.5), blur: 1.5,
                color: UIColor.black.withAlphaComponent(0.35).cgColor
            )
            Palette.haloOnMap.setStroke()
            for shape in [crescent, pilot] {
                shape.lineWidth = 3
                shape.lineJoinStyle = .round
                shape.stroke()
            }
            canvas.setShadow(offset: .zero, blur: 0, color: nil)
            color.setFill()
            crescent.fill()
            pilot.fill()
        }
    }

    /// A glider on the ground: a small disc.
    static func disc(color: UIColor) -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { context in
            let disc = UIBezierPath(ovalIn: CGRect(x: 9.5, y: 9.5, width: 11, height: 11))
            context.cgContext.setShadow(
                offset: CGSize(width: 0, height: 0.5), blur: 1.5,
                color: UIColor.black.withAlphaComponent(0.35).cgColor
            )
            color.setFill()
            disc.fill()
            context.cgContext.setShadow(offset: .zero, blur: 0, color: nil)
            Palette.haloOnMap.setStroke()
            disc.lineWidth = 2
            disc.stroke()
        }
    }
}
