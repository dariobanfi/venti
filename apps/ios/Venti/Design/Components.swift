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
        @Environment(\.ventiInFlight) private var isInFlight
        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            configuration.label
                .font(isInFlight ? .ventiType(.heading) : .venti(.body, .semibold))
                .lineLimit(1)
                .padding(.horizontal, isInFlight ? VentiSpace.s4 : VentiSpace.s5)
                .frame(minHeight: isInFlight ? VentiSize.touchInFlight : VentiSize.touch)
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
    /// pack's shadow for it (`--shadow-float`). No glass. In flight there
    /// are no shadows: the shape has an edge instead. A ground other than
    /// the surface is for a control that is pressed or switched on.
    func ventiFloating(in shape: some Shape, ground: VentiColor.Role = .surface) -> some View {
        modifier(Floating(shape: shape, ground: ground))
    }
}

/// The pack's segmented control (`.v-seg`): a few choices side by side in
/// a pill, the chosen one on the primary colour. Up to four share the
/// width: equally where every name fits its share, and else each as wide
/// as its name needs (`VentiSegmentWidths`), so that no name is cut. More
/// than four, or four whose names are wider than the pill, keep their own
/// width and scroll.
struct VentiSegmented<Value: Hashable>: View {
    let options: [(value: Value, name: String)]
    @Binding var selection: Value

    var body: some View {
        if options.count <= 4 {
            ViewThatFits(in: .horizontal) {
                segments(sharingTheWidth: true)
                scrolling
            }
        } else {
            scrolling
        }
    }

    private var scrolling: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            segments(sharingTheWidth: false)
        }
        .clipShape(.capsule)
    }

    private func segments(sharingTheWidth: Bool) -> some View {
        let row = sharingTheWidth ? AnyLayout(VentiSegmentWidths()) : AnyLayout(HStackLayout(spacing: 0))
        return row {
            ForEach(options, id: \.value) { option in
                let isChosen = option.value == selection
                Button {
                    selection = option.value
                } label: {
                    Text(option.name)
                        .font(.ventiType(.label))
                        .lineLimit(1)
                        .padding(.horizontal, sharingTheWidth ? VentiSpace.s1 : VentiSpace.s4)
                        .frame(maxWidth: sharingTheWidth ? .infinity : nil, minHeight: 38)
                        .foregroundStyle(.venti(isChosen ? .onPrimary : .textSecondary))
                        .background(.venti(isChosen ? .primary : .surfaceRaised), in: .capsule)
                        .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(isChosen ? .isSelected : [])
            }
        }
        .padding(3)
        .background(.venti(.surfaceRaised), in: .capsule)
    }
}

/// How the segments of the pack's segmented control share its width: side
/// by side without a gap, each as wide as `widths` says.
struct VentiSegmentWidths: Layout {
    /// The widths of segments whose names need `needed`, in a pill
    /// `available` wide. Equal where every name fits an equal share. Else
    /// a name that needs more than its share gets what it needs, and the
    /// others share what is left equally. Where the names together are
    /// wider than the pill, each gets its part of the width by what it
    /// needs: the control scrolls before it comes to that.
    static func widths(needed: [CGFloat], available: CGFloat) -> [CGFloat] {
        let all = needed.reduce(0, +)
        guard !needed.isEmpty, available > 0, all > 0 else {
            return needed.map { _ in 0 }
        }
        if all >= available {
            return needed.map { $0 / all * available }
        }
        var widths = needed
        var open = Array(needed.indices)
        var room = available
        while !open.isEmpty {
            let share = room / CGFloat(open.count)
            let wider = open.filter { needed[$0] > share }
            if wider.isEmpty {
                for index in open {
                    widths[index] = share
                }
                break
            }
            for index in wider {
                room -= needed[index]
            }
            open.removeAll { wider.contains($0) }
        }
        return widths
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let needed = subviews.map { $0.sizeThatFits(.unspecified) }
        let all = needed.map(\.width).reduce(0, +)
        let width = proposal.width.flatMap { $0.isFinite ? $0 : nil } ?? all
        return CGSize(width: width, height: needed.map(\.height).max() ?? 0)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let needed = subviews.map { $0.sizeThatFits(.unspecified).width }
        var x = bounds.minX
        for (subview, width) in zip(subviews, Self.widths(needed: needed, available: bounds.width)) {
            subview.place(
                at: CGPoint(x: x, y: bounds.minY), anchor: .topLeading,
                proposal: ProposedViewSize(width: width, height: bounds.height)
            )
            x += width
        }
    }
}

