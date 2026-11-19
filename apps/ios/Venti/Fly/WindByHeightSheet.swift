import SwiftUI
import VentiCore

/// The wind in each band of height the glider has circled in, the highest
/// first. The wind in the valley and the wind above the ridges are two
/// winds, and a pilot deciding where to go next wants both.
///
/// It reads the running session, so the list fills while it is open.
struct WindByHeightSheet: View {
    @Environment(FlySession.self) private var fly
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                let bands = fly.view?.windsByHeight ?? []
                Section {
                    if bands.isEmpty {
                        Text("No wind measured yet. It is worked out from your drift while you circle.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(bands, id: \.floorM) { band in
                        WindByHeightRow(band: band)
                    }
                } footer: {
                    Text("Measured from your drift while you circled, as the mean over each visit to a band of 200 m. A band you have not circled in has no wind to show, and an old one may have changed.")
                }
            }
            .navigationTitle("Wind by height")
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
}

/// One band: where the air moves, the band's heights, and the wind in
/// numbers.
private struct WindByHeightRow: View {
    let band: WindAtHeightView

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "arrow.up")
                .font(.headline.weight(.bold))
                // The arrow shows where the wind goes, north at the top.
                .rotationEffect(.degrees(band.fromDeg + 180))
                .foregroundStyle(windBand(speedMs: band.speedMs).ink)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text("\(Format.metres(band.floorM)) to \(Format.metres(band.ceilingM)) m")
                    .font(.body.weight(band.isHere ? .semibold : .regular))
                    .monospacedDigit()
                Text(band.isHere ? "You are here · measured \(age)" : "Measured \(age)")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            Text("\(Format.kilometresPerHour(metresPerSecond: band.speedMs)) km/h \(Format.compassPoint(degrees: band.fromDeg))")
                .font(.system(.body, design: .rounded, weight: .semibold))
                .monospacedDigit()
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(Format.metres(band.floorM)) to \(Format.metres(band.ceilingM)) metres\(band.isHere ? ", where you are" : "")")
        .accessibilityValue(
            "\(Format.spokenKilometresPerHour(metresPerSecond: band.speedMs)) from \(Format.compassPoint(degrees: band.fromDeg)), measured \(age)"
        )
        .accessibilityIdentifier("wind-by-height-row")
    }

    private var age: String {
        Format.age(minutes: Int64(band.ageS / 60))
    }
}
