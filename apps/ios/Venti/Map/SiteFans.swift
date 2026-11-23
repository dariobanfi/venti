import CoreLocation
import MapLibre
import UIKit
import VentiCore

/// The directions a take-off works in, drawn on the map tab as a small fan
/// round the head of its pin, as Burnair draws them (plan item 19; the
/// owner, 2026-11-23). The spans are the source's (`SiteView.wind`, the
/// API's `wind_directions`); whether a wind fits them is the core's
/// (`windFits`). A fan points where the wind comes from: a take-off that
/// works in a north wind has its fan to the north.
///
/// Its colour says whether the wind of the hour the map shows fits, where
/// the map shows an hour of the forecast's wind: the pack's good where it
/// fits, a hollow fan in the muted ink where it does not, so that the two
/// differ in more than their colour. Anywhere else, on the wind as
/// measured, on another layer and with nothing chosen, every fan is the
/// pin's own dark. A landing has no fan: its pin, the pale one, is its
/// sign.
///
/// The cockpit's map draws the take-offs it holds with the same fans, in
/// one neutral colour of the flight theme (`Fit.inFlight`): there is no
/// hour of the forecast in flight.
@MainActor
enum SiteFans {
    /// The symbol layer, under the sites' signs, from the sites' source.
    static let layer = "flying-site-fans"

    /// Whether the wind of the hour fits the take-off.
    enum Fit: String {
        case fits
        case doesNotFit = "off"
        /// No hour of the forecast's wind on the map, or none at the place.
        case unknown = "plain"
        /// On the cockpit's map: one neutral colour, whatever the wind.
        case inFlight = "flight"
    }

    /// The fan's picture: a circle as wide as a finger.
    static let size = CGSize(width: 52, height: 52)
    /// How far the fan reaches from its middle, points.
    static let radius: CGFloat = 24
    /// How far above the site's place the middle of the pin's head stands
    /// (`SiteSymbols.picture(for:)`: the head's radius and 3 points over
    /// it, the tip at the bottom).
    static let headAboveTip: CGFloat = SiteSymbols.size.height - 13

    /// Half of one of the sixteen points of the compass: a single direction
    /// is drawn that wide on each side, and a span's ends too, so that "N
    /// to NE" covers the two points whole.
    nonisolated static let halfPoint = 11.25

    /// Whether a site gets a fan: one whose source gives the directions
    /// it works in, and that is not a landing. Directions are a take-off's
    /// word, also where the site's kind is "other": OpenStreetMap's
    /// take-offs in the Lechtal, mapped without their use, give them.
    nonisolated static func hasFan(_ site: SiteView) -> Bool {
        !site.wind.isEmpty && site.kind != .landing
    }

    /// The wedges of a fan, degrees clockwise from north, each `from` to
    /// `to` going clockwise with `to` above `from` (it may pass 360).
    nonisolated static func wedges(of spans: [WindSpan]) -> [(from: Double, to: Double)] {
        spans.map { span in
            var to = span.toDeg
            if to < span.fromDeg {
                to += 360
            }
            let from = span.fromDeg - halfPoint
            return (from, min(to + halfPoint, from + 360))
        }
    }

    /// The name the map knows a fan's picture by: the same spans in the
    /// same state are one picture.
    nonisolated static func name(of spans: [WindSpan], fit: Fit) -> String {
        "site-fan-\(fit.rawValue)-"
            + spans.map { "\(Int($0.fromDeg.rounded()))-\(Int($0.toDeg.rounded()))" }.joined(separator: "_")
    }

    /// Where the wind of an hour comes from, square by square: the
    /// arrows of the core's `ThermalMapShapes`, whose `heading` is where
    /// the wind blows to. A calm square has no direction and is left out.
    nonisolated static func windFrom(arrows: String) -> [UInt32: Double] {
        guard let data = arrows.data(using: .utf8),
              let collection = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let features = collection["features"] as? [[String: Any]]
        else { return [:] }
        var from: [UInt32: Double] = [:]
        for feature in features {
            guard let properties = feature["properties"] as? [String: Any],
                  properties["symbol"] as? String == "arrow",
                  let cell = (properties["cell"] as? NSNumber)?.uint32Value,
                  let heading = (properties["heading"] as? NSNumber)?.doubleValue
            else { continue }
            from[cell] = (heading + 180).truncatingRemainder(dividingBy: 360)
        }
        return from
    }

