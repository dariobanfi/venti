import SwiftUI

/// "Continue with Google", drawn to Google's "Sign in with Google branding
/// guidelines" (https://developers.google.com/identity/branding-guidelines,
/// read 2026-11-21), as a custom button of the kind they allow.
///
/// This is the one place in the app whose colours and sizes are Google's
/// and not the design language's tokens, by the owner's word of 2026-11-21.
/// Nothing here is to be "brought into line" with the pack: no token, no
/// pack font, no pack radius.
///
/// What the guidelines state for iOS, and what is drawn:
/// - Light: fill #FFFFFF, stroke #747775 1 px inside, text #1F1F1F.
///   Dark: fill #131314, stroke #8E918F 1 px inside, text #E3E3E3.
/// - 44 points high, a pill; 16 before the logo, 12 between the logo and
///   the text, 16 after the text; the logo 20 by 20.
/// - The text 14/20 in a medium weight. Google's face for it is Google
///   Sans Medium (their padding drawing still says Roboto Medium); the app
///   bundles neither, so it is the system's font at that size and weight.
/// - The button may be scaled as long as its proportions are kept, so
///   every measure grows by one factor with the pilot's text size.
///
/// The "G" is Google's own picture (`google-g` in `Google.xcassets`,
/// see `apps/ios/AGENTS.md` for where it came from): never redrawn,
/// recoloured or tinted. One picture on a transparent ground serves the
/// light and the dark button.
struct GoogleButton: View {
    /// The core's words: "Continue with Google".
    let title: String
    /// Whether the app is busy with it: a spinner in place of the words.
    var isWorking = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Content(title: title, isWorking: isWorking)
        }
        .buttonStyle(Style())
        .accessibilityLabel(title)
    }

    /// Google's colours for the theme the button is shown in.
    private struct Colours {
        let fill: Color
        let stroke: Color
        let text: Color

        init(_ scheme: ColorScheme) {
            if scheme == .dark {
                fill = Color(hex: 0x131314)
                stroke = Color(hex: 0x8E918F)
                text = Color(hex: 0xE3E3E3)
            } else {
                fill = Color(hex: 0xFFFFFF)
                stroke = Color(hex: 0x747775)
                text = Color(hex: 0x1F1F1F)
            }
        }
    }

    private struct Content: View {
        let title: String
        let isWorking: Bool

        @Environment(\.colorScheme) private var scheme
        /// Google's 14-point text at the pilot's text size; every other
        /// measure follows it by the same factor.
        @ScaledMetric(relativeTo: .subheadline) private var textSize: CGFloat = 14

        var body: some View {
            let scale = textSize / 14
            let colours = Colours(scheme)
            Group {
                if isWorking {
                    ProgressView()
                        .tint(colours.text)
                } else {
                    HStack(spacing: 12 * scale) {
                        Image(decorative: "google-g")
                            .resizable()
                            .interpolation(.high)
                            .scaledToFit()
                            .frame(width: 20 * scale, height: 20 * scale)
                        Text(title)
                            .font(.system(size: textSize, weight: .medium))
                            .foregroundStyle(colours.text)
                            .lineLimit(1)
                            .minimumScaleFactor(0.7)
                    }
                }
            }
            .padding(.horizontal, 16 * scale)
            .frame(maxWidth: .infinity, minHeight: 44 * scale)
        }
    }

    private struct Style: ButtonStyle {
        func makeBody(configuration: Configuration) -> some View {
            Face(configuration: configuration)
        }

        private struct Face: View {
            let configuration: Configuration

            @Environment(\.colorScheme) private var scheme

            var body: some View {
                let colours = Colours(scheme)
                configuration.label
                    .background(colours.fill, in: .capsule)
                    .overlay {
                        // Pressed, as Material does it: the text's colour
                        // laid thinly over the whole button, the "G" too.
                        if configuration.isPressed {
                            Capsule().fill(colours.text.opacity(0.12))
                        }
                    }
                    .overlay {
                        Capsule().strokeBorder(colours.stroke, lineWidth: 1)
                    }
                    .contentShape(.capsule)
            }
        }
    }
}
