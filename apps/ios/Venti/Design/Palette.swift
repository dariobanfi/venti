import SwiftUI
import UIKit

/// The app's colours. `docs/design.md` explains the choices.
///
/// Everything else on screen uses the system's own colours and materials, so
/// the app follows light and dark mode, increased contrast and whatever the
/// pilot has set for transparency.
enum Palette {
    /// The one accent: the main button, selected controls, and a flight's
    /// line in lists, charts and on the overview map. Violet, because it is
    /// none of the colours that carry meaning on a track.
    static var accentUI: UIColor {
        UIColor { traits in
            traits.userInterfaceStyle == .dark ? UIColor(hex: 0x9085E9) : UIColor(hex: 0x6250D6)
        }
    }

    static var accent: Color { Color(uiColor: accentUI) }

    /// The accent as drawn on the map, which is light in both appearances.
    static var accentOnMap: UIColor { UIColor(hex: 0x6250D6) }

    /// Mean wind in a chart: one blue, a step lighter on a dark ground.
    static var wind: Color {
        Color(uiColor: UIColor { traits in
            traits.userInterfaceStyle == .dark ? UIColor(hex: 0x6DA7EC) : UIColor(hex: 0x256ABF)
        })
    }

    /// The span from mean wind up to the gusts, in the same blue held back.
    static var gusts: Color {
        Color(uiColor: UIColor { traits in
            traits.userInterfaceStyle == .dark ? UIColor(hex: 0x1C5CAB) : UIColor(hex: 0xB7D3F6)
        })
    }
}

/// How fast the glider was climbing or sinking, in five bands.
///
/// A track is coloured by band: blue where it sank, grey where it held its
/// height, orange and red where it climbed. Two hues with a neutral middle
/// stay distinguishable for colour-blind pilots; the palette was checked for
/// that.
enum ClimbBand: Int, CaseIterable, Sendable {
    case strongSink
    case sink
    case level
    case climb
    case strongClimb

    init(metresPerSecond rate: Double) {
        switch rate {
        case ..<(-2.0): self = .strongSink
        case ..<(-0.5): self = .sink
        case ..<0.5: self = .level
        case ..<2.0: self = .climb
        default: self = .strongClimb
        }
    }

    var uiColor: UIColor {
        switch self {
        case .strongSink: UIColor(hex: 0x184F95)
        case .sink: UIColor(hex: 0x3987E5)
        case .level: UIColor(hex: 0x85847F)
        case .climb: UIColor(hex: 0xEB6834)
        case .strongClimb: UIColor(hex: 0xB8242B)
        }
    }

    var color: Color { Color(uiColor: uiColor) }

    /// The band in words, for the legend.
    var label: String {
        switch self {
        case .strongSink: "Sinking faster than 2 m/s"
        case .sink: "Sinking 0.5 to 2 m/s"
        case .level: "Within 0.5 m/s of level"
        case .climb: "Climbing 0.5 to 2 m/s"
        case .strongClimb: "Climbing faster than 2 m/s"
        }
    }
}

extension UIColor {
    /// A colour from its hexadecimal RGB value, for example `0x6250D6`.
    convenience init(hex: UInt32) {
        self.init(
            red: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: 1
        )
    }
}
