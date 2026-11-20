import SwiftUI
import UIKit

// The owner's design language in the app.
//
// The tokens are `design/tokens/VentiTheme.swift`, compiled into the app as
// the pack has them, and the typeface is the pack's Fira Sans. This file is
// what the app adds to use them: which theme a view is in, colours by their
// role, type that grows with the pilot's text size, and the surfaces of a
// list. `design/DESIGN.md` says what each token is for.

extension EnvironmentValues {
    /// The theme a view is drawn in. `ventiThemed()` sets it from the
    /// iPhone's appearance; the cockpit sets the flight theme.
    @Entry var venti: VentiTheme = .light

    /// Whether a view is in the cockpit, in flight: its buttons are larger,
    /// for gloves, and what floats over the map has an edge and no shadow.
    @Entry var ventiInFlight = false
}

extension VentiTheme {
    /// The theme for an appearance of the system.
    static func of(_ scheme: ColorScheme) -> VentiTheme {
        scheme == .dark ? .dark : .light
    }

    /// The same for UIKit, which draws the bars.
    static func of(_ traits: UITraitCollection) -> VentiTheme {
        traits.userInterfaceStyle == .dark ? .dark : .light
    }

    /// The theme's colour for a role.
    func color(_ role: VentiColor.Role) -> Color {
        switch role {
        case .bg: bg
        case .surface: surface
        case .surfaceRaised: surfaceRaised
        case .border: border
        case .borderStrong: borderStrong
        case .text: text
        case .textSecondary: textSecondary
        case .textMuted: textMuted
        case .primary: primary
        case .primaryPressed: primaryPressed
        case .onPrimary: onPrimary
        case .accent: accent
        case .onAccent: onAccent
        case .accentText: accentText
        case .link: link
        case .focus: focus
        case .good: good
        case .goodSoft: goodSoft
        case .caution: caution
        case .cautionSoft: cautionSoft
        case .danger: danger
        case .dangerSoft: dangerSoft
        case .dangerFill: dangerFill
        case .onDangerFill: onDangerFill
        }
    }
}

/// A colour of the design language by its role, resolved in the theme of
/// wherever it is drawn: `.foregroundStyle(.venti(.accentText))`,
/// `.background(.venti(.surface))`. A view names the role and never a
/// colour.
struct VentiColor: ShapeStyle {
    enum Role: Sendable {
        case bg, surface, surfaceRaised
        case border, borderStrong
        case text, textSecondary, textMuted
        case primary, primaryPressed, onPrimary
        case accent, onAccent, accentText
        case link, focus
        case good, goodSoft, caution, cautionSoft, danger, dangerSoft
        case dangerFill, onDangerFill
    }

    let role: Role

    func resolve(in environment: EnvironmentValues) -> Color {
        environment.venti.color(role)
    }
}

extension ShapeStyle where Self == VentiColor {
    /// The theme's colour for a role.
    static func venti(_ role: VentiColor.Role) -> VentiColor {
        VentiColor(role: role)
    }
}

/// The pack's type styles. `VentiType` in the pack has them as fonts of a
/// fixed size; here they grow with the text size the pilot has set, as
/// `design/DESIGN.md` asks. The sizes are the pack's.
enum VentiText {
    case display, hero, title, heading, body, label, caption
    case instrumentXL, instrumentL, instrumentM

    var font: Font {
        switch self {
        case .display: .custom("FiraSans-BlackItalic", size: 56, relativeTo: .largeTitle)
        case .hero: .custom("FiraSans-BlackItalic", size: 36, relativeTo: .largeTitle)
        case .title: .custom("FiraSans-SemiBold", size: 26, relativeTo: .title)
        case .heading: .custom("FiraSans-SemiBold", size: 19, relativeTo: .title3)
        case .body: .custom("FiraSans-Regular", size: 16, relativeTo: .body)
        case .label: .custom("FiraSans-Medium", size: 14, relativeTo: .subheadline)
        case .caption: .custom("FiraSans-Regular", size: 12, relativeTo: .caption)
        case .instrumentXL: .custom("FiraSans-Bold", size: 84, relativeTo: .largeTitle).monospacedDigit()
        case .instrumentL: .custom("FiraSans-Bold", size: 44, relativeTo: .largeTitle).monospacedDigit()
        case .instrumentM: .custom("FiraSans-SemiBold", size: 24, relativeTo: .title2).monospacedDigit()
        }
    }
}

