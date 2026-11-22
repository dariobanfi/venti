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
    /// The glider's speed with the brakes up, km/h.
    static let trimSpeed = "VentiGliderTrimSpeed"
    /// The glider's glide ratio at that speed.
    static let glideRatio = "VentiGliderGlideRatio"
    /// Whether the cockpit's map draws the FAI triangle areas: off until
    /// the pilot switches them on.
    static let showsFaiAreas = "VentiCockpitShowsFaiAreas"
    /// The class the glider is certified in: "a", "b", "c", "d", "ccc",
    /// or nothing for none, which is how it starts. With one the cockpit
    /// says what the air itself does on a glide and when to speed up.
    static let gliderClass = "VentiGliderClass"
    /// Whether a flight is never taken for landed (hike and fly): the
    /// pilot ends it.
    static let neverLand = "VentiFlightNeverLands"
    /// The layers of the map tab the pilot has switched off with its
    /// chips. All are shown until then.
    static let mapHidesWind = "VentiMapHidesWind"
    static let mapHidesSites = "VentiMapHidesSites"
    static let mapHidesPilots = "VentiMapHidesPilots"
    static let mapHidesCables = "VentiMapHidesCables"
    /// Whether the map tab draws airspace: off until the pilot switches it
    /// on, with the round button over the map or in the layers sheet. The
    /// cockpit draws all of it whatever this says.
    static let mapShowsAirspace = "VentiMapShowsAirspace"
    /// Whether the maps show the aerial view, photographs from above, in
    /// place of the drawn ground. One choice for every map of the app.
    static let mapShowsAerial = "VentiMapShowsAerial"
    /// Whether the map tab shows the forecast map, the kind of day square
    /// by square. Off until the pilot switches it on.
    static let mapShowsForecast = "VentiMapShowsForecast"
    /// Whether the map tab shows the rain radar, the last frames of rain
    /// the radars saw. Off until the pilot switches it on.
    static let mapShowsRadar = "VentiMapShowsRadar"
    /// Whether the map tab shows where gliders climb at this moment, and
    /// where the public flights' thermals have been: two layers of the
    /// layers sheet, each off until the pilot switches it on.
    static let mapShowsClimbs = "VentiMapShowsClimbs"
    static let mapShowsFlownThermals = "VentiMapShowsFlownThermals"
    /// Whether the map tab shows the thermal map of thermal.kk7.ch: off
    /// until switched on in the layers sheet.
    static let mapShowsThermalMap = "VentiMapShowsThermalMap"
    /// Whether the cockpit's map shows it: on until switched off in the
    /// cockpit's settings, at 40 % as the owner's XCTrack layout has it.
    static let cockpitHidesThermalMap = "VentiCockpitHidesThermalMap"
    /// Whether the cockpit asks for the climbs of others around the
    /// glider and draws them on its map: off until the pilot switches it
    /// on, and then nothing is asked or drawn.
    static let cockpitShowsClimbs = "VentiCockpitShowsClimbs"
    /// The height up to which the maps draw airspace, in metres; 0 for all
    /// heights. One choice for every map that draws airspace but the
    /// cockpit's, which draws all of it.
    static let mapAirspaceCeilingM = "VentiMapAirspaceCeilingM"
    /// The layer of the forecast map: "thermals", "wind", "gusts",
    /// "cloud" or "rain". Kept as the pilot left it.
    static let mapForecastLayer = "VentiMapForecastLayer"
    /// The height of the forecast map's wind above the sea, metres; -1
    /// for the ground. Kept as the pilot left it.
    static let mapForecastLevelM = "VentiMapForecastLevelM"
    /// What the pilot asks of the flying sites, on the map and in their
    /// list: which sites ("all", "takeoffs", "landings"), the wind a
    /// take-off is to work in ("any", "measured", or where it comes from
    /// in degrees) and the height take-offs lie at or above, metres; 0
    /// for any. Kept as the pilot left them.
    static let mapSitesShown = "VentiMapSitesShown"
    static let mapSitesWind = "VentiMapSitesWind"
    static let mapSitesFromM = "VentiMapSitesFromM"
    /// Whether the map and the list show the places from pilots, the
    /// crowdsourced layer: off until the pilot switches it on.
    static let mapSitesFromPilots = "VentiMapSitesFromPilots"
    /// Whether the map shows the places where pilots take off and no site
    /// stands: off until the pilot switches it on.
    static let mapShowsCandidates = "VentiMapShowsCandidates"
    /// Whether the pilot has switched off airspace from the server.
    static let airspaceWithoutServer = "VentiAirspaceWithoutServer"

    /// Whom flights are shown to while they are under way: "link",
    /// "public", or nothing for nobody.
    static let liveAudience = "VentiLiveAudience"
    /// Whether the cockpit leaves the screen's brightness as it is. By
    /// itself it turns it all the way up for as long as it is open.
    static let cockpitKeepsBrightness = "VentiCockpitKeepsBrightness"

    /// The folder new flights are brought in from by themselves
    /// (`WatchedFolder`): its bookmark, its name as the pilot knows it,
    /// and the files of it that were tried already.
    static let watchedFolder = "VentiWatchedFolder"
    static let watchedFolderName = "VentiWatchedFolderName"
    static let watchedSeen = "VentiWatchedSeen"

    /// What the core assumes of a glider until the pilot says otherwise.
    static let defaultTrimSpeed = 38.0
    static let defaultGlideRatio = 8.0
}
