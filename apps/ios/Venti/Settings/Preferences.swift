/// The names under which the app keeps the pilot's settings on the device.
enum Preferences {
    /// The pilot's name, written into recorded flights.
    static let pilot = "VentiPilotName"
    /// The glider, written into recorded flights.
    static let glider = "VentiGliderName"
    /// Whether the vario is silent.
    static let varioMuted = "VentiVarioMuted"
    /// The model the assistant uses.
    static let assistantModel = "VentiAssistantModel"
    /// Where the assistant's model runs: "anthropic", or "other" for a
    /// service the pilot set by its address.
    static let assistantService = "VentiAssistantService"
    /// That other service's address, and its name for the model to use.
    static let assistantAddress = "VentiAssistantAddress"
    static let assistantOtherModel = "VentiAssistantOtherModel"
    /// The address of the Venti server that live wind comes from.
    static let server = "VentiServerAddress"
    /// The glider's speed with the brakes up, km/h.
    static let trimSpeed = "VentiGliderTrimSpeed"
    /// The glider's glide ratio at that speed.
    static let glideRatio = "VentiGliderGlideRatio"
    /// Whether the cockpit's map leaves out the FAI triangle areas.
    static let hidesFaiAreas = "VentiCockpitHidesFaiAreas"
    /// Whether the pilot has switched off airspace from the server.
    static let airspaceWithoutServer = "VentiAirspaceWithoutServer"

    /// What the core assumes of a glider until the pilot says otherwise.
    static let defaultTrimSpeed = 38.0
    static let defaultGlideRatio = 8.0
}