extension Font {
    /// One of the pack's type styles.
    static func ventiType(_ text: VentiText) -> Font {
        text.font
    }

    /// The pack's type in place of one of the system's text styles: the
    /// pack's size for that role, in Fira Sans, growing with the pilot's
    /// text size. Titles and headlines are semibold unless a weight is
    /// named.
    static func venti(_ style: Font.TextStyle, _ weight: Font.Weight? = nil) -> Font {
        let size: CGFloat
        var usual = Font.Weight.regular
        switch style {
        case .largeTitle, .title:
            size = 26
            usual = .semibold
        case .title2, .title3:
            size = 19
            usual = .semibold
        case .headline:
            size = 16
            usual = .semibold
        case .body, .callout:
            size = 16
        case .subheadline:
            size = 14
        default:
            size = 12
        }
        return .custom(face(weight ?? usual), size: size, relativeTo: style)
    }

    /// The file of the typeface nearest to a weight.
    private static func face(_ weight: Font.Weight) -> String {
        switch weight {
        case .medium: "FiraSans-Medium"
        case .semibold: "FiraSans-SemiBold"
        case .bold, .heavy, .black: "FiraSans-Bold"
        default: "FiraSans-Regular"
        }
    }
}

extension View {
    /// Draws what is inside in the design language: the light or the dark
    /// theme by the iPhone's appearance, its ink in three steps for what
    /// the system calls primary, secondary and tertiary, and its type.
    func ventiThemed() -> some View {
        modifier(Themed(isInFlight: false))
    }

    /// The same in the flight theme, whatever the iPhone is set to: for
    /// the cockpit, and for every sheet it opens. What the system draws
    /// there is light too, and the text grows no further than the screen
    /// has room for.
    func ventiFlightThemed() -> some View {
        modifier(Themed(isInFlight: true))
            .preferredColorScheme(.light)
            .dynamicTypeSize(...DynamicTypeSize.xxLarge)
    }
}

private struct Themed: ViewModifier {
    @Environment(\.colorScheme) private var scheme
    let isInFlight: Bool

    func body(content: Content) -> some View {
        let theme = isInFlight ? VentiTheme.flight : VentiTheme.of(scheme)
        content
            .environment(\.venti, theme)
            .environment(\.ventiInFlight, isInFlight)
            .tint(theme.primary)
            .foregroundStyle(theme.text, theme.textSecondary, theme.textMuted)
            .font(.ventiType(.body))
    }
}

/// A list in the design language: the page's ground behind it, and under
/// its rows the surface of a card.
struct VentiList<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        List {
            Group {
                content
            }
            .listRowBackground(Rectangle().fill(.venti(.surface)))
        }
        .scrollContentBackground(.hidden)
        .background(.venti(.bg))
    }
}

/// What UIKit draws: the titles of navigation bars and the words of tabs
/// and segmented controls, in the pack's typeface and ink.
@MainActor
enum VentiAppearance {
    static func apply() {
        let ink = UIColor { traits in UIColor(VentiTheme.of(traits).text) }
        if let inline = UIFont(name: "FiraSans-SemiBold", size: 19),
           let large = UIFont(name: "FiraSans-SemiBold", size: 26) {
            let bar = UINavigationBar.appearance()
            bar.titleTextAttributes = [
                .font: UIFontMetrics(forTextStyle: .headline).scaledFont(for: inline),
                .foregroundColor: ink,
            ]
            bar.largeTitleTextAttributes = [
                .font: UIFontMetrics(forTextStyle: .largeTitle).scaledFont(for: large),
                .foregroundColor: ink,
            ]
        }
        if let label = UIFont(name: "FiraSans-Medium", size: 14) {
            let scaled = UIFontMetrics(forTextStyle: .subheadline).scaledFont(for: label)
            UISegmentedControl.appearance().setTitleTextAttributes([.font: scaled], for: .normal)
        }
        if let caption = UIFont(name: "FiraSans-Medium", size: 12) {
            UITabBarItem.appearance().setTitleTextAttributes([.font: caption], for: .normal)
        }
    }
}
