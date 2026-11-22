import SwiftUI
import VentiCore

/// The glider's class as the settings keep it and name it.
enum GliderChoice {
    /// What the preference holds: nothing for none, then the classes.
    static let stored = ["", "a", "b", "c", "d", "ccc"]

    /// A stored class for the core; `nil` for none and for anything else.
    static func kind(_ stored: String?) -> GliderClassKind? {
        switch stored {
        case "a": .a
        case "b": .b
        case "c": .c
        case "d": .d
        case "ccc": .ccc
        default: nil
        }
    }

    /// A class as the menu names it: "None", "EN A", "CCC".
    static func name(_ stored: String) -> String {
        switch stored {
        case "a": "EN A"
        case "b": "EN B"
        case "c": "EN C"
        case "d": "EN D"
        case "ccc": "CCC"
        default: "None"
        }
    }
}

/// What the pilot sets for the cockpit beyond its sound and its files: the
/// glider, the FAI triangle areas and the brightness. A page of its own,
/// so that the settings' first page stays short.
struct CockpitSettingsScreen: View {
    @AppStorage(Preferences.trimSpeed) private var trimSpeed = Preferences.defaultTrimSpeed
    @AppStorage(Preferences.glideRatio) private var glideRatio = Preferences.defaultGlideRatio
    @AppStorage(Preferences.gliderClass) private var gliderClass = ""
    @AppStorage(Preferences.neverLand) private var neverLand = false
    @AppStorage(Preferences.showsFaiAreas) private var showsFaiAreas = false
    @AppStorage(Preferences.cockpitKeepsBrightness) private var keepsBrightness = false

    var body: some View {
        VentiList {
            Section {
                Picker("Glider class", selection: $gliderClass) {
                    ForEach(GliderChoice.stored, id: \.self) { stored in
                        Text(GliderChoice.name(stored)).tag(stored)
                    }
                }
                .accessibilityIdentifier("glider-class")
                Stepper(value: $trimSpeed, in: 25...60, step: 1) {
                    LabeledContent("Trim speed", value: "\(Format.metres(trimSpeed)) km/h")
                }
                Stepper(value: $glideRatio, in: 4...14, step: 0.5) {
                    LabeledContent("Glide ratio", value: Format.glideRatio(glideRatio))
                }
                Toggle("Never land (hike and fly)", isOn: $neverLand)
                    .accessibilityIdentifier("never-land")
            } header: {
                Text("Glider")
            } footer: {
                Text("Your glider's speed with the brakes up and its glide ratio. From them and the wind, the cockpit predicts how high you arrive at the place you fly to. The prediction assumes air that neither rises nor sinks on the way. With a glider class the cockpit also says what the air itself does on a glide and when to speed up; its numbers are typical of the class, not your glider's. With \"Never land\" a flight is not ended by itself: landing, walking on and taking off again stay one recording, which you end by hand.")
            }

            Section {
                Toggle("FAI triangle areas", isOn: $showsFaiAreas)
                    .accessibilityIdentifier("fai-areas")
            } footer: {
                Text("The FAI triangle areas are drawn on the cockpit's map once you have flown three kilometres in a straight line: a turnpoint inside either of them makes that line one side of an FAI triangle.")
            }

            Section {
                Toggle(
                    "Full brightness in the cockpit",
                    isOn: Binding(
                        get: { !keepsBrightness },
                        set: { isOn in keepsBrightness = !isOn }
                    )
                )
            } header: {
                Text("In the sun")
            } footer: {
                Text("With full brightness the cockpit turns the screen all the way up while it is open and back afterwards; it costs battery. The cockpit is always drawn for the sun: near-black on white, whatever the iPhone's appearance.")
            }
        }
        .navigationTitle("Cockpit")
        .navigationBarTitleDisplayMode(.inline)
    }
}
