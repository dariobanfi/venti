import SwiftUI
import VentiCore

/// The wind in each band of height the glider has circled in, the highest
/// first. The wind in the valley and the wind above the ridges are two
/// winds, and a pilot deciding where to go next wants both.
///
/// It reads the running session, so the list fills while it is open.
struct WindByHeightSheet: View {
    /// Whether the wind on the ground comes first: the pilot tapped its
    /// badge or its warning, not the wind aloft.
    var groundFirst = false

    @Environment(FlySession.self) private var fly
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                if groundFirst {
                    onTheGround
                    byHeight
                } else {
                    byHeight
                    onTheGround
                }
            }
            .navigationTitle(groundFirst ? "Wind on the ground" : "Wind by height")
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

extension WindByHeightSheet {
    /// The wind in each band of height the glider has circled in.
    @ViewBuilder fileprivate var byHeight: some View {
        let bands = fly.view?.windsByHeight ?? []
        Section {
            // The wind the cockpit shows, in full: how strong, where
            // from, what it was measured from and how old it is.
            if let words = fly.view?.windWords {
                Text(words)
                    .accessibilityIdentifier("wind-words")
            }
            if bands.isEmpty {
                Text("No wind measured yet. It is worked out from your drift while you circle.")
                    .foregroundStyle(.venti(.textSecondary))
            }
            ForEach(bands, id: \.floorM) { band in
                WindByHeightRow(band: band)
            }
        } header: {
            if groundFirst {
                Text("By height")
            }
        } footer: {
            Text("Measured from your drift while you circled, as the mean over each visit to a band of 200 m. A band you have not circled in has no wind to show, and an old one may have changed. Under the wind stands how fast you climbed through the band while circling.")
        }
    }

    /// What the wind stations around measured last: the wind down where
    /// the landings are.
    @ViewBuilder fileprivate var onTheGround: some View {
        if let below = fly.windBelow, !below.stations.isEmpty {
            Section {
                ForEach(below.stations, id: \.id) { station in
                    GroundWindRow(station: station)
                }
            } header: {
                Text("On the ground")
            } footer: {
                Text(groundFootnote)
            }
        }
    }

    /// What the stations' numbers are, and whose.
    fileprivate var groundFootnote: String {
        let what = "The nearest wind stations within 75 km with a reading of the last hour, as Venti had them when it was last asked, every two minutes in flight. A station measures where it stands, and some report three quarters of an hour late: the age stands beside each. Mean wind and gusts in km/h."
        let whose = fly.windBelowCredits.map(\.text).joined(separator: " · ")
        return whose.isEmpty ? what : "\(what)\n\(whose)"
    }
}

/// One wind station: where the air moves there, its name and where it
/// stands, and what it measured.
private struct GroundWindRow: View {
    let station: GroundWindStation

    var body: some View {
        HStack(spacing: 12) {
            Group {
                if let from = station.fromDeg {
                    Image(systemName: "arrow.up")
                        .rotationEffect(.degrees(from + 180))
                } else {
                    Image(systemName: "circle.dotted")
                }
            }
            .font(.venti(.headline, .bold))
            .foregroundStyle(windBand(speedMs: station.speedMs).ink)
            .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(station.name)
                    .font(.venti(.body, station.isAtLanding ? .semibold : .regular))
                    .lineLimit(1)
                Text(station.isAtLanding ? "At your landing · \(station.place)" : station.place)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.venti(.textSecondary))
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            // The wind over its age, each on a line of its own: side by
            // side they broke in the middle of "5 min ago".
            VStack(alignment: .trailing, spacing: 2) {
                Text(station.wind)
                    .font(.venti(.subheadline))
                    .foregroundStyle(station.isStrong ? Color.venti(.danger) : Color.venti(.text))
                Text(station.age)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.venti(.textSecondary))
            }
            .monospacedDigit()
            .lineLimit(1)
            // The numbers keep their width; a long name gives way.
            .layoutPriority(1)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("ground-wind-row")
    }
}

/// One band: where the air moves, the band's heights, and the wind in
/// numbers.
private struct WindByHeightRow: View {
    let band: WindAtHeightView

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "arrow.up")
                .font(.venti(.headline, .bold))
                // The arrow shows where the wind goes, north at the top.
                .rotationEffect(.degrees(band.fromDeg + 180))
                .foregroundStyle(windBand(speedMs: band.speedMs).ink)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text("\(Format.metres(band.floorM)) to \(Format.metres(band.ceilingM)) m")
                    .font(.venti(.body, band.isHere ? .semibold : .regular))
                    .monospacedDigit()
                Text(band.isHere ? "You are here · measured \(age)" : "Measured \(age)")
                    .font(.venti(.subheadline))
                    .foregroundStyle(.venti(.textSecondary))
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text("\(Format.kilometresPerHour(metresPerSecond: band.speedMs)) km/h \(Format.compassPoint(degrees: band.fromDeg))")
                    .font(.venti(.body, .semibold))
                    .monospacedDigit()
                // How the thermal climbed through this band: where it is
                // strong, and where it weakens toward the top.
                // The colour of the climb is a mark beside the number: text
                // is not coloured by a scale.
                if let climb = band.climbMs {
                    HStack(spacing: 6) {
                        Circle()
                            .fill(ClimbBand(metresPerSecond: climb).color)
                            .frame(width: 10, height: 10)
                        Text("\(Format.climb(metresPerSecond: climb)) m/s")
                            .font(.venti(.subheadline, .medium))
                            .monospacedDigit()
                            .foregroundStyle(.venti(.textSecondary))
                    }
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(Format.metres(band.floorM)) to \(Format.metres(band.ceilingM)) metres\(band.isHere ? ", where you are" : "")")
        .accessibilityValue(
            "\(Format.spokenKilometresPerHour(metresPerSecond: band.speedMs)) from \(Format.compassPoint(degrees: band.fromDeg)), measured \(age)"
                + (band.climbMs.map { ", climbed \(Format.climb(metresPerSecond: $0)) metres per second" } ?? "")
        )
        .accessibilityIdentifier("wind-by-height-row")
    }

    private var age: String {
        Format.age(minutes: Int64(band.ageS / 60))
    }
}
