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

    // What is drawn on a map takes the light theme's values: the map is
    // light in both appearances.

    /// The accent as drawn on the map: a flight's line, the pilot's plans.
    static var accentOnMap: UIColor { UIColor(VentiTheme.light.primary) }

    /// The near-black of dots, dashes, casings and words on the map: the
    /// design language's map ink (`--map-label`, `--map-track-casing`).
    static var inkOnMap: UIColor { UIColor(VentiBrand.ink) }

    /// The halo around words on the map and the edge under its lines and
    /// signs (`--map-label-halo`). The pack's Swift tokens leave the map's
    /// colours out; this one has the value of the light theme's paper.
    static var haloOnMap: UIColor { UIColor(VentiTheme.light.bg) }

    /// What is well on the map: a glider that is flying, the ground
    /// within reach.
    static var goodOnMap: UIColor { UIColor(VentiTheme.light.good) }

    /// What is not flyable on the map: the forecast map's squares of
    /// such a day, and their line in its key, as a quiet veil in the
    /// muted ink. Not the danger colour, which is nearly the thermal
    /// scale's darkest step: the best day and the worst would look alike.
    static var notFlyableOnMap: UIColor { UIColor(VentiTheme.light.textMuted) }

    /// A flying site on the map: the dark of its pin, and of the outline
    /// and the faint fill of a site that is mapped as an area.
    static var siteOnMap: UIColor { UIColor(VentiTheme.light.primary) }

    /// A take-off that says nothing to what the filter of the sites asks,
    /// shown apart from the ones that fit: the pack's pair for something
    /// that is not to be chosen (`.v-btn[disabled]`), the muted ink on the
    /// raised surface.
    static var siteUnsaidOnMap: UIColor { UIColor(VentiTheme.light.textMuted) }
    static var siteUnsaidGroundOnMap: UIColor { UIColor(VentiTheme.light.surfaceRaised) }

    /// A place where pilots take off and no site stands: a small ring in
    /// the muted ink, hollow on the map's paper, so that it is not taken
    /// for a site's pin.
    static var candidateOnMap: UIColor { UIColor(VentiTheme.light.textMuted) }

    /// What has gone quiet on the map: an old reading, a glider nothing
    /// was heard of for a while.
    static var quietOnMap: UIColor { UIColor(VentiTheme.light.borderStrong) }

    /// An aircraft another network hears (the Open Glider Network): the
    /// muted ink, darker than what has gone quiet and not the green of
    /// Venti's own pilots, so that the two are told apart at a glance.
    static var otherOnMap: UIColor { UIColor(VentiTheme.light.textMuted) }

    /// The pilot's own glider on the map: cream with a dark ring, the
    /// pairing of the dot in the logo (`--map-own-position` and its ring).
    static var ownPositionOnMap: UIColor { UIColor(VentiBrand.cream) }
    static var ownPositionRingOnMap: UIColor { UIColor(VentiBrand.indigo) }

    /// Mean wind in a chart: the fifth of the seven steps of the design
    /// language's wind scale, by the appearance of where it is drawn.
    static var wind: Color {
        Color(uiColor: UIColor { traits in UIColor(VentiTheme.of(traits).wind[4]) })
    }

    /// The span from mean wind up to the gusts: the same scale's second
    /// step, held back.
    static var gusts: Color {
        Color(uiColor: UIColor { traits in UIColor(VentiTheme.of(traits).wind[1]) })
    }
}

/// How strong a forecast's thermals are, in the six steps of the design
/// language's thermal scale: pale where there is little to climb in, deep
/// coral where the climb is strong.
///
/// The steps and their colours are the pack's (`VentiScaleBreaks.thermal`,
/// `VentiTheme.thermal`): 0.5, 1, 1.5, 2 and 3 m/s.
struct ThermalBand: Equatable, Sendable {
    /// The step, 0 for the weakest to 5.
    let step: Int

    init(metresPerSecond climb: Double) {
        let breaks = VentiScaleBreaks.thermal
        step = breaks.firstIndex { climb < $0 } ?? breaks.count
    }

    /// The step the core names for a kind of day on the thermal map,
    /// counted as the pack counts them, 1 to 6 (`thermal-1…6`).
    init(scaleStep: UInt8) {
        step = min(max(Int(scaleStep) - 1, 0), VentiScaleBreaks.thermal.count)
    }

    /// The step's colour, by the appearance of where it is drawn.
    var color: Color {
        Color(uiColor: UIColor { traits in UIColor(VentiTheme.of(traits).thermal[step]) })
    }

    /// The step's colour on a map, and in the key beside one: the light
    /// theme's, as the map is light in both appearances.
    var colorOnMap: Color { VentiTheme.light.thermal[step] }

    var uiColorOnMap: UIColor { UIColor(colorOnMap) }
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
