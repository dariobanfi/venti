import SwiftUI
import VentiCore

/// The airspace that matters next, as the cockpit shows it all the time:
/// small over a corner of the first page's map, large under the map of
/// the second. The words, the choice of airspace, the distances and the
/// heights are the core's (`CockpitView.airspaceProximity`); these views
/// place them and reckon nothing but where to draw the height line.

extension AirspaceProximity {
    /// The ground it stands on, as the warning band for it would: the
    /// alert red while the glider is about to be inside or is, the colour
    /// of caution when very near, plain otherwise and when quieted.
    var ground: VentiColor.Role {
        if silenced {
            return .surfaceRaised
        }
        return switch level {
        case .inside, .entering: .dangerFill
        case .veryNear: .cautionSoft
        case .near, nil: .surface
        }
    }

    /// Its ink: as the band's, and with no warning up the amber of the map
    /// for an airspace that only deserves attention.
    var ink: VentiColor.Role {
        if silenced {
            return .textSecondary
        }
        return switch level {
        case .inside, .entering: .onDangerFill
        case .veryNear: .caution
        case .near: .text
        case nil: policy == .caution ? .caution : .text
        }
    }

    /// What a screen reader says: the core's line, its doubt, and whether
    /// it is quieted.
    var spoken: String {
        [words, uncertain, silenced ? "quieted" : nil].compactMap { $0 }.joined(separator: ". ")
    }
}

/// The design language's airspace sign, or the quieted bell for an
/// airspace the pilot has quieted, as the band has them.
private struct ProximitySign: View {
    let silenced: Bool
    let size: CGFloat

    var body: some View {
        if silenced {
            Image(systemName: "bell.slash.fill")
                .resizable()
                .scaledToFit()
                .frame(width: size, height: size)
        } else {
            Image("icon-airspace")
                .resizable()
                .frame(width: size, height: size)
        }
    }
}

/// The small reading over a corner of the first page's map: the sign and
/// the core's short words, with the name where the corner has the room,
/// else the place and the height on two lines. Nothing in it is tapped.
struct AirspaceProximityBadge: View {
    let proximity: AirspaceProximity

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s2) {
            ProximitySign(silenced: proximity.silenced, size: VentiSize.icon)
            ViewThatFits(in: .horizontal) {
                Text("\(proximity.name) · \(proximity.shortWords)")
                    .fixedSize()
                Text(proximity.shortWords)
                    .fixedSize()
                VStack(alignment: .leading, spacing: 0) {
                    Text(proximity.placeWords)
                    // In the airspace the place says it all.
                    if !proximity.insideOutline {
                        Text(proximity.heightWords)
                    }
                }
            }
            .lineLimit(1)
        }
        .font(.ventiType(.label))
        .monospacedDigit()
        .foregroundStyle(.venti(proximity.ink))
        .padding(.horizontal, VentiSpace.s3)
        .padding(.vertical, VentiSpace.s2)
        .ventiFloating(in: .rect(cornerRadius: VentiRadius.m), ground: proximity.ground)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Airspace next")
        .accessibilityValue(proximity.spoken)
        .accessibilityIdentifier("airspace-proximity")
    }
}

/// The large reading under the map of the second page: the name, where
/// and how high as two readings side by side, the airspace's class and
/// limits, the doubt when there is one, and beside them the height line.
/// A tap lists the warnings, as a tap on the band does. Shown only with a
/// reading: without one the cockpit leaves it out, as Android does.
struct AirspaceProximityStrip: View {
    let proximity: AirspaceProximity
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(alignment: .top, spacing: VentiSpace.s3) {
                AirspaceHeightLine(scale: AirspaceHeightScale(proximity), ink: proximity.ink)
                    .frame(width: VentiSpace.s6, height: VentiSpace.s8)
                VStack(alignment: .leading, spacing: 0) {
                    HStack(spacing: VentiSpace.s2) {
                        ProximitySign(silenced: proximity.silenced, size: VentiSize.icon)
                        Text(proximity.name)
                            .font(.ventiType(.heading))
                            .lineLimit(1)
                    }
                    HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s4) {
                        Text(proximity.placeWords)
                        if !proximity.insideOutline || proximity.inHeight != .atHeight {
                            Text(proximity.heightWords)
                        }
                    }
                    .font(.ventiType(.instrumentM))
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
                    Text(proximity.detail)
                        .font(.ventiType(.label))
                        .lineLimit(1)
                    if let uncertain = proximity.uncertain {
                        Text(uncertain)
                            .font(.ventiType(.label))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
            }
            .foregroundStyle(.venti(proximity.ink))
            .padding(.horizontal, VentiSpace.s4)
            .padding(.vertical, VentiSpace.s2)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.venti(proximity.ground))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .fixedSize(horizontal: false, vertical: true)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Airspace next")
        .accessibilityValue(proximity.spoken)
        .accessibilityHint("Shows all warnings")
        .accessibilityIdentifier("airspace-proximity-map")
    }
}

