import MapLibre
import SwiftUI

/// The round button that switches a map between the drawn ground and the
/// aerial view: photographs from above, with the names and everything the
/// app draws left on them.
///
/// It is one choice for every map of the app, and the button keeps it: a
/// screen with a map reads the same preference and hands it to its map.
struct AerialButton: View {
    @AppStorage(Preferences.mapShowsAerial) private var showsAerial = false

    var body: some View {
        Button {
            showsAerial.toggle()
        } label: {
            Label("Aerial view", image: "icon-layers")
        }
        .buttonStyle(.ventiRound(isOn: showsAerial))
        .accessibilityIdentifier("map-aerial")
        .accessibilityAddTraits(showsAerial ? .isSelected : [])
    }
}

/// Who made the photographs in view on one map, in short: said on the
/// map while they show. Each provider's own wording is behind the map's
/// credits button, which a phone's map folds away. A screen keeps one for
/// its map, and the map works the line out again when it comes to rest.
@Observable
@MainActor
final class PhotoCredits {
    /// "Photos: basemap.at · Bayerische Vermessungsverwaltung · EOX";
    /// empty until the map has its style.
    private(set) var line = ""

    /// The line for where the map now rests. The map says so while
    /// SwiftUI may be drawing: the line changes after that.
    func update(from map: MLNMapView) {
        guard let line = MapSetup.photoCredit(on: map), line != self.line else { return }
        DispatchQueue.main.async {
            self.line = line
        }
    }
}

/// The photographs' makers in one short line over a map, while the aerial
/// view is on. The map tab says the same in its own line of credits.
struct PhotographCredit: View {
    let credits: PhotoCredits
    @AppStorage(Preferences.mapShowsAerial) private var showsAerial = false

    var body: some View {
        if showsAerial, !credits.line.isEmpty {
            Text(credits.line)
                .font(.venti(.caption2))
                .foregroundStyle(.venti(.textSecondary))
                .lineLimit(2)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .ventiFloating(in: .rect(cornerRadius: 10))
                .accessibilityIdentifier("photo-credits")
        }
    }
}
