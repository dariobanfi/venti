import Foundation
import MapLibre
import Observation
import SwiftUI
import VentiCore

/// The cables around where the pilot looks or flies: OpenStreetMap's power
/// lines and ropeways, from Venti. The core makes the request and reads
/// the answer (`cablesRequest`, `CableSet`); this class sends it and keeps
/// the set for the maps and the cockpit.
@MainActor
@Observable
final class CablesModel {
    typealias Transport = (URL) async throws -> (Int, Data)

    /// The cables read from the last answer. Absent until one came.
    private(set) var set: CableSet?
    /// The same as a map layer: the lines, their credits and the core's
    /// sentence on what the data leaves out.
    private(set) var layer: CablesMap?
    /// What went wrong at the last ask, in words.
    private(set) var problem: String?

    /// The server to ask. The app sets this.
    @ObservationIgnored var server: () -> URL? = { nil }

    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private var isFetching = false
    @ObservationIgnored private var failedAt: Date?
    /// Where the set in hand was asked for, and from which server.
    @ObservationIgnored private var askedAt: Coordinate?
    @ObservationIgnored private var askedOf: URL?

    /// The core's box is some 65 by 65 km around the place asked for: a
    /// place this far from it is asked for anew, before its edge is near.
    static let askAgainBeyondM = 15_000.0
    /// After a failure the server is not asked again for this long.
    static let retryAfter: TimeInterval = 60

    init(transport: @escaping Transport = WindModel.fetch) {
        self.transport = transport
    }

    /// The lines to draw, or none.
    var lines: [CableLine] { layer?.lines ?? [] }

    /// Whether a place calls for another ask: nothing is held, it was
    /// asked of another server, or the place is far from where it was.
    func isDue(around place: Coordinate, server: URL) -> Bool {
        guard let askedAt, askedOf == server else { return true }
        return Self.metres(from: askedAt, to: place) > Self.askAgainBeyondM
    }

    /// Asks for the cables around a place when it is due. Called whenever
    /// the app learns where the pilot is looking or flying.
    func refresh(around place: Coordinate) async {
        guard !isFetching, let server = server(), isDue(around: place, server: server) else { return }
        if let failedAt, Date().timeIntervalSince(failedAt) < Self.retryAfter {
            return
        }
        isFetching = true
        defer { isFetching = false }
        do {
            let request = try cablesRequest(lat: place.lat, lon: place.lon)
            guard let url = URL(string: server.absoluteString + request.path) else { return }
            let (status, body) = try await transport(url)
            take(try CableSet.fromAnswer(status: UInt16(clamping: status), body: body), at: place, of: server)
        } catch {
            failedAt = Date()
            problem = error is CoreError
                ? error.words
                : "The server could not be reached for cables. \(error.localizedDescription)"
        }
    }

    /// Keeps a set read from an answer.
    func take(_ cables: CableSet, at place: Coordinate, of server: URL?) {
        set = cables
        let latest = cables.layer()
        if latest != layer {
            layer = latest
        }
        askedAt = place
        askedOf = server
        failedAt = nil
        problem = nil
    }

    /// The credit line's part for the cables, while any is drawn.
    var creditWords: String? {
        guard let layer, !layer.lines.isEmpty, !layer.credits.isEmpty else { return nil }
        return "Cables: \(layer.credits.map(\.provider).formatted(.list(type: .and)))"
    }

    nonisolated static func metres(from one: Coordinate, to other: Coordinate) -> Double {
        let radius = 6_371_000.0
        let lat1 = one.lat * .pi / 180
        let lat2 = other.lat * .pi / 180
        let dLat = lat2 - lat1
        let dLon = (other.lon - one.lon) * .pi / 180
        let a = sin(dLat / 2) * sin(dLat / 2) + cos(lat1) * cos(lat2) * sin(dLon / 2) * sin(dLon / 2)
        return 2 * radius * asin(min(1, sqrt(a)))
    }
}

/// How cables are drawn on a map: a line in the danger token over the
/// pack's light casing, solid where the cable's height is mapped and
/// dashed where it is not.
@MainActor
enum CablesLayers {
    static let source = "cables"
    /// The layers a tap looks at.
    static let lineLayers = ["cables-mapped", "cables-unmapped"]
    /// Below this zoom the lines are a tangle; they are left off.
    static let minimumZoom: Float = 12

