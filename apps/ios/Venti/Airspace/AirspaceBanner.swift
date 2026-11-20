import AVFoundation
import SwiftUI
import UIKit
import VentiCore

extension AirspaceLevel {
    /// The level in a word or two, for a heading.
    var words: String {
        switch self {
        case .near: "Airspace near"
        case .veryNear: "Airspace very near"
        case .entering: "About to enter"
        case .inside: "Inside airspace"
        }
    }
}

/// The most urgent airspace warning, laid over the cockpit's map. A tap
/// opens all of them, with the ways to quiet one.
struct AirspaceBanner: View {
    let warnings: [AirspaceWarning]
    /// Whether anything is being checked at all.
    let check: AirspaceCheck
    let open: () -> Void

    /// What to say when the app is not checking, so that its silence is
    /// not taken for "no airspace near".
    private var notice: String? {
        switch check {
        case .checking, .noAirspace: nil
        case .waitingForHeight: "Airspace is not being checked yet: no position with a height."
        case .stale: "Airspace is not being checked: no position. What is shown is from the last one."
        case .outsideFiles: "Your airspace does not cover this place. Nothing is checked here."
        }
    }

    var body: some View {
        VStack(spacing: 6) {
            if let notice {
                Label(notice, systemImage: "eye.slash")
                    .font(.venti(.footnote, .medium))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .ventiFloating(in: .rect(cornerRadius: 14))
                    .accessibilityIdentifier("airspace-notice")
            }
            band
        }
    }

    @ViewBuilder private var band: some View {
        if let warning = warnings.first(where: { !$0.silenced }) ?? warnings.first {
            Button(action: open) {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: warning.silenced ? "bell.slash.fill" : "exclamationmark.triangle.fill")
                        .font(.venti(.title3))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(warning.headline)
                            .font(.venti(.headline))
                            .multilineTextAlignment(.leading)
                        Text(warning.detail)
                            .font(.venti(.footnote))
                            .multilineTextAlignment(.leading)
                        if let uncertain = warning.uncertain {
                            Text(uncertain)
                                .font(.venti(.footnote))
                                .multilineTextAlignment(.leading)
                        }
                    }
                    Spacer(minLength: 4)
                    if warnings.count > 1 {
                        Text("+\(warnings.count - 1)")
                            .font(.venti(.subheadline, .semibold))
                            .monospacedDigit()
                    }
                }
                .foregroundStyle(Self.ink(for: warning))
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Self.ground(for: warning), in: .rect(cornerRadius: 16))
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(warning.silenced ? "Airspace warning, quieted" : "Airspace warning")
            .accessibilityValue("\(warning.headline). \(warning.detail). \(warning.uncertain ?? "")")
            .accessibilityHint("Shows all warnings")
            .accessibilityIdentifier("airspace-banner")
        }
    }

    /// Red when the glider is inside or about to be, amber when very near,
    /// plain otherwise. A quieted warning is plain whatever its level.
    private static func ground(for warning: AirspaceWarning) -> AnyShapeStyle {
        if warning.silenced {
            return AnyShapeStyle(.venti(.surface))
        }
        switch warning.level {
        case .inside, .entering: return AnyShapeStyle(Color(red: 0.72, green: 0.14, blue: 0.17))
        case .veryNear: return AnyShapeStyle(Color(red: 0.98, green: 0.70, blue: 0.10))
        case .near: return AnyShapeStyle(.venti(.surface))
        }
    }

    private static func ink(for warning: AirspaceWarning) -> Color {
        if warning.silenced {
            return .secondary
        }
        switch warning.level {
        case .inside, .entering: return .white
        case .veryNear: return Color(red: 0.04, green: 0.04, blue: 0.04)
        case .near: return .primary
        }
    }
}

/// Every airspace warning of the moment, with what the pilot can do about
/// each: say it has been seen, or that the airspace does not apply today.
struct AirspaceWarningsSheet: View {
    @Environment(FlySession.self) private var fly
    @Environment(AirspaceModel.self) private var airspace
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                ForEach(fly.airspaceWarnings, id: \.id) { warning in
                    Section {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(warning.headline)
                                .font(.venti(.headline))
                            Text(warning.detail)
                                .font(.venti(.subheadline))
                                .foregroundStyle(.secondary)
                            if let uncertain = warning.uncertain {
                                Text(uncertain)
                                    .font(.venti(.subheadline))
                            }
                        }
                        .accessibilityElement(children: .combine)
                        if warning.silenced {
                            Button("Warn me again", systemImage: "bell") {
                                fly.unsilenceAirspace(warning.id)
                            }
                        } else {
                            Button("Seen: quiet for 10 minutes", systemImage: "bell.slash") {
                                fly.acknowledgeAirspace(warning.id)
                                dismiss()
                            }
                            Button("Does not apply today", systemImage: "moon.zzz") {
                                fly.ignoreAirspaceToday(warning.id)
                                dismiss()
                            }
                        }
                    } header: {
                        Text(warning.silenced ? "Quieted" : warning.level.words)
                    } footer: {
                        Text(Self.origin(of: warning))
                    }
                }
                if fly.airspaceWarnings.isEmpty {
                    Text("No airspace near.")
                        .foregroundStyle(.secondary)
                }
                Section {
                } footer: {
                    Text(Self.advice + airspace.credits.compactMap(\.notice).map { "\n\n\($0)" }.joined())
                }
            }
            .navigationTitle("Airspace")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    private static let advice = "A warning you have seen speaks again if it becomes more urgent. “Does not apply today” is for an area you know to be inactive or have permission for: Venti cannot check that. Warnings are advice from the airspace the app has; you remain responsible for where you fly."

    /// Where the airspace comes from and how old that is.
    static func origin(of warning: AirspaceWarning) -> String {
        switch (warning.origin, warning.dataUnixS) {
        case (.file, nil): "From your file “\(warning.source)”."
        case (.file, let date?): "From your file “\(warning.source)”, brought in \(Format.day(unixSeconds: date))."
        case (.server, nil): "From \(warning.source), by way of your server."
        case (.server, let date?): "From \(warning.source), by way of your server, as of \(Format.day(unixSeconds: date))."
        }
    }
}

/// Says a warning aloud. The pilot's eyes are outside the cockpit.
@MainActor
final class AirspaceVoice {
    private let synthesizer = AVSpeechSynthesizer()
    private let tap = UINotificationFeedbackGenerator()

    /// Speaks a warning, replacing one still being spoken, and taps the
    /// phone in the pilot's pocket or on the harness.
    func say(_ text: String) {
        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = AVSpeechSynthesisVoice(language: "en-GB")
        synthesizer.speak(utterance)
        tap.notificationOccurred(.warning)
    }

    /// Speaks once whatever is being spoken has ended, without cutting
    /// it off: for what matters less than airspace.
    func sayAfter(_ text: String) {
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = AVSpeechSynthesisVoice(language: "en-GB")
        synthesizer.speak(utterance)
        tap.notificationOccurred(.warning)
    }

    func stop() {
        if synthesizer.isSpeaking {
            synthesizer.stopSpeaking(at: .immediate)
        }
    }
}
