import SwiftUI
import UIKit
import VentiCore

/// How the core's four bands of wind strength are drawn.
///
/// One hue from light to dark, the design language's wind scale: strength
/// is shown as a magnitude, not as a verdict. Whether 25 km/h is too much
/// depends on the pilot, the wing and the place, and the app does not
/// decide that from one number. Which speed falls in which band, and how a
/// station is marked, is the core's decision.
///
/// The pack's scale has seven steps, one every 5 km/h; the core sorts a
/// speed into four bands (below 10, 10 to 20, 20 to 30, 30 km/h and more).
/// Each band takes the pack's step for the middle of its span: the second,
/// fourth, sixth and seventh. The pack's own breaks wait for the core to
/// sort by them.
extension WindBand {
    /// Every band, the weakest first.
    static var all: [WindBand] { [.light, .moderate, .fresh, .strong] }

    /// Which of the pack's seven steps of wind the band is drawn in,
    /// from 0.
    var step: Int {
        switch self {
        case .light: 1
        case .moderate: 3
        case .fresh: 5
        case .strong: 6
        }
    }

    /// The band's colour on the map, which is light in both appearances.
    var uiColor: UIColor { UIColor(VentiTheme.light.wind[step]) }

    var color: Color { Color(uiColor: uiColor) }

    /// The band's colour for a list or a table, which follow the iPhone's
    /// appearance: the pack's dark scale runs the other way, dim for light
    /// wind and bright for strong, so that every step still stands out.
    var ink: Color {
        let step = step
        return Color(uiColor: UIColor { traits in UIColor(VentiTheme.of(traits).wind[step]) })
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
        pictures.append(Picture(name: name(of: .old), image: dot(color: Palette.quietOnMap)))
        return pictures
    }

    /// A dart pointing up, with a pale edge so it stands out on any ground.
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
            Palette.haloOnMap.setStroke()
            dart.stroke()
            canvas.setShadow(offset: .zero, blur: 0, color: nil)
            color.setFill()
            dart.fill()
        }
    }

    /// A small disc with a pale edge.
    static func dot(color: UIColor) -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { context in
            let disc = UIBezierPath(ovalIn: CGRect(x: side / 2 - 4.5, y: side / 2 - 4.5, width: 9, height: 9))
            disc.lineWidth = 3.5

            let canvas = context.cgContext
            canvas.setShadow(
                offset: CGSize(width: 0, height: 0.5), blur: 1.5,
                color: UIColor.black.withAlphaComponent(0.35).cgColor
            )
            Palette.haloOnMap.setStroke()
            disc.stroke()
            canvas.setShadow(offset: .zero, blur: 0, color: nil)
            color.setFill()
            disc.fill()
        }
    }
}
