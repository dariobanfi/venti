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
    /// The layers of the map tab the pilot has switched off with its
    /// chips. All are shown until then.
    static let mapHidesWind = "VentiMapHidesWind"
    static let mapHidesAirspace = "VentiMapHidesAirspace"
    static let mapHidesSites = "VentiMapHidesSites"
    static let mapHidesPilots = "VentiMapHidesPilots"
    /// Whether the maps show the aerial view, photographs from above, in
    /// place of the drawn ground. One choice for every map of the app.
    static let mapShowsAerial = "VentiMapShowsAerial"
    /// Whether the pilot has switched off airspace from the server.
    static let airspaceWithoutServer = "VentiAirspaceWithoutServer"

    /// Whom flights are shown to while they are under way: "link",
    /// "public", or nothing for nobody.
    static let liveAudience = "VentiLiveAudience"
    /// Whether the cockpit leaves the screen's brightness as it is. By
    /// itself it turns it all the way up for as long as it is open.
    static let cockpitKeepsBrightness = "VentiCockpitKeepsBrightness"

    /// What the core assumes of a glider until the pilot says otherwise.
    static let defaultTrimSpeed = 38.0
    static let defaultGlideRatio = 8.0
}
