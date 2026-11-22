import MapLibre
import SwiftUI
import UIKit
import VentiCore

/// What a map draws of the air others found: a mark for each place where
/// pilots climb now, with the core's label ("+2.4") under it, and a
/// rectangle for each cell where the public flights' thermals have been,
/// the stronger the more it weighs. The climbs take the colour of their
/// rate from the pack's scale "Climb and sink", the cells one colour of
/// the scale "Thermals": no colour here is the app's own.
@MainActor
enum AirLayers {
    /// The climbs' marks on the map tab: the layer a tap looks at.
    static let climbs = "live-climbs"
    /// The cells of where thermals have been.
    static let flown = "flown-thermals"

    /// The pack's scale "Thermals" at its fourth step: strong enough over
    /// the relief, not the strongest.
    static var flownColour: UIColor { ThermalBand(scaleStep: 4).uiColorOnMap }

    /// How see-through the lightest and the heaviest cell are: the
    /// forecast squares' range.
    static let flownLeast = 0.2
    static let flownMost = 0.75

    /// Adds the layer of the cells to a style, empty. Call it before
    /// adding airspace and what else the app draws, so that all of that
    /// lies over it.
    static func addFlown(to style: MLNStyle) {
        let cells = MLNShapeSource(identifier: flown, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(cells)
        let fill = MLNFillStyleLayer(identifier: flown, source: cells)
        fill.fillColor = NSExpression(forConstantValue: flownColour)
        fill.fillAntialiased = NSExpression(forConstantValue: false)
        fill.fillOpacity = NSExpression(forKeyPath: "opacity")
        style.addLayer(fill)
    }

    /// Adds the layers of the climbs to a style, empty, under one name: a
    /// dot in the colour of the rate, the core's label under it.
    static func addClimbs(to style: MLNStyle, named name: String = climbs, textSize: Double = 12) {
        let marks = MLNShapeSource(identifier: name, shape: MLNShapeCollectionFeature(shapes: []), options: nil)
        style.addSource(marks)
        let dots = MLNCircleStyleLayer(identifier: name, source: marks)
        dots.circleRadius = NSExpression(forConstantValue: 7)
        var steps: [NSNumber: UIColor] = [:]
        for band in ClimbBand.allCases.dropFirst() {
            steps[NSNumber(value: band.rawValue)] = band.uiColor
        }
        dots.circleColor = NSExpression(
            forMLNStepping: NSExpression(forKeyPath: "band"),
            from: NSExpression(forConstantValue: ClimbBand.strongSink.uiColor),
            stops: NSExpression(forConstantValue: steps)
        )
        dots.circleStrokeColor = NSExpression(forConstantValue: Palette.haloOnMap)
        dots.circleStrokeWidth = NSExpression(forConstantValue: 2.5)
        style.addLayer(dots)
        let labels = MLNSymbolStyleLayer(identifier: name + "-labels", source: marks)
        labels.text = NSExpression(forKeyPath: "label")
        labels.textFontNames = NSExpression(forConstantValue: ["noto-sans-regular"])
        labels.textFontSize = NSExpression(forConstantValue: textSize)
        labels.textAnchor = NSExpression(forConstantValue: "top")
        labels.textOffset = NSExpression(forConstantValue: NSValue(cgVector: CGVector(dx: 0, dy: 0.9)))
        labels.textColor = NSExpression(forConstantValue: Palette.inkOnMap)
        labels.textHaloColor = NSExpression(forConstantValue: Palette.haloOnMap)
        labels.textHaloWidth = NSExpression(forConstantValue: 1.6)
        labels.textAllowsOverlap = NSExpression(forConstantValue: true)
        style.addLayer(labels)
    }

    /// Draws the climbs given in the source of that name; none empties it.
    static func show(_ climbs: [ClimbView], in style: MLNStyle, named name: String = climbs) {
        guard let source = style.source(withIdentifier: name) as? MLNShapeSource else { return }
        source.shape = MLNShapeCollectionFeature(shapes: features(for: climbs))
    }

    /// Draws the cells given; `nil` empties the layer.
    static func show(_ thermals: FlownThermals?, in style: MLNStyle) {
        guard let source = style.source(withIdentifier: flown) as? MLNShapeSource else { return }
        source.shape = MLNShapeCollectionFeature(shapes: features(for: thermals))
    }

    /// A climb as the map's feature: its place, the core's label, the step
    /// of its rate on the pack's scale, and its place in the list as its
    /// identifier.
    static func features(for climbs: [ClimbView]) -> [MLNPointFeature] {
        climbs.enumerated().map { index, climb in
            let feature = MLNPointFeature()
            feature.coordinate = climb.position.location
            feature.attributes = [
                "id": String(index),
                "label": climb.label,
                "band": ClimbBand(metresPerSecond: climb.climbMs).rawValue,
            ]
            return feature
        }
    }

    /// A cell as the map's feature: the rectangle of the answer's side
    /// around its middle, the more solid the more it weighs.
    static func features(for thermals: FlownThermals?) -> [MLNPolygonFeature] {
        guard let thermals else { return [] }
        let half = thermals.cellDeg / 2
        return thermals.cells.enumerated().map { index, cell in
            let (lat, lon) = (cell.position.lat, cell.position.lon)
            let ring = [
                CLLocationCoordinate2D(latitude: lat - half, longitude: lon - half),
                CLLocationCoordinate2D(latitude: lat - half, longitude: lon + half),
                CLLocationCoordinate2D(latitude: lat + half, longitude: lon + half),
                CLLocationCoordinate2D(latitude: lat + half, longitude: lon - half),
                CLLocationCoordinate2D(latitude: lat - half, longitude: lon - half),
            ]
            let feature = MLNPolygonFeature(coordinates: ring, count: UInt(ring.count), interiorPolygons: [])
            feature.attributes = ["id": String(index), "opacity": opacity(ofWeight: cell.weight)]
            return feature
        }
    }

    /// How solid a cell of that weight is drawn: from the lightest to the
    /// heaviest, in a straight line.
    static func opacity(ofWeight weight: Double) -> Double {
        flownLeast + (flownMost - flownLeast) * min(max(weight, 0), 1)
    }
}

/// A climb or a cell of where thermals have been, tapped on the map: the
/// core's line, and under it whose it is and what it is.
struct ChosenAirLine: Identifiable, Equatable {
    let title: String
    let line: String
    let notes: [String]
    /// What a test finds the line by: "climb-line" or "flown-line".
    let identifier: String
    var id: String { identifier + line }

