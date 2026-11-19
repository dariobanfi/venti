import SwiftUI
import UIKit
import VentiCore

/// How strong the wind is, in four bands of its mean speed.
///
/// One hue from light to dark: strength is shown as a magnitude, not as a
/// verdict. Whether 25 km/h is too much depends on the pilot, the wing and
/// the place, and the app does not decide that from one number. The steps
/// were checked to be told apart by colour-blind pilots.
enum WindBand: Int, CaseIterable, Sendable {
    case light
    case moderate
    case fresh
    case strong

    init(metresPerSecond speed: Double) {
        switch speed * 3.6 {
        case ..<10: self = .light
        case ..<20: self = .moderate
        case ..<30: self = .fresh
        default: self = .strong
        }
    }

    var uiColor: UIColor {
        switch self {
        case .light: UIColor(hex: 0x86B6EF)
        case .moderate: UIColor(hex: 0x3987E5)
        case .fresh: UIColor(hex: 0x1C5CAB)
        case .strong: UIColor(hex: 0x0D366B)
        }
    }

    var color: Color { Color(uiColor: uiColor) }

    /// The band in words, for the legend.
    var label: String {
        switch self {
        case .light: "Below 10 km/h"
        case .moderate: "10 to 20 km/h"
        case .fresh: "20 to 30 km/h"
        case .strong: "30 km/h and more"
        }
    }

    /// How large the band's arrow is drawn, relative to the smallest.
    var arrowScale: Double {
        switch self {
        case .light: 0.85
        case .moderate: 1.0
        case .fresh: 1.15
        case .strong: 1.3
        }
    }
}

/// How one station is drawn on the map and read in a list.
struct WindMark: Equatable {
    enum Symbol: Equatable {
        /// An arrow in the band's colour, pointing where the wind blows.
        case arrow(WindBand)
        /// Wind too weak to have a direction worth drawing.
        case calm
        /// A reading too old to stand for the wind now, or none at all.
        case old
    }

    let symbol: Symbol
    /// Where the arrow points, in degrees clockwise from north: the
    /// direction the wind blows towards.
    let heading: Double
    /// Mean wind and gusts in km/h, "12/25", or just "12" without gusts.
    /// Empty for a station with nothing current to say.
    let label: String

    /// Below this the wind has no direction worth an arrow.
    static let calmBelowMs = 0.5

    init(_ station: WindStation) {
        guard let reading = station.latest, !station.isOld, let speed = reading.speedMs else {
            symbol = .old
            heading = 0
            label = ""
            return
        }
        label = Self.speeds(mean: speed, gust: reading.gustMs)
        if let from = reading.directionDeg, speed >= Self.calmBelowMs {
            symbol = .arrow(WindBand(metresPerSecond: speed))
            heading = (from + 180).truncatingRemainder(dividingBy: 360)
        } else {
            symbol = .calm
            heading = 0
        }
    }

    /// "12/25": mean wind and gusts in whole km/h. Gusts are left out when
    /// they are not known or round to the same number.
    static func speeds(mean: Double, gust: Double?) -> String {
        let mean = Format.kilometresPerHour(metresPerSecond: mean)
        guard let gust else { return mean }
        let gusts = Format.kilometresPerHour(metresPerSecond: gust)
        return gusts == mean ? mean : "\(mean)/\(gusts)"
    }
}

/// The pictures the map draws stations with.
@MainActor
enum WindSymbols {
    static let side: CGFloat = 30

    static func name(of symbol: WindMark.Symbol) -> String {
        switch symbol {
        case .arrow(let band): "wind-arrow-\(band.rawValue)"
        case .calm: "wind-calm"
        case .old: "wind-old"
        }
    }

    /// A picture with the name the map knows it by.
    struct Picture {
        let name: String
        let image: UIImage
    }

    static var all: [Picture] {
        var pictures = WindBand.allCases.map { band in
            Picture(name: name(of: .arrow(band)), image: arrow(color: band.uiColor))
        }
        pictures.append(Picture(name: name(of: .calm), image: dot(color: WindBand.light.uiColor)))
        pictures.append(Picture(name: name(of: .old), image: dot(color: UIColor(hex: 0xA9A8A2))))
        return pictures
    }

    /// A dart pointing up, with a white edge so it stands out on any ground.
    /// The map turns it to where the wind blows.
    static func arrow(color: UIColor) -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { context in
            let dart = UIBezierPath()
            dart.move(to: CGPoint(x: side * 0.5, y: side * 0.13))
            dart.addLine(to: CGPoint(x: side * 0.78, y: side * 0.87))
            dart.addLine(to: CGPoint(x: side * 0.5, y: side * 0.69))
            dart.addLine(to: CGPoint(x: side * 0.22, y: side * 0.87))
            dart.close()
            dart.lineJoinStyle = .round
            dart.lineWidth = 3.5

            let canvas = context.cgContext
            canvas.setShadow(
                offset: CGSize(width: 0, height: 0.5), blur: 1.5,
                color: UIColor.black.withAlphaComponent(0.35).cgColor
            )
            UIColor.white.setStroke()
            dart.stroke()
            canvas.setShadow(offset: .zero, blur: 0, color: nil)
            color.setFill()
            dart.fill()
        }
    }

    /// A small disc with a white edge.
    static func dot(color: UIColor) -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { context in
            let disc = UIBezierPath(ovalIn: CGRect(x: side / 2 - 4.5, y: side / 2 - 4.5, width: 9, height: 9))
            disc.lineWidth = 3.5

            let canvas = context.cgContext
            canvas.setShadow(
                offset: CGSize(width: 0, height: 0.5), blur: 1.5,
                color: UIColor.black.withAlphaComponent(0.35).cgColor
            )
            UIColor.white.setStroke()
            disc.stroke()
            canvas.setShadow(offset: .zero, blur: 0, color: nil)
            color.setFill()
            disc.fill()
        }
    }
}
