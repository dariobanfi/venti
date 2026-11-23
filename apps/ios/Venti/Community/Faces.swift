import SwiftUI
import VentiCore

// A pilot's face wherever a pilot is named: their picture, which Venti
// keeps and serves (`/v1/pictures/{id}`, a path on the server), and their
// initials in a circle where they have none, or where the answer does not
// name it. The web's `faces.ts` makes the same initials.

/// The initials of a name: the first letters of its first and last words,
/// "Mara Muster" → "MM", "jonas" → "J". "?" for a name without a letter.
enum Initials {
    static func of(_ name: String) -> String {
        let words = name.split(whereSeparator: \.isWhitespace)
            .map { $0.drop { !$0.isLetter && !$0.isNumber } }
            .filter { !$0.isEmpty }
        guard let first = words.first?.first else { return "?" }
        let last = words.count > 1 ? words.last?.first : nil
        return [first, last].compactMap { $0 }.map(String.init).joined().uppercased()
    }
}

/// A picture on the server, by the path an answer names.
enum PictureAddress {
    /// The picture's address on the Venti the app reads from; `nil`
    /// without a picture or without a Venti.
    static func url(_ path: String?, on server: URL?) -> URL? {
        guard let path, !path.isEmpty, let server else { return nil }
        return URL(string: server.absoluteString + path)
    }
}

/// A pilot's face: the picture, round, or the initials in a circle while
/// there is none, while it comes and where it could not be had. It grows
/// with the pilot's text size. Decoration beside the name, which says who
/// it is: VoiceOver reads the name, not the face.
struct PilotFace: View {
    /// The name the pilot is shown by, for the initials.
    let name: String
    /// The path of the pilot's picture on the server, where the answer
    /// names one.
    var picturePath: String?
    /// How wide the circle is, at the standard text size: one of the
    /// pack's spaces.
    var size: CGFloat = VentiSpace.s5
    /// The pilot of this iPhone, drawn on the primary ground where the
    /// face is initials.
    var isMe = false
    /// The initials where an answer gives them (`NearPilotLine.initials`);
    /// else they are made from the name.
    var initials: String?
    /// Whether VoiceOver finds the face as an element of its own, which
    /// its caller names; else it is hidden beside the name.
    var isOwnElement = false

    @Environment(WindModel.self) private var wind
    @ScaledMetric(relativeTo: .body) private var scale = 1

    var body: some View {
        let side = size * scale
        Group {
            if let url = PictureAddress.url(picturePath, on: wind.server) {
                AsyncImage(url: url) { phase in
                    if let image = phase.image {
                        image
                            .resizable()
                            .scaledToFill()
                    } else {
                        letters(side)
                    }
                }
            } else {
                letters(side)
            }
        }
        .frame(width: side, height: side)
        .clipShape(.circle)
        .overlay {
            Circle().strokeBorder(.venti(.border), lineWidth: 1)
        }
        .accessibilityHidden(!isOwnElement)
    }

    private func letters(_ side: CGFloat) -> some View {
        let shown = initials.flatMap { $0.isEmpty ? nil : $0 } ?? Initials.of(name)
        return Text(verbatim: shown)
            .font(side >= VentiSpace.s7 ? .ventiType(.heading) : .venti(.caption, .semibold))
            .minimumScaleFactor(0.5)
            .lineLimit(1)
            .padding(VentiSpace.s1)
            .frame(width: side, height: side)
            .foregroundStyle(.venti(isMe ? .onPrimary : .text))
            .background(.venti(isMe ? .primary : .surfaceRaised))
    }
}

/// Who is in on a plan, as a row of faces standing together: at most
/// `room`, and how many more there are. The core names them, the pilot of
/// this iPhone first as "You" while they are in.
struct PlanFaces: View {
    let names: [String]
    /// Whether the first name is the pilot of this iPhone.
    let firstIsMe: Bool
    /// The name the pilot of this iPhone is shown by, for their initials.
    let myName: String?
    var room = 5

    var body: some View {
        let shown = names.count > room ? Array(names.prefix(room - 1)) : names
        let more = names.count - shown.count
        HStack(spacing: -VentiSpace.s2) {
            ForEach(Array(shown.enumerated()), id: \.offset) { index, name in
                let isMe = firstIsMe && index == 0
                PilotFace(name: isMe ? (myName ?? name) : name, size: VentiSpace.s6, isMe: isMe)
            }
            if more > 0 {
                Text(verbatim: "+\(more)")
                    .font(.venti(.caption, .semibold))
                    .monospacedDigit()
                    .foregroundStyle(.venti(.textSecondary))
                    .padding(.horizontal, VentiSpace.s2)
                    .frame(minWidth: VentiSpace.s6, minHeight: VentiSpace.s6)
                    .background(.venti(.surface), in: .capsule)
                    .overlay {
                        Capsule().strokeBorder(.venti(.border), lineWidth: 1)
                    }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(names.joined(separator: ", "))
    }
}