/// Where the glider and the airspace lie on one scale of height, as parts
/// of the line's height from its foot (0) to its top (1). A floor far
/// below the glider (one at the ground where the session has no terrain)
/// runs off the foot, an airspace without a ceiling off the top.
struct AirspaceHeightScale: Equatable {
    /// The glider.
    let glider: Double
    /// The airspace's floor, 0 when it runs off the foot.
    let bottom: Double
    /// Its ceiling, 1 when it runs off the top.
    let top: Double
    /// Whether the floor lies below the foot of the line.
    let runsOffTheFoot: Bool
    /// Whether the ceiling lies above the top of the line, or there is none.
    let runsOffTheTop: Bool

    /// The most of the air below the glider the line shows.
    static let belowM = 1000.0
    /// The most of the air above the glider the line shows.
    static let aboveM = 1000.0
    /// The least height the line spans, so that a few metres do not fill it.
    static let leastM = 600.0

    init(_ proximity: AirspaceProximity) {
        self.init(altitudeM: proximity.altitudeM, floorM: proximity.floorM, ceilingM: proximity.ceilingM)
    }

    init(altitudeM: Double, floorM: Double, ceilingM: Double?) {
        // What lies within reach of the glider, and the glider itself.
        let shownFloor = max(floorM, altitudeM - Self.belowM)
        let shownCeiling = ceilingM.map { min($0, max(altitudeM, shownFloor) + Self.aboveM) }
        var low = min(altitudeM, shownFloor)
        var high = max(altitudeM, shownCeiling ?? shownFloor)
        // A little air at both ends, and never less than the least span.
        let span = max(high - low, Self.leastM)
        let middle = (low + high) / 2
        low = middle - span / 2 * 1.2
        high = middle + span / 2 * 1.2
        let part = { (metres: Double) in min(max((metres - low) / (high - low), 0), 1) }
        glider = part(altitudeM)
        runsOffTheFoot = floorM < low
        bottom = runsOffTheFoot ? 0 : part(floorM)
        if let ceilingM, ceilingM <= high {
            runsOffTheTop = false
            top = part(ceilingM)
        } else {
            runsOffTheTop = true
            top = 1
        }
    }
}

/// The height line drawn: a thin line for the air, the airspace on it as
/// a bar from its floor to its ceiling, and the glider as a mark beside
/// it. In the ink of the reading it stands in.
struct AirspaceHeightLine: View {
    let scale: AirspaceHeightScale
    let ink: VentiColor.Role

    @Environment(\.venti) private var theme

    var body: some View {
        Canvas { context, size in
            let colour = theme.color(ink)
            let y = { (part: Double) in size.height * (1 - part) }
            let barX = size.width * 0.55
            let barWidth = size.width * 0.3
            // The air: a thin line the height of the scale.
            context.fill(
                Path(CGRect(x: barX + barWidth / 2 - 1, y: 0, width: 2, height: size.height)),
                with: .color(colour.opacity(0.4))
            )
            // The airspace, from its floor to its ceiling.
            let bar = CGRect(
                x: barX, y: y(scale.top), width: barWidth,
                height: max(y(scale.bottom) - y(scale.top), 2)
            )
            context.fill(Path(bar), with: .color(colour))
            // The glider: a mark pointing at its height.
            let at = y(scale.glider)
            var mark = Path()
            mark.move(to: CGPoint(x: barX - 2, y: at))
            mark.addLine(to: CGPoint(x: 0, y: at - size.width * 0.25))
            mark.addLine(to: CGPoint(x: 0, y: at + size.width * 0.25))
            mark.closeSubpath()
            context.fill(mark, with: .color(colour))
            context.fill(
                Path(CGRect(x: 0, y: at - 1, width: size.width, height: 2)),
                with: .color(colour)
            )
        }
        .accessibilityHidden(true)
    }
}

/// The cross-country distance so far beside the map of the second page,
/// in the third page's words: "Triangle, 1.2 km to close".
struct ScoreBadge: View {
    let score: LiveScore

    var body: some View {
        let words = "\(Format.kilometres(metres: score.distanceM)) km · \(ScoreBadge.kind(score))"
        Text(words)
            .font(.ventiType(.label))
            .monospacedDigit()
            .lineLimit(1)
            .padding(.horizontal, VentiSpace.s3)
            .padding(.vertical, VentiSpace.s2)
            .ventiFloating(in: .capsule)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Cross-country distance")
            .accessibilityValue(words)
            .accessibilityIdentifier("cockpit-score")
    }

    /// How the distance scores, and for an open triangle how far it is to
    /// where it began: "Triangle, 1.2 km to close".
    static func kind(_ score: LiveScore) -> String {
        let kind = switch score.kind {
        case .freeFlight: "Free flight"
        case .freeTriangle: score.closed ? "Closed triangle" : "Triangle"
        case .faiTriangle: score.closed ? "Closed FAI triangle" : "FAI triangle"
        }
        if let gap = score.closingM, !score.closed, gap >= 100 {
            return "\(kind), \(Format.kilometres(metres: gap)) km to close"
        }
        return kind
    }
}
