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

/// The most urgent airspace warning, at the top of the cockpit. A tap
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
                    .font(.ventiType(.label))
                    .padding(.horizontal, VentiSpace.s4)
                    .padding(.vertical, VentiSpace.s2)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(.venti(.surfaceRaised), in: .rect(cornerRadius: VentiRadius.m))
                    .accessibilityIdentifier("airspace-notice")
            }
            band
        }
    }

    @ViewBuilder private var band: some View {
        if let warning = warnings.first(where: { !$0.silenced }) ?? warnings.first {
            Button(action: open) {
                HStack(alignment: .center, spacing: VentiSpace.s3) {
                    if warning.silenced {
                        Image(systemName: "bell.slash.fill")
                            .font(.ventiType(.heading))
                            .frame(width: VentiSize.icon)
                    } else {
                        Image("icon-airspace")
                            .resizable()
                            .frame(width: VentiSize.icon, height: VentiSize.icon)
                    }
                    VStack(alignment: .leading, spacing: 0) {
                        Text(warning.headline)
                            .font(.ventiType(.heading))
                            .multilineTextAlignment(.leading)
                        Text(warning.detail)
                            .font(.ventiType(.label))
                            .multilineTextAlignment(.leading)
                        if let uncertain = warning.uncertain {
                            Text(uncertain)
                                .font(.ventiType(.label))
                                .multilineTextAlignment(.leading)
                        }
                    }
                    Spacer(minLength: VentiSpace.s1)
                    if warnings.count > 1 {
                        Text("+\(warnings.count - 1)")
                            .font(.ventiType(.heading))
                            .monospacedDigit()
                    }
                }
                .foregroundStyle(.venti(Self.ink(for: warning)))
                .padding(.horizontal, VentiSpace.s4)
                .padding(.vertical, VentiSpace.s2)
                .frame(maxWidth: .infinity, minHeight: VentiSize.touch, alignment: .leading)
                .background(.venti(Self.ground(for: warning)), in: .rect(cornerRadius: VentiRadius.m))
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(warning.silenced ? "Airspace warning, quieted" : "Airspace warning")
            .accessibilityValue("\(warning.headline). \(warning.detail). \(warning.uncertain ?? "")")
            .accessibilityHint("Shows all warnings")
            .accessibilityIdentifier("airspace-banner")
        }
    }

    /// The design language's alert when the glider is inside or about to
    /// be, its caution when very near, plain otherwise. A quieted warning
    /// is plain whatever its level. The words say the level too.
    static func ground(for warning: AirspaceWarning) -> VentiColor.Role {
        if warning.silenced {
            return .surfaceRaised
        }
        return switch warning.level {
        case .inside, .entering: .dangerFill
        case .veryNear: .cautionSoft
        case .near: .surfaceRaised
        }
    }

    static func ink(for warning: AirspaceWarning) -> VentiColor.Role {
        if warning.silenced {
            return .textSecondary
        }
        return switch warning.level {
        case .inside, .entering: .onDangerFill
        case .veryNear: .caution
        case .near: .text
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
                                .foregroundStyle(.venti(.textSecondary))
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
                        .foregroundStyle(.venti(.textSecondary))
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
