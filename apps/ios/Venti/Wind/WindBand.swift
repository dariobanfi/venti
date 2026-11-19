import SwiftUI
import UIKit
import VentiCore

/// How the core's four bands of wind strength are drawn.
///
/// One hue from light to dark: strength is shown as a magnitude, not as a
/// verdict. Whether 25 km/h is too much depends on the pilot, the wing and
/// the place, and the app does not decide that from one number. The steps
/// were checked to be told apart by colour-blind pilots. Which speed falls
/// in which band, and how a station is marked, is the core's decision.
extension WindBand {
    /// Every band, the weakest first.
    static var all: [WindBand] { [.light, .moderate, .fresh, .strong] }

    var uiColor: UIColor {
        switch self {
        case .light: UIColor(hex: 0x86B6EF)
        case .moderate: UIColor(hex: 0x3987E5)
        case .fresh: UIColor(hex: 0x1C5CAB)
        case .strong: UIColor(hex: 0x0D366B)
        }
    }

    var color: Color { Color(uiColor: uiColor) }

    /// The band's colour for a list or a table, which follow the iPhone's
    /// appearance: on a dark surface the scale runs the other way, dim for
    /// light wind and bright for strong, so that every step still stands
    /// out. Checked as an ordered ramp against both surfaces. The map
    /// keeps `uiColor`: its arrows have a white edge and lie on a map that
    /// is light in both appearances.
    var ink: Color {
        let dark: UIColor =
            switch self {
            case .light: UIColor(hex: 0x256ABF)
            case .moderate: UIColor(hex: 0x5598E7)
            case .fresh: UIColor(hex: 0x9CC4F3)
            case .strong: UIColor(hex: 0xE3EEFC)
            }
        let light = uiColor
        return Color(uiColor: UIColor { traits in
            traits.userInterfaceStyle == .dark ? dark : light
        })
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

    /// The band in the name of its picture.
    fileprivate var name: String {
        switch self {
        case .light: "light"
        case .moderate: "moderate"
        case .fresh: "fresh"
        case .strong: "strong"
        }
    }
}

/// The pictures the map draws stations with.
@MainActor
enum WindSymbols {
    static let side: CGFloat = 30

    static func name(of symbol: WindSymbol) -> String {
        switch symbol {
        case .arrow(let band): "wind-arrow-\(band.name)"
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
        var pictures = WindBand.all.map { band in
            Picture(name: name(of: .arrow(band: band)), image: arrow(color: band.uiColor))
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