/// The pack's empty state: its valley, what is not here yet, what brings
/// it here, and what to do. For a place that is empty because nothing has
/// happened there yet, which is an arrival. Not for something that went
/// wrong: that is said with the caution sign and without art, which
/// never stands where data is missed.
struct VentiEmpty<Actions: View>: View {
    let title: String
    let text: String
    @ViewBuilder var actions: Actions

    var body: some View {
        VStack(spacing: VentiSpace.s4) {
            Image("art-valley-wide-empty")
                .resizable()
                .aspectRatio(2, contentMode: .fit)
                .clipShape(.rect(cornerRadius: VentiRadius.m))
                .accessibilityHidden(true)
            VStack(spacing: VentiSpace.s2) {
                Text(title)
                    .font(.ventiType(.heading))
                    .accessibilityAddTraits(.isHeader)
                Text(text)
                    .foregroundStyle(.venti(.textSecondary))
            }
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            actions
        }
        .padding(VentiSpace.s5)
        .frame(maxWidth: 440)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        // The page's ground, also where the screen under it has none.
        .background(.venti(.bg))
    }
}

extension VentiEmpty where Actions == EmptyView {
    /// An empty state with nothing to do about it here.
    init(title: String, text: String) {
        self.init(title: title, text: text) {}
    }
}

/// The pack's chip (`.v-chip`): switches a layer of the map on and off.
/// Drawn 36 high, with room above and below it for the finger.
struct VentiChip: View {
    let title: String
    /// The name of one of the pack's icons; none for a chip of words alone.
    var icon: String? = nil
    @Binding var isOn: Bool

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            HStack(spacing: 6) {
                if let icon {
                    Image(icon)
                        .resizable()
                        .frame(width: 18, height: 18)
                }
                Text(title)
                    .font(.ventiType(.label))
                    .lineLimit(1)
            }
            .padding(.leading, icon == nil ? 14 : 10)
            .padding(.trailing, 14)
            .frame(height: 36)
            .foregroundStyle(.venti(isOn ? .onPrimary : .text))
            .ventiFloating(in: .capsule, ground: isOn ? .primary : .surface)
            .frame(minHeight: VentiSize.touch)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(isOn ? .isSelected : [])
    }
}

/// The pack's round button (`.v-btn--round`): a sign alone, floating over
/// a map. Its label is still said to VoiceOver.
struct VentiRoundButtonStyle: ButtonStyle {
    /// For a button that switches something on and off: whether it is on.
    var isOn = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .labelStyle(.iconOnly)
            .font(.system(size: 20, weight: .medium))
            .frame(width: VentiSize.touch, height: VentiSize.touch)
            .foregroundStyle(.venti(isOn ? .link : .text))
            .ventiFloating(in: .circle, ground: configuration.isPressed ? .surfaceRaised : .surface)
            .contentShape(.circle)
    }
}

extension ButtonStyle where Self == VentiRoundButtonStyle {
    /// The pack's round button over a map.
    static var ventiRound: VentiRoundButtonStyle { VentiRoundButtonStyle() }

    /// The pack's round button over a map, for something that is on or off.
    static func ventiRound(isOn: Bool) -> VentiRoundButtonStyle { VentiRoundButtonStyle(isOn: isOn) }
}

private struct Floating<Outline: Shape>: ViewModifier {
    let shape: Outline
    let ground: VentiColor.Role
    @Environment(\.ventiInFlight) private var isInFlight

    func body(content: Content) -> some View {
        if isInFlight {
            content
                .background(.venti(ground), in: shape)
                .overlay {
                    shape.stroke(.venti(.borderStrong), lineWidth: 1.5)
                }
        } else {
            content
                .background(.venti(ground), in: shape)
                .shadow(color: VentiBrand.ink.opacity(0.14), radius: 3, y: 2)
                .shadow(color: VentiBrand.ink.opacity(0.16), radius: 12, y: 8)
        }
    }
}
