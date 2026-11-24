import SwiftUI
import VentiCore

/// What the cockpit knows beyond its pages' readings, each a row in the
/// core's words and absent while the core has none: where the wind is
/// from, the airspeed and the heading it estimates, where the altitude
/// comes from, the QNH; and for a pilot who set a glider class, what the
/// air itself does and the speed to fly. Opened from the third page.
///
/// It reads the running session, so the rows change while it is open.
struct CockpitDetailsSheet: View {
    @Environment(FlySession.self) private var fly
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let rows = Self.rows(of: fly.view)
        let words = cockpitDetailsWords()
        NavigationStack {
            VentiList {
                Section {
                    if rows.isEmpty {
                        Text(words.noneYet)
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityIdentifier("cockpit-details-none")
                    }
                    ForEach(rows, id: \.name) { row in
                        VStack(alignment: .leading, spacing: VentiSpace.s1) {
                            Text(row.name)
                                .font(.ventiType(.label))
                                .foregroundStyle(.venti(.textSecondary))
                            Text(row.words)
                                .font(.ventiType(.heading))
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(row.name)
                        .accessibilityValue(row.words)
                        .accessibilityIdentifier("cockpit-detail")
                    }
                } footer: {
                    if rows.contains(where: { $0.name == words.airspeed }) {
                        Text(words.note)
                    }
                }
            }
            // The first row under the bar: a grouped list begins with the
            // room of a heading, and this one has none.
            .contentMargins(.top, VentiSpace.s2, for: .scrollContent)
            .navigationTitle(words.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                }
            }
        }
    }

    /// The rows the view has words for, in a fixed order.
    static func rows(of view: CockpitView?) -> [(name: String, words: String)] {
        guard let view else { return [] }
        // The names are the core's.
        let words = cockpitDetailsWords()
        let all: [(String, String?)] = [
            (words.wind, view.windWords),
            (words.airspeed, view.airspeedWords),
            (words.heading, view.headingWords),
            (words.altitude, view.altitudeSourceWords),
            (words.qnh, view.qnhWords),
            (words.air, view.nettoWords),
            (words.speedToFly, view.speedToFly?.words),
        ]
        return all.compactMap { name, words in
            words.map { (name: name, words: $0) }
        }
    }
}
