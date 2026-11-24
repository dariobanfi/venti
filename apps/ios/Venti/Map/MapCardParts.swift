import SwiftUI
import VentiCore

/// What the panel over the map tab's map is made of with other cards: a
/// line of a key, and how a card says that something is wrong.

/// One line of a key: a square in the colour the map draws with, and the
/// words. The map's colours, which are the light theme's in both
/// appearances, with an edge so that the palest stands out on any surface.
struct MapKeyLine: View {
    let colour: Color
    let words: String

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s2) {
            // The pack's scale legend (`.v-scale`) has its swatches 12
            // high, square-cornered.
            Rectangle()
                .fill(colour)
                .overlay {
                    Rectangle()
                        .strokeBorder(.venti(.borderStrong), lineWidth: 1)
                }
                .frame(width: 16, height: 12)
                // Beside the first line's letters, not above them.
                .padding(.top, 3)
                .accessibilityHidden(true)
            Text(words)
                .font(.ventiType(.label))
                .foregroundStyle(.venti(.text))
                .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
    }
}

/// Words in the caution manner: the pack's sign in the status colour, and
/// the words beside it.
struct CardCaution: View {
    let words: String

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s2) {
            Image("icon-caution")
                .resizable()
                .frame(width: 18, height: 18)
                .foregroundStyle(.venti(.caution))
                .accessibilityHidden(true)
            Text(words)
                .font(.ventiType(.label))
                .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
    }
}

/// What went wrong, in the core's words where it has them, and the one
/// thing to do about it.
struct CardProblem: View {
    let words: String
    let identifier: String
    let tryAgain: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            CardCaution(words: words)
                .accessibilityIdentifier(identifier)
            Button(commonWords().tryAgain, action: tryAgain)
                .buttonStyle(.venti(.quiet))
        }
    }
}
