import SwiftUI
import UIKit

// The design language's components, as `design/components/components.css`
// has them for the web, in SwiftUI. Each names the class it follows.

/// Sizes of the pack that its Swift tokens do not carry (`--size-*` in
/// `design/tokens/tokens.css`).
enum VentiSize {
    /// The least a finger needs.
    static let touch: CGFloat = 48
    /// The same in flight, with gloves.
    static let touchInFlight: CGFloat = 64
    /// An icon beside text.
    static let icon: CGFloat = 24
}

extension Color {
    /// The colour of a role as a `Color`, for where one is asked for by
    /// type: light or dark by the appearance of where it is drawn. Views
    /// otherwise name the role as a style, `.venti(.danger)`, which also
    /// knows the flight theme.
    static func venti(_ role: VentiColor.Role) -> Color {
        Color(uiColor: UIColor { traits in UIColor(VentiTheme.of(traits).color(role)) })
    }
}

/// The pack's button (`.v-btn`): a pill of the height a finger needs, in
/// one of its kinds. Its label is a verb.
struct VentiButtonStyle: ButtonStyle {
    enum Kind {
        /// The main button of a screen.
        case primary
        /// The one action that starts or records a flight: one a screen
        /// at most.
        case accent
        /// A button beside the main one.
        case quiet
        /// What cannot be taken back.
        case danger
    }

    let kind: Kind

    func makeBody(configuration: Configuration) -> some View {
        Face(kind: kind, configuration: configuration)
    }

    /// A view of its own, so that it can read the theme and whether the
    /// button is enabled.
    private struct Face: View {
        let kind: Kind
        let configuration: Configuration

        @Environment(\.venti) private var theme
        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            configuration.label
                .font(.venti(.body, .semibold))
                .lineLimit(1)
                .padding(.horizontal, VentiSpace.s5)
                .frame(minHeight: VentiSize.touch)
                .foregroundStyle(ink)
                .background(fill, in: .capsule)
                .overlay {
                    if kind == .quiet, isEnabled {
                        Capsule().strokeBorder(theme.borderStrong, lineWidth: 1.5)
                    }
                }
                .scaleEffect(configuration.isPressed ? 0.98 : 1)
                .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
                .contentShape(.capsule)
        }

        private var ink: Color {
            guard isEnabled else { return theme.textMuted }
            return switch kind {
            case .primary: theme.onPrimary
            case .accent: theme.onAccent
            case .quiet: theme.text
            case .danger: theme.onDangerFill
            }
        }

        private var fill: Color {
            guard isEnabled else { return theme.surfaceRaised }
            let isPressed = configuration.isPressed
            return switch kind {
            case .primary: isPressed ? theme.primaryPressed : theme.primary
            case .accent: isPressed ? VentiBrand.brick : theme.accent
            case .quiet: isPressed ? theme.surfaceRaised : .clear
            case .danger: theme.dangerFill
            }
        }
    }
}

extension ButtonStyle where Self == VentiButtonStyle {
    /// The pack's button in one of its kinds.
    static func venti(_ kind: VentiButtonStyle.Kind) -> VentiButtonStyle {
        VentiButtonStyle(kind: kind)
    }
}

/// The pack's status (`.v-status`): always a sign and a word, on the soft
/// colour of its kind. Never a colour alone.
struct VentiStatus: View {
    enum Kind {
        case good, caution, danger
    }

    let kind: Kind
    let word: String

    var body: some View {
        HStack(spacing: 5) {
            Image(sign)
                .resizable()
                .frame(width: 17, height: 17)
            Text(word)
                .font(.venti(.subheadline, .semibold))
                .lineLimit(1)
        }
        .padding(.leading, 7)
        .padding(.trailing, 10)
        .frame(height: 28)
        .foregroundStyle(.venti(ink))
        .background(.venti(ground), in: .rect(cornerRadius: VentiRadius.s))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(word)
    }

    private var sign: String {
        switch kind {
        case .good: "icon-good"
        case .caution: "icon-caution"
        case .danger: "icon-danger"
        }
    }

    private var ink: VentiColor.Role {
        switch kind {
        case .good: .good
        case .caution: .caution
        case .danger: .danger
        }
    }

    private var ground: VentiColor.Role {
        switch kind {
        case .good: .goodSoft
        case .caution: .cautionSoft
        case .danger: .dangerSoft
        }
    }
}

extension View {
    /// The pack's card (`.v-card`): its surface with a hairline border.
    /// Flat: only what floats over a map has a shadow.
    func ventiCard() -> some View {
        padding(VentiSpace.s4)
            .background(.venti(.surface), in: .rect(cornerRadius: VentiRadius.m))
            .overlay {
                RoundedRectangle(cornerRadius: VentiRadius.m)
                    .strokeBorder(.venti(.border), lineWidth: 1)
            }
    }

    /// What floats over a map: the pack's surface in a shape, with the
    /// pack's shadow for it (`--shadow-float`). No glass.
    func ventiFloating(in shape: some Shape) -> some View {
        background(.venti(.surface), in: shape)
            .shadow(color: VentiBrand.ink.opacity(0.14), radius: 3, y: 2)
            .shadow(color: VentiBrand.ink.opacity(0.16), radius: 12, y: 8)
    }
}
