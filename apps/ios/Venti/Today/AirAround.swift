import SwiftUI
import VentiCore

/// The air around and above a take-off, as the screens word it: the wind
/// near by and the layers of an hour. Which wind is much stronger, where a
/// layer lies and every sentence about either are the core's; here are only
/// the names of the two figures and how an hour is read out.
enum AirAround {
    static let windNearBy = "Wind near by"
    static let gustsNearBy = "Gusts near by"

    /// One of the two figures of an hour: its name and what it says.
    struct Figure: Equatable, Identifiable {
        let name: String
        let words: String

        var id: String { name }
    }

    /// The hour on the place's clock as its detail is headed: "14:00".
    static func clock(_ hour: OutlookHour) -> String {
        "\(hour.hour):00"
    }

    /// The strongest wind and the strongest gust near by with where they
    /// are, as the core gives them and in the words of its own line: "28
    /// km/h 20 km to the south over ground at 2400 m". The ground's height
    /// stands with the figure because a square on a ridge has more wind
    /// than a valley on any day. None for an hour without a neighbourhood,
    /// and none for a figure the model's squares do not have.
    static func figures(_ nearby: OutlookNearby?) -> [Figure] {
        guard let nearby else { return [] }
        func figure(_ name: String, kmh: String, place: String, groundM: Double?) -> Figure? {
            guard !kmh.isEmpty else { return nil }
            var words = "\(kmh) km/h"
            if !place.isEmpty {
                words += " \(place)"
            }
            if let groundM {
                words += " over ground at \(Format.metres(groundM)) m"
            }
            return Figure(name: name, words: words)
        }
        return [
            figure(windNearBy, kmh: nearby.windKmh, place: nearby.windWhere, groundM: nearby.windGroundM),
            figure(gustsNearBy, kmh: nearby.gustKmh, place: nearby.gustWhere, groundM: nearby.gustGroundM),
        ].compactMap { $0 }
    }

    /// Whether an hour has something to say beyond the picture: a line, a
    /// figure or a layer.
    static func hasDetail(_ hour: OutlookHour) -> Bool {
        !hour.layers.isEmpty || !figures(hour.nearby).isEmpty || !(hour.nearby?.line ?? "").isEmpty
    }

    /// An hour of the picture read out: what is much stronger near by,
    /// then its layers, then the two figures. Empty for an hour with none
    /// of them.
    static func spoken(_ hour: OutlookHour) -> String {
        let line = hour.nearby?.line ?? ""
        return ((line.isEmpty ? [] : [line])
            + hour.layers.map(\.words)
            + figures(hour.nearby).map { "\($0.name): \($0.words)" })
            .joined(separator: ". ")
    }
}

extension AirScale {
    /// The part of a stretch of air between two heights that the picture
    /// reaches, the lower height first: a layer that goes on above the
    /// ceiling or below the floor is cut there. None for one that lies
    /// wholly outside, and for one without any height.
    func within(bottomM: Double, topM: Double) -> (bottomM: Double, topM: Double)? {
        let bottom = max(bottomM, floorM)
        let top = min(topM, ceilingM)
        return top > bottom ? (bottom, top) : nil
    }
}

/// The wind near by when it is much stronger than at the take-off, as a
/// note in the caution manner: the pack's caution sign in its colour beside
/// the core's line, as the cards over the map say a caution. It is no
/// verdict, so it has no ground and no pill, and stands under the day's own
/// reason and never in the status's place. Nothing where the line is empty.
struct NearbyLine: View {
    let words: String
    var identifier = "nearby-line"

    var body: some View {
        if !words.isEmpty {
            // The words in the quiet ink of the day's own reason: the sign
            // marks them, and they are not to be louder than the reason.
            CardCaution(words: words)
                .foregroundStyle(.venti(.textSecondary))
                .multilineTextAlignment(.leading)
                .accessibilityIdentifier(identifier)
        }
    }
}

/// One hour of the picture in words, under it: what is much stronger near
/// by, the hour's layers each beside the band that marks it in the
/// picture, and the strongest wind and gust near by as two quiet figures.
struct HourAir: View {
    let hour: OutlookHour

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            Text(AirAround.clock(hour))
                .font(.ventiType(.label).weight(.semibold))
                .monospacedDigit()
                .foregroundStyle(.venti(.text))
                .accessibilityIdentifier("air-hour-clock")
            NearbyLine(words: hour.nearby?.line ?? "", identifier: "air-hour-line")
            ForEach(Array(hour.layers.enumerated()), id: \.offset) { _, layer in
                // The band stands on the first line of its words.
                HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                    // The band as the picture draws it, so that the words
                    // name their mark.
                    Canvas { context, size in
                        LayerBand.draw(
                            in: &context, band: CGRect(origin: .zero, size: size), inversion: layer.inversion
                        )
                    }
                    .frame(width: VentiSpace.s4, height: VentiSpace.s3)
                    .accessibilityHidden(true)
                    Text(layer.words)
                        .font(.ventiType(.caption))
                        .foregroundStyle(.venti(.textSecondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("air-hour-layer")
            }
            ForEach(AirAround.figures(hour.nearby)) { figure in
                HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s2) {
                    Text(figure.name)
                    Text(figure.words)
                        .monospacedDigit()
                        .fixedSize(horizontal: false, vertical: true)
                }
                .font(.ventiType(.caption))
                .foregroundStyle(.venti(.textSecondary))
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(figure.name)
                .accessibilityValue(figure.words)
                .accessibilityIdentifier("air-hour-figure")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A layer's band in the picture of the day's air: filled, with an edge
/// above and below, for an inversion; hatched for a stable layer. The pack
/// has no token for a band, so both are drawn in `text-muted`, the stand-in
/// the design document proposes; the fill is thinned so that the arrows and
/// their speeds over it stay readable.
enum LayerBand {
    /// How much of `text-muted` an inversion's fill takes.
    static let inversionFill = 0.4
    /// How much of it a stable layer's lines take: quieter than the arrows
    /// drawn over them.
    static let hatchInk = 0.7
    /// How far apart the lines of a stable layer stand.
    static let hatchStep = VentiSpace.s2

    static func draw(in context: inout GraphicsContext, band: CGRect, inversion: Bool) {
        guard band.width > 0, band.height > 0 else { return }
        let ink = Color.venti(.textMuted)
        if inversion {
            context.fill(Path(band), with: .color(ink.opacity(inversionFill)))
            var edges = Path()
            edges.move(to: CGPoint(x: band.minX, y: band.minY))
            edges.addLine(to: CGPoint(x: band.maxX, y: band.minY))
            edges.move(to: CGPoint(x: band.minX, y: band.maxY))
            edges.addLine(to: CGPoint(x: band.maxX, y: band.maxY))
            context.stroke(edges, with: .color(ink), lineWidth: 1)
        } else {
            // Lines rising to the right, on one grid for the whole picture
            // (x + y a multiple of the step), so that the bands of
            // neighbouring hours read as one layer through the day
            // wherever each begins and ends.
            var clipped = context
            clipped.clip(to: Path(band))
            var lines = Path()
            var sum = ((band.minX + band.minY) / hatchStep).rounded(.down) * hatchStep
            while sum - band.maxY < band.maxX {
                lines.move(to: CGPoint(x: sum - band.maxY, y: band.maxY))
                lines.addLine(to: CGPoint(x: sum - band.minY, y: band.minY))
                sum += hatchStep
            }
            clipped.stroke(lines, with: .color(ink.opacity(hatchInk)), lineWidth: 1)
        }
    }
}
