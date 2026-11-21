import SwiftUI
import VentiCore

/// The card at the map's lower edge while the rain radar is on, in the
/// manner of the forecast map's: one line of time for all the layers as
/// the pack's hour strip, a cell a moment, with a button that plays them;
/// the line of the moment on the map, or in its place that the newest
/// picture is too old to be the rain of now; and behind "Key and notes",
/// closed when the card opens, what each layer is, the key, what the
/// radar did not see, what the pictures are and are not, and whose they
/// are. While Venti is asked, and when there is no radar, it says so.
/// Every word about the rain is the core's.
struct RadarCard: View {
    let radar: RadarModel
    let tryAgain: () -> Void

    /// Whether the key and the notes are shown.
    @State private var showsKey = false
    @Environment(\.accessibilityReduceMotion) private var reducesMotion

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            if let view = radar.view {
                if !view.moments.isEmpty {
                    moments(of: view)
                }
                words
                if let problem = radar.pictureProblem {
                    CardProblem(words: problem, identifier: "radar-picture-problem") {
                        Task {
                            await radar.tryPictureAgain()
                        }
                    }
                }
                KeyAndNotesButton(isShown: $showsKey, identifier: "radar-more")
                if showsKey {
                    // As high as it is where the screen has the room, and
                    // else scrolling in what room there is.
                    ViewThatFits(in: .vertical) {
                        keyAndNotes(of: view)
                        ScrollView {
                            keyAndNotes(of: view)
                        }
                    }
                }
            } else if case .none(let words) = radar.state {
                // A Venti without a rain radar: said plainly, and nothing
                // to try again.
                Text(words)
                    .font(.ventiType(.label))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("radar-none")
            } else if case .failed(let words) = radar.state {
                CardProblem(words: words, identifier: "radar-problem", tryAgain: tryAgain)
            } else {
                HStack(spacing: VentiSpace.s2) {
                    ProgressView()
                    Text("Asking Venti for the rain radar.")
                        .font(.ventiType(.label))
                        .fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("radar-asking")
            }
        }
        .padding(VentiSpace.s3)
        .frame(maxWidth: .infinity, alignment: .leading)
        .ventiFloating(in: .rect(cornerRadius: VentiRadius.m))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("radar-card")
    }

    /// The moments: the button that plays them, where there is more than
    /// one and the pilot has not asked for less motion, and the strip.
    private func moments(of view: RadarView) -> some View {
        HStack(alignment: .top, spacing: CellStrip.gap) {
            if view.moments.count > 1, !reducesMotion {
                Button {
                    radar.play()
                } label: {
                    Image(systemName: radar.isPlaying ? "stop.fill" : "play.fill")
                        .font(.ventiType(.label))
                        .frame(width: VentiSize.touch, height: VentiSize.touch)
                        .foregroundStyle(.venti(.text))
                        .background(.venti(.surfaceRaised), in: .rect(cornerRadius: VentiRadius.s))
                        .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(radar.isPlaying ? "Stop" : "Play the radar pictures")
                .accessibilityIdentifier("radar-play")
            }
            CellStrip(
                cells: view.moments.map(\.clock), named: RadarStrip.named(of: view.moments.count),
                chosen: chosen, name: "Radar picture", spoken: radar.spoken, identifier: "radar-frames"
            )
        }
    }

    /// The chosen moment, as the strip chooses it: a finger on the strip
    /// stops the playing.
    private var chosen: Binding<Int?> {
        Binding {
            radar.chosenIndex
        } set: { index in
            guard let index, index != radar.chosenIndex else { return }
            radar.stop()
            Task {
                await radar.choose(index)
            }
        }
    }

    /// The line under the strip: the moment on the map with its time and
    /// its age, and what lies under it of another moment; or, in the
    /// caution manner, that the newest picture is not the rain of now.
    @ViewBuilder private var words: some View {
        if let caution = radar.caution {
            CardCaution(words: caution)
                .accessibilityIdentifier("radar-stale")
        } else if !radar.label.isEmpty {
            HStack(spacing: VentiSpace.s2) {
                Text(radar.label)
                    .font(.ventiType(.label))
                    .foregroundStyle(.venti(.text))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("radar-words")
                if radar.isFetching {
                    ProgressView()
                        .controlSize(.small)
                }
            }
        }
    }

    /// What stands behind "Key and notes": what each layer is, a line a
    /// layer, the topmost first; the key from the most rain down, with
    /// what the radar did not see as its last line; what the pictures are
    /// and are not; and the credit of every source drawn.
    private func keyAndNotes(of view: RadarView) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                ForEach(view.layers.reversed(), id: \.source) { layer in
                    Text(layer.what)
                }
            }
            .font(.ventiType(.label))
            .foregroundStyle(.venti(.text))
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("radar-layers")
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                ForEach(view.key, id: \.step) { line in
                    MapKeyLine(colour: RadarColours.onMap(line), words: line.words)
                }
                MapKeyLine(colour: RadarColours.unseenOnMap, words: view.unseenWords)
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Key")
            .accessibilityIdentifier("radar-key")
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                Text(view.note)
                ForEach(radar.drawnLayers.compactMap(\.credit), id: \.text) { credit in
                    Text(credit.text)
                }
            }
            .font(.ventiType(.caption))
            .foregroundStyle(.venti(.textSecondary))
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("radar-note")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// How the radar's moments stand in the hour strip.
enum RadarStrip {
    /// The most moments whose times stand in their cells: a clock's time
    /// needs a cell a finger's width wide, and a phone has six of them
    /// beside the button that plays.
    static let mostWithTimes = 6

    /// Which moments' times are written under the strip where they are
    /// too many for a time in each cell: the newest and every third
    /// before it, every sixth of more than twelve, which with a moment
    /// every five minutes is every quarter and every half of an hour.
    /// Only those that have the room of a whole step before them. `nil`
    /// where every cell holds its time.
    static func named(of count: Int) -> Set<Int>? {
        guard count > mostWithTimes else { return nil }
        let every = count > 2 * mostWithTimes ? 6 : 3
        return Set(stride(from: count - 1, through: every - 1, by: -every))
    }
}
