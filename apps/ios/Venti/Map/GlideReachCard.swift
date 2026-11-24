import SwiftUI
import VentiCore

/// The card under the glide's reach on the map: the core's words, the
/// glide, the start height and the wind, each change asking the core
/// again, and the core's note at its foot. "Clear" takes the area away.
///
/// The wind is one the pilot sets, a direction and a speed, off at first:
/// the map has no forecast at hand for a place it was not asked about.
struct GlideReachCard: View {
    @Bindable var reach: GlideReachModel
    let clear: () -> Void

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: VentiSpace.s3) {
                    answer
                    settings
                    if let note = reach.answer?.note, !note.isEmpty {
                        Text(note)
                            .font(.ventiType(.caption))
                            .foregroundStyle(.venti(.textMuted))
                            .fixedSize(horizontal: false, vertical: true)
                            .accessibilityIdentifier("glide-reach-note")
                    }
                }
                .padding(.horizontal, VentiSpace.s4)
                .padding(.bottom, VentiSpace.s4)
            }
            .background(.venti(.bg))
            .navigationTitle(GlideReachWords.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(GlideReachWords.clear, action: clear)
                        .accessibilityIdentifier("glide-reach-clear")
                }
            }
        }
        // Every change of a setting asks the core again, from the ground
        // already brought.
        .task(id: reach.setting) {
            await reach.ask()
        }
    }

    /// The core's answer in words, or that the ground is being read.
    @ViewBuilder private var answer: some View {
        if let answer = reach.answer {
            Text(answer.words)
                .font(.venti(.body))
                .foregroundStyle(.venti(.text))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("glide-reach-words")
        } else if reach.isWorking {
            HStack(spacing: VentiSpace.s2) {
                ProgressView()
                Text(GlideReachWords.working)
                    .font(.ventiType(.label))
                    .fixedSize(horizontal: false, vertical: true)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("glide-reach-working")
        }
    }

    /// The glide, the start height and the wind, a step at a time.
    private var settings: some View {
        VStack(alignment: .leading, spacing: 0) {
            Stepper(value: $reach.glideRatio, in: GlideReachWords.glideRatios, step: GlideReachWords.glideStep) {
                LabeledContent("Glide", value: Format.glideRatio(reach.glideRatio))
            }
            .accessibilityIdentifier("glide-reach-glide")
            Stepper {
                LabeledContent("Start height", value: startWords)
            } onIncrement: {
                reach.raiseStart()
            } onDecrement: {
                reach.lowerStart()
            }
            .accessibilityIdentifier("glide-reach-start")
            Toggle("Wind", isOn: $reach.windIsOn)
                .frame(minHeight: VentiSize.touch)
                .accessibilityIdentifier("glide-reach-wind")
            if reach.windIsOn {
                Stepper {
                    LabeledContent("Wind from", value: Format.compassPoint(degrees: reach.windFromDeg))
                } onIncrement: {
                    reach.windFromDeg = (reach.windFromDeg + GlideReachWords.windStepDeg).truncatingRemainder(dividingBy: 360)
                } onDecrement: {
                    reach.windFromDeg = (reach.windFromDeg + 360 - GlideReachWords.windStepDeg)
                        .truncatingRemainder(dividingBy: 360)
                }
                .accessibilityIdentifier("glide-reach-wind-from")
                Stepper(value: $reach.windKmh, in: GlideReachWords.windsKmh, step: GlideReachWords.windStepKmh) {
                    LabeledContent("Wind speed", value: "\(Format.count(reach.windKmh)) km/h")
                }
                .accessibilityIdentifier("glide-reach-wind-speed")
            }
        }
        .font(.venti(.subheadline))
        .monospacedDigit()
    }

    /// "On the ground" for a start from the place itself, else the height.
    private var startWords: String {
        guard let height = reach.startHeightM else { return "On the ground" }
        return "\(Format.metres(height)) m"
    }
}
