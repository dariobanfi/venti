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
        UIColor { traits in UIColor(VentiTheme.of(traits).primary) }
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

/// How fast the glider was climbing or sinking, in the seven steps of the
/// design language's vario scale: teal where it sank, grey where it held
/// its height, coral where it climbed. "Cool is down, warm is up."
///
/// The steps and their colours are the pack's (`VentiScaleBreaks.vario`,
/// `VentiTheme.vario`): -3, -1.5, -0.5, +0.5, +1.5 and +3 m/s.
enum ClimbBand: Int, CaseIterable, Sendable {
    case strongSink
    case sink
    case lightSink
    case level
    case lightClimb
    case climb
    case strongClimb

    init(metresPerSecond rate: Double) {
        let breaks = VentiScaleBreaks.vario
        let step = breaks.firstIndex { rate < $0 } ?? breaks.count
        self = ClimbBand(rawValue: step) ?? .level
    }

    /// The step's colour on a map. The map is pale in both appearances,
    /// so it is the light theme's.
    var uiColor: UIColor { UIColor(VentiTheme.light.vario[rawValue]) }

    /// The step's colour in a chart or a legend, by the appearance of
    /// where it is drawn.
    var color: Color {
        Color(uiColor: UIColor { traits in UIColor(VentiTheme.of(traits).vario[rawValue]) })
    }

    /// The band in words, for the legend.
    var label: String {
        switch self {
        case .strongSink: "Sinking faster than 3 m/s"
        case .sink: "Sinking 1.5 to 3 m/s"
        case .lightSink: "Sinking 0.5 to 1.5 m/s"
        case .level: "Within 0.5 m/s of level"
        case .lightClimb: "Climbing 0.5 to 1.5 m/s"
        case .climb: "Climbing 1.5 to 3 m/s"
        case .strongClimb: "Climbing faster than 3 m/s"
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