    /// A climb of the layer "Climbs now", with the credit of the network
    /// that heard it.
    init(climb: ClimbView, among climbs: LiveClimbs) {
        title = mapLayersWords().climbsNow
        line = climb.line
        notes = climbs.credits.filter { $0.source == climb.source }.map(\.text)
        identifier = "climb-line"
    }

    /// A cell of where thermals have been, with what the layer is and
    /// what its flights are given under.
    init(cell: FlownCellView, of thermals: FlownThermals) {
        title = mapLayersWords().flownThermals
        line = cell.line
        notes = [thermals.note, thermals.terms].filter { !$0.isEmpty }
        identifier = "flown-line"
    }
}

/// What the map says of a climb or of a cell, tapped.
struct AirLineSheet: View {
    let chosen: ChosenAirLine
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Text(chosen.line)
                        .font(.venti(.headline))
                        .monospacedDigit()
                        .accessibilityIdentifier(chosen.identifier)
                } footer: {
                    VStack(alignment: .leading, spacing: VentiSpace.s2) {
                        ForEach(chosen.notes, id: \.self) { note in
                            Text(note)
                        }
                    }
                    .accessibilityIdentifier("air-line-notes")
                }
            }
            .navigationTitle(chosen.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                    .accessibilityIdentifier("air-line-close")
                }
            }
        }
    }
}

/// The two layers of what others found in the air, as the map tab asks
/// for them: the climbs when their layer is switched on, when the map
/// comes to rest elsewhere and again every half minute while the app is
/// in front; where thermals have been when its layer is switched on and
/// when the map rests elsewhere. Off, nothing is asked and what was read
/// is forgotten. And the sheet of a climb or a cell tapped.
struct AirOnTheMap: ViewModifier {
    /// The part of the map in view.
    let region: Bounds?
    /// The Venti asked, by its address; empty for none.
    let address: String
    @Binding var chosen: ChosenAirLine?

    @Environment(AirModel.self) private var air
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage(Preferences.mapShowsClimbs) private var showsClimbs = false
    @AppStorage(Preferences.mapShowsFlownThermals) private var showsFlown = false

    private struct Watch: Hashable {
        let isOn: Bool
        let region: Bounds?
        let address: String
        let isInFront: Bool
    }

    func body(content: Content) -> some View {
        content
            .task(id: Watch(isOn: showsClimbs, region: region, address: address, isInFront: scenePhase == .active)) {
                guard showsClimbs else {
                    await air.loadClimbs(nil, isOn: false)
                    return
                }
                guard scenePhase == .active else { return }
                // The map reports every pause of a drag; wait until it rests.
                try? await Task.sleep(for: .milliseconds(AirModel.restMilliseconds))
                while !Task.isCancelled {
                    await air.loadClimbs(region, isOn: true)
                    try? await Task.sleep(for: .seconds(AirModel.climbsEvery))
                }
            }
            .task(id: Watch(isOn: showsFlown, region: region, address: address, isInFront: true)) {
                guard showsFlown else {
                    await air.loadFlown(nil, isOn: false)
                    return
                }
                try? await Task.sleep(for: .milliseconds(AirModel.restMilliseconds))
                guard !Task.isCancelled else { return }
                await air.loadFlown(region, isOn: true)
            }
            .sheet(item: $chosen) { chosen in
                AirLineSheet(chosen: chosen)
                    .presentationDetents([.medium, .large])
            }
    }
}
