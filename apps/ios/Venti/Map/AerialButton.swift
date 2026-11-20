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