    static func add(to style: MLNStyle, minimumZoom: Float = minimumZoom) {
        let lines = MLNShapeSource(identifier: source, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(lines)
        let casing = MapSetup.line("cables-casing", from: lines, color: Palette.haloOnMap, width: 5)
        casing.minimumZoomLevel = minimumZoom
        style.addLayer(casing)
        let mapped = MapSetup.line("cables-mapped", from: lines, color: Palette.dangerOnMap, width: 2)
        mapped.predicate = NSPredicate(format: "mapped == 1")
        mapped.minimumZoomLevel = minimumZoom
        style.addLayer(mapped)
        let unmapped = MapSetup.line("cables-unmapped", from: lines, color: Palette.dangerOnMap, width: 2)
        unmapped.lineDashPattern = NSExpression(forConstantValue: [3, 2])
        unmapped.lineCap = NSExpression(forConstantValue: "butt")
        unmapped.predicate = NSPredicate(format: "mapped == 0")
        unmapped.minimumZoomLevel = minimumZoom
        style.addLayer(unmapped)
    }

    /// Draws these cables in place of what was drawn.
    static func show(_ lines: [CableLine], in style: MLNStyle) {
        guard let shapes = style.source(withIdentifier: source) as? MLNShapeSource else { return }
        shapes.shape = MLNShapeCollectionFeature(shapes: lines.compactMap(feature))
    }

    static func feature(for line: CableLine) -> MLNPolylineFeature? {
        guard line.points.count >= 2 else { return nil }
        let points = line.points.map(\.location)
        let feature = MLNPolylineFeature(coordinates: points, count: UInt(points.count))
        feature.attributes = ["id": line.id, "mapped": line.heightMapped ? 1 : 0]
        return feature
    }
}

/// A cable tapped on the map: its words, whose data it is, and the core's
/// sentence on what the data leaves out.
struct ChosenCable: Identifiable {
    let line: CableLine
    let credits: [Credit]
    let limits: String
    var id: String { line.id }
}

struct CableSheet: View {
    let cable: ChosenCable
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Text(cable.line.words)
                        .font(.venti(.headline))
                        .accessibilityIdentifier("cable-words")
                } footer: {
                    VStack(alignment: .leading, spacing: VentiSpace.s2) {
                        ForEach(cable.credits, id: \.text) { credit in
                            Text(credit.text)
                        }
                        Text(cable.limits)
                            .accessibilityIdentifier("cable-limits")
                    }
                }
            }
            .navigationTitle(cable.line.name ?? "Cable")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
        .presentationDetents([.medium])
    }
}

/// The cockpit's cable warning, under the airspace warning: on the alert
/// red while it sounds, plain once seen. "Seen" quiets it for ten minutes,
/// as the airspace warning's does.
struct CableBanner: View {
    let warning: CableWarning
    let seen: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: VentiSpace.s3) {
            Image(systemName: warning.silenced ? "bell.slash.fill" : "exclamationmark.triangle.fill")
                .font(.ventiType(.heading))
                .frame(width: VentiSize.icon)
            Text(warning.words)
                .font(.ventiType(.heading))
                .multilineTextAlignment(.leading)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityIdentifier("cable-warning-words")
            if !warning.silenced {
                Button("Seen", action: seen)
                    .font(.ventiType(.heading))
                    .frame(minWidth: VentiSize.touchInFlight, minHeight: VentiSize.touchInFlight)
                    .accessibilityHint("Quiet for 10 minutes")
                    .accessibilityIdentifier("cable-seen")
            }
        }
        .foregroundStyle(.venti(warning.silenced ? .textSecondary : .onDangerFill))
        .padding(.horizontal, VentiSpace.s4)
        .padding(.vertical, VentiSpace.s2)
        .frame(maxWidth: .infinity, minHeight: VentiSize.touch, alignment: .leading)
        .background(.venti(warning.silenced ? .surfaceRaised : .dangerFill), in: .rect(cornerRadius: VentiRadius.m))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("cable-warning")
    }
}
