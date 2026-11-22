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

    /// A class as the menu names it, in the core's words: "None",
    /// "EN A", "CCC".
    static func name(_ stored: String) -> String {
        gliderClassName(class: kind(stored))
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
    @AppStorage(Preferences.cockpitShowsClimbs) private var showsClimbs = false

    var body: some View {
        let glider = gliderWords()
        VentiList {
            Section {
                Picker(glider.classTitle, selection: $gliderClass) {
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
                Toggle(glider.neverLand, isOn: $neverLand)
                    .accessibilityIdentifier("never-land")
            } header: {
                Text(glider.title)
            } footer: {
                // The first sentences, about the two numbers, are the
                // app's: the core's footer speaks of the class and of
                // "Never land".
                Text("Your glider's speed with the brakes up and its glide ratio. From them and the wind, the cockpit predicts how high you arrive at the place you fly to. The prediction assumes air that neither rises nor sinks on the way. \(glider.footer)")
            }

            Section {
                Toggle("FAI triangle areas", isOn: $showsFaiAreas)
                    .accessibilityIdentifier("fai-areas")
            } footer: {
                Text("The FAI triangle areas are drawn on the cockpit's map once you have flown three kilometres in a straight line: a turnpoint inside either of them makes that line one side of an FAI triangle.")
            }

            // Where others climb near the glider, on the cockpit's map:
            // off until the pilot asks for it, and then nothing is asked
            // of Venti for it. The name and the line are the core's.
            Section {
                Toggle(mapLayersWords().cockpitClimbs, isOn: $showsClimbs)
                    .accessibilityIdentifier("cockpit-climbs")
            } footer: {
                Text(mapLayersWords().cockpitClimbsNote)
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
        // The page is called what the row that opens it says.
        .navigationTitle(pagesWords().gliderAndCockpit)
        .navigationBarTitleDisplayMode(.inline)
    }
}
