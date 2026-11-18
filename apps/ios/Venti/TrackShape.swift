import SwiftUI
import VentiCore

/// A track drawn to fit a rectangle, north up.
///
/// This is a plain line drawing for the first app shell. The map replaces it.
struct TrackShape: Shape {
    let points: [TrackPoint]
    let bounds: Bounds

    func path(in rect: CGRect) -> Path {
        var path = Path()
        guard points.count > 1, rect.width > 0, rect.height > 0 else { return path }

        // A degree of longitude is shorter than a degree of latitude by the
        // cosine of the latitude; without this the track looks stretched.
        let middleLatitude = (bounds.north + bounds.south) / 2
        let eastWestScale = cos(middleLatitude * .pi / 180)
        let width = max((bounds.east - bounds.west) * eastWestScale, 1e-9)
        let height = max(bounds.north - bounds.south, 1e-9)
        let scale = min(rect.width / width, rect.height / height)
        let offsetX = rect.minX + (rect.width - width * scale) / 2
        let offsetY = rect.minY + (rect.height - height * scale) / 2

        func place(_ point: TrackPoint) -> CGPoint {
            CGPoint(
                x: offsetX + (point.lon - bounds.west) * eastWestScale * scale,
                y: offsetY + (bounds.north - point.lat) * scale
            )
        }

        path.move(to: place(points[0]))
        for point in points.dropFirst() {
            path.addLine(to: place(point))
        }
        return path
    }
}