    /// Whether the wind fits a site, by the core: `.unknown` where there
    /// is no wind for the place.
    static func fit(of site: SiteView, windFrom: Double?) -> Fit {
        guard let windFrom, let fits = windFits(spans: site.wind, fromDeg: windFrom) else { return .unknown }
        return fits ? .fits : .doesNotFit
    }

    /// Adds the fans' layer to a style, reading the sites' source. Call it
    /// before adding the sites' signs, which stand on the fans. `scale` is
    /// the signs' own (`iconScale`): MapLibre scales an icon's offset with
    /// the icon, so the fan stays round the smaller pin's head.
    static func add(to style: MLNStyle, source: MLNSource, scale: Double = 1) {
        let fans = MLNSymbolStyleLayer(identifier: layer, source: source)
        if scale != 1 {
            fans.iconScale = NSExpression(forConstantValue: scale)
        }
        fans.predicate = NSPredicate(format: "fan != ''")
        fans.iconImageName = NSExpression(forKeyPath: "fan")
        fans.iconAllowsOverlap = NSExpression(forConstantValue: true)
        fans.iconIgnoresPlacement = NSExpression(forConstantValue: true)
        fans.iconAnchor = NSExpression(forConstantValue: "center")
        fans.minimumZoomLevel = 9
        style.addLayer(fans)
        turn(fans, to: 0)
    }

    /// Keeps the fans' north on the map's north while the map is turned,
    /// round the head of the pin, which stays upright on the screen. The
    /// picture is turned against the map's direction; MapLibre turns an
    /// icon's offset with the icon, so the offset is turned back by as
    /// much and the middle stays on the head.
    static func turn(_ fans: MLNSymbolStyleLayer, to direction: CLLocationDirection) {
        fans.iconRotation = NSExpression(forConstantValue: -direction)
        let offset = offset(for: direction)
        fans.iconOffset = NSExpression(forConstantValue: NSValue(cgVector: offset))
    }

    /// The icon offset that puts the fan's middle on the pin's head with
    /// the icon turned by `-direction`.
    static func offset(for direction: CLLocationDirection) -> CGVector {
        let radians = direction * .pi / 180
        return CGVector(dx: headAboveTip * sin(radians), dy: -headAboveTip * cos(radians))
    }

    /// The picture of a fan, north up.
    static func picture(of spans: [WindSpan], fit: Fit) -> UIImage {
        let (fill, edge): (UIColor?, UIColor) =
            switch fit {
            case .fits: (Palette.goodOnMap, Palette.haloOnMap)
            case .doesNotFit: (nil, Palette.notFlyableOnMap)
            case .unknown: (Palette.siteOnMap, Palette.haloOnMap)
            case .inFlight: (Palette.siteFanInFlightOnMap, Palette.haloOnMap)
            }
        return UIGraphicsImageRenderer(size: size).image { _ in
            let centre = CGPoint(x: size.width / 2, y: size.height / 2)
            for wedge in wedges(of: spans) {
                // Degrees clockwise from north to the screen's angle, which
                // runs clockwise from the right.
                let start = (wedge.from - 90) * .pi / 180
                let end = (wedge.to - 90) * .pi / 180
                let path = UIBezierPath()
                path.move(to: centre)
                path.addArc(withCenter: centre, radius: radius, startAngle: start, endAngle: end, clockwise: true)
                path.close()
                path.lineJoinStyle = .round
                if let fill {
                    fill.setFill()
                    path.fill()
                    path.lineWidth = 1.5
                } else {
                    Palette.haloOnMap.setFill()
                    path.fill()
                    path.lineWidth = 2
                }
                edge.setStroke()
                path.stroke()
            }
        }
    }
}
