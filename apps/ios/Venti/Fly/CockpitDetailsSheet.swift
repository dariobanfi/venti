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
        NavigationStack {
            VentiList {
                Section {
                    if rows.isEmpty {
                        Text("Nothing to show yet.")
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
                    if rows.contains(where: { $0.name == "Airspeed" }) {
                        Text("Airspeed and heading are estimates: your way over the ground less the wind last measured.")
                    }
                }
            }
            .navigationTitle("Details")
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

    /// The rows the view has words for, in a fixed order.
    static func rows(of view: CockpitView?) -> [(name: String, words: String)] {
        guard let view else { return [] }
        let all: [(String, String?)] = [
            ("Wind", view.windWords),
            ("Airspeed", view.airspeedWords),
            ("Heading", view.headingWords),
            ("Altitude", view.altitudeSourceWords),
            ("QNH", view.qnhWords),
            ("Air", view.nettoWords),
            ("Speed to fly", view.speedToFly?.words),
        ]
        return all.compactMap { name, words in
            words.map { (name: name, words: $0) }
        }
    }
}
