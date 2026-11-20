import Foundation
import Observation
import UIKit
import VentiCore

/// Who is in the air: the gliders whose pilots show their flight to
/// everyone, from the Venti server the pilot has set, for the part of the
/// map in view.
///
/// What to ask the server and how to read its answer is the core's. This
/// class sends the request, and again every quarter of a minute while the
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
    /// The flight being followed: the glider whose page is open, with its
    /// track so far.
    private(set) var followed: LiveFollowView?
    /// When the answer came that it is from.
    private(set) var followedAt = Date()
    /// Why a flight cannot be followed, in words.
    private(set) var followProblem: String?

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }

    @ObservationIgnored private let transport: Transport
    /// The area last asked for, and from which server.
    @ObservationIgnored private var asked: (area: Bounds, server: URL)?
    @ObservationIgnored private var waiting: Task<Void, Never>?

    init(transport: @escaping Transport = LiveModel.fetch) {
        self.transport = transport
    }

    /// The map says which part of it is in view.
    func show(_ region: Bounds) {
        guard let server = server(), region.east - region.west <= Self.widestMap else {
            forget()
            return
        }
        // Inside what was asked for: the next round brings it up to date.
        if let asked, asked.server == server, asked.area.contains(region) {
            return
        }
        waiting?.cancel()
        waiting = Task { [weak self] in
            // The map reports every pause of a drag; wait until it rests.
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            // A little more than is in view, so that a small drag needs no
            // new request.
            await self?.load(region.grown(by: 0.25), from: server)
        }
    }

    /// Asks again every quarter of a minute until the task that runs this
    /// is cancelled, which is when the map leaves the screen.
    func keepCurrent() async {
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(Self.askEvery))
            guard !Task.isCancelled else { return }
            if let asked, let server = server(), asked.server == server {
                await load(asked.area, from: server)
            }
        }
    }

    /// Asks the server who is in the air in an area. An answer that does
    /// not come leaves the gliders where they were: their ages go on
    /// counting, which is what is known about them.
    func load(_ area: Bounds, from server: URL) async {
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
            asked = (area, server)
        } catch {
            // A server from before live tracks, or none in reach: nothing
            // new is known.
            asked = asked ?? (area, server)
        }
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

    /// How often a followed flight is asked for again, seconds. The
    /// server asks for five to ten.
    static let followEvery = 10.0

    /// Follows one glider's flight until the task that runs this is
    /// cancelled: the whole track first, then what arrives.
    func follow(_ id: String) async {
        followed = nil
        followProblem = nil
        defer { followed = nil }
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
                more = view.more
            } catch {
                // Gone from the server: said, and not asked for again. Any
                // other failure leaves the track as it was.
                if flight.isOver() {
                    followProblem = error.words
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
            Picture(name: flying, image: wing(color: UIColor(hex: 0x157F4B))),
            Picture(name: quiet, image: wing(color: UIColor(hex: 0x85847F))),
            Picture(name: landed, image: disc(color: UIColor(hex: 0x85847F))),
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
            UIColor.white.setStroke()
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
            UIColor.white.setStroke()
            disc.lineWidth = 2
            disc.stroke()
        }
    }
}
