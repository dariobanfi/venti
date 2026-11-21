package fly.venti.app.settings

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit

/**
 * The pilot's settings, kept on the phone.
 *
 * They are preferences of this device, not of the core, until accounts
 * exist. The names are passed to the core when a recording starts.
 */
interface Preferences {
    /** The pilot's name, written into recorded flights. */
    var pilot: String

    /** The glider, written into recorded flights. */
    var glider: String

    /** Whether the vario is silent. */
    var varioMuted: Boolean

    /** The Bluetooth address of the vario used last, to reconnect to it. */
    var rememberedVario: String?

    /** The model the assistant uses at Anthropic. */
    var assistantModel: String?

    /** Where the assistant runs: "anthropic", or "other" for the service at [assistantAddress]. */
    var assistantService: String?

    /** The address of the other service the assistant runs at, as the core made it. */
    var assistantAddress: String?

    /** That service's own name for the model. */
    var assistantOtherModel: String?


    /** The glider's speed with the brakes up, km/h. */
    var trimSpeed: Double

    /** The glider's glide ratio at that speed. */
    var glideRatio: Double

    /** Whether the pilot has switched off airspace from the server. */
    var airspaceWithoutServer: Boolean

    /** Whether the cockpit's map leaves out the FAI triangle areas. */
    var hidesFaiAreas: Boolean

    /**
     * Whom flights are shown to while they are under way: "link", "public",
     * or nothing for nobody.
     */
    var liveAudience: String

    /** What the pilot signs in to the DHV-XC with, and the glider as it is set up there. Not the password. */
    var dhvxcUser: String
    var dhvxcGlider: String

    /** Whether the DHV-XC is asked to publish a flight at once. Off until the pilot turns it on. */
    var dhvxcPublishes: Boolean

    /**
     * Whether the Today tab shows the take-offs around the pilot. Set when
     * the pilot taps "Use my position" there: until then it shows those
     * around the middle of the first region, also when the cockpit may
     * already use the position.
     */
    var todayNearPilot: Boolean

    /** The layers of the map tab the pilot has switched off with its chips. All are shown until then. */
    var mapHidesWind: Boolean
    var mapHidesAirspace: Boolean
    var mapHidesSites: Boolean
    var mapHidesPilots: Boolean
    var mapHidesCables: Boolean

    /** Whether the maps show the aerial view, photographs from above. One choice for every map of the app. */
    var mapShowsAerial: Boolean

    /**
     * The height the maps draw airspace up to, metres (the ceiling of a
     * step of the core's filter); `null` for all. Not the cockpit's map,
     * which draws all of it.
     */
    var mapAirspaceCeilingM: Double?

    /** The forecast map's layer by the hour, by the core's name of it ("WIND"); `null` for the thermals. */
    var mapForecastLayer: String?

    /** The height of its wind, metres above the sea; `null` for the ground. */
    var mapForecastHeightM: Int?

    /** Whether the maps show the thermal map of thermal.kk7.ch. Off until the pilot switches it on; one choice for every map. */
    var mapShowsThermals: Boolean

    /** Whether the map tab shows the forecast map, the kind of day square by square. Off until the pilot switches it on. */
    var mapShowsForecast: Boolean

    /** Whether the map tab shows the rain radar. Off until the pilot switches it on. */
    var mapShowsRadar: Boolean

    /** Whether the map tab shows the places where pilots take off and no site stands. Off until the pilot switches it on. */
    var mapShowsCandidates: Boolean

    /** What the pilot asks of the sites on the map, the places from pilots among it, as `SitesFilterWords.kept` writes it; `null` for nothing. */
    var mapSitesFilter: String?

    /** The folder Venti brings new flight files in from whenever it opens, as Android names it; empty for none. */
    var watchedFolder: String

    /** The files of that folder already looked at: where each lies, its size and when it changed. */
    var watchedSeen: Set<String>

    companion object {
        // What the core assumes of a glider until the pilot says otherwise.
        const val DEFAULT_TRIM_SPEED = 38.0
        const val DEFAULT_GLIDE_RATIO = 8.0
    }
}

/** The settings in the phone's own store. */
class StoredPreferences(context: Context) : Preferences {
    private val store: SharedPreferences = context.getSharedPreferences("venti", Context.MODE_PRIVATE)

    override var pilot: String
        get() = store.getString(PILOT, null).orEmpty()
        set(value) = store.edit { putString(PILOT, value) }

    override var glider: String
        get() = store.getString(GLIDER, null).orEmpty()
        set(value) = store.edit { putString(GLIDER, value) }

    override var varioMuted: Boolean
        get() = store.getBoolean(VARIO_MUTED, false)
        set(value) = store.edit { putBoolean(VARIO_MUTED, value) }

    override var rememberedVario: String?
        get() = store.getString(VARIO, null)
        set(value) = store.edit { putString(VARIO, value) }

    override var assistantModel: String?
        get() = store.getString(ASSISTANT_MODEL, null)
        set(value) = store.edit { putString(ASSISTANT_MODEL, value) }

    override var assistantService: String?
        get() = store.getString(ASSISTANT_SERVICE, null)
        set(value) = store.edit { putString(ASSISTANT_SERVICE, value) }

    override var assistantAddress: String?
        get() = store.getString(ASSISTANT_ADDRESS, null)
        set(value) = store.edit { putString(ASSISTANT_ADDRESS, value) }

    override var assistantOtherModel: String?
        get() = store.getString(ASSISTANT_OTHER_MODEL, null)
        set(value) = store.edit { putString(ASSISTANT_OTHER_MODEL, value) }


    override var trimSpeed: Double
        get() = store.getFloat(TRIM_SPEED, Preferences.DEFAULT_TRIM_SPEED.toFloat()).toDouble()
        set(value) = store.edit { putFloat(TRIM_SPEED, value.toFloat()) }

    override var glideRatio: Double
        get() = store.getFloat(GLIDE_RATIO, Preferences.DEFAULT_GLIDE_RATIO.toFloat()).toDouble()
        set(value) = store.edit { putFloat(GLIDE_RATIO, value.toFloat()) }

    override var airspaceWithoutServer: Boolean
        get() = store.getBoolean(AIRSPACE_WITHOUT_SERVER, false)
        set(value) = store.edit { putBoolean(AIRSPACE_WITHOUT_SERVER, value) }

    override var hidesFaiAreas: Boolean
        get() = store.getBoolean(HIDES_FAI_AREAS, false)
        set(value) = store.edit { putBoolean(HIDES_FAI_AREAS, value) }

    override var liveAudience: String
        get() = store.getString(LIVE_AUDIENCE, null).orEmpty()
        set(value) = store.edit { putString(LIVE_AUDIENCE, value) }

    override var dhvxcUser: String
        get() = store.getString(DHVXC_USER, null).orEmpty()
        set(value) = store.edit { putString(DHVXC_USER, value) }

    override var dhvxcGlider: String
        get() = store.getString(DHVXC_GLIDER, null).orEmpty()
        set(value) = store.edit { putString(DHVXC_GLIDER, value) }

    override var dhvxcPublishes: Boolean
        get() = store.getBoolean(DHVXC_PUBLISHES, false)
        set(value) = store.edit { putBoolean(DHVXC_PUBLISHES, value) }

    override var todayNearPilot: Boolean
        get() = store.getBoolean(TODAY_NEAR_PILOT, false)
        set(value) = store.edit { putBoolean(TODAY_NEAR_PILOT, value) }

    override var mapHidesWind: Boolean
        get() = store.getBoolean("VentiMapHidesWind", false)
        set(value) = store.edit { putBoolean("VentiMapHidesWind", value) }

    override var mapHidesAirspace: Boolean
        get() = store.getBoolean("VentiMapHidesAirspace", false)
        set(value) = store.edit { putBoolean("VentiMapHidesAirspace", value) }

    override var mapHidesSites: Boolean
        get() = store.getBoolean("VentiMapHidesSites", false)
        set(value) = store.edit { putBoolean("VentiMapHidesSites", value) }

    override var mapHidesPilots: Boolean
        get() = store.getBoolean("VentiMapHidesPilots", false)
        set(value) = store.edit { putBoolean("VentiMapHidesPilots", value) }

    override var mapHidesCables: Boolean
        get() = store.getBoolean("VentiMapHidesCables", false)
        set(value) = store.edit { putBoolean("VentiMapHidesCables", value) }

    override var mapShowsAerial: Boolean
        get() = store.getBoolean("VentiMapShowsAerial", false)
        set(value) = store.edit { putBoolean("VentiMapShowsAerial", value) }

    override var mapForecastLayer: String?
        get() = store.getString("VentiMapForecastLayer", null)
        set(value) = store.edit { putString("VentiMapForecastLayer", value) }

    override var mapForecastHeightM: Int?
        get() = if (store.contains("VentiMapForecastHeight")) store.getInt("VentiMapForecastHeight", 0) else null
        set(value) = store.edit {
            if (value == null) remove("VentiMapForecastHeight") else putInt("VentiMapForecastHeight", value)
        }

    override var mapAirspaceCeilingM: Double?
        get() = if (store.contains("VentiMapAirspaceCeiling")) store.getFloat("VentiMapAirspaceCeiling", 0f).toDouble() else null
        set(value) = store.edit {
            if (value == null) remove("VentiMapAirspaceCeiling") else putFloat("VentiMapAirspaceCeiling", value.toFloat())
        }

    override var mapShowsThermals: Boolean
        get() = store.getBoolean("VentiMapShowsThermals", false)
        set(value) = store.edit { putBoolean("VentiMapShowsThermals", value) }

    override var mapShowsForecast: Boolean
        get() = store.getBoolean("VentiMapShowsForecast", false)
        set(value) = store.edit { putBoolean("VentiMapShowsForecast", value) }

    override var mapShowsCandidates: Boolean
        get() = store.getBoolean("VentiMapShowsCandidates", false)
        set(value) = store.edit { putBoolean("VentiMapShowsCandidates", value) }

    override var mapShowsRadar: Boolean
        get() = store.getBoolean("VentiMapShowsRadar", false)
        set(value) = store.edit { putBoolean("VentiMapShowsRadar", value) }

    override var mapSitesFilter: String?
        get() = store.getString("VentiMapSitesFilter", null)
        set(value) = store.edit { putString("VentiMapSitesFilter", value) }

    override var watchedFolder: String
        get() = store.getString("VentiWatchedFolder", null).orEmpty()
        set(value) = store.edit { putString("VentiWatchedFolder", value) }

    override var watchedSeen: Set<String>
        get() = store.getStringSet("VentiWatchedSeen", null).orEmpty().toSet()
        set(value) = store.edit { putStringSet("VentiWatchedSeen", value) }

    // The same names as on the iPhone.
    private companion object {
        const val PILOT = "VentiPilotName"
        const val GLIDER = "VentiGliderName"
        const val VARIO_MUTED = "VentiVarioMuted"
        const val VARIO = "VentiVarioIdentifier"
        const val ASSISTANT_MODEL = "VentiAssistantModel"
        const val ASSISTANT_SERVICE = "VentiAssistantService"
        const val ASSISTANT_ADDRESS = "VentiAssistantAddress"
        const val ASSISTANT_OTHER_MODEL = "VentiAssistantOtherModel"
        const val TRIM_SPEED = "VentiGliderTrimSpeed"
        const val GLIDE_RATIO = "VentiGliderGlideRatio"
        const val AIRSPACE_WITHOUT_SERVER = "VentiAirspaceWithoutServer"
        const val HIDES_FAI_AREAS = "VentiCockpitHidesFaiAreas"
        const val LIVE_AUDIENCE = "VentiLiveAudience"
        const val DHVXC_USER = "dhvxc-user"
        const val DHVXC_GLIDER = "dhvxc-glider"
        const val DHVXC_PUBLISHES = "dhvxc-publishes"
        const val TODAY_NEAR_PILOT = "today-near-pilot"
    }
}

/** Settings that last as long as the app runs. Tests use them. */
class MemoryPreferences : Preferences {
    override var pilot = ""
    override var glider = ""
    override var varioMuted = false
    override var rememberedVario: String? = null
    override var assistantModel: String? = null
    override var assistantService: String? = null
    override var assistantAddress: String? = null
    override var assistantOtherModel: String? = null
    override var trimSpeed = Preferences.DEFAULT_TRIM_SPEED
    override var glideRatio = Preferences.DEFAULT_GLIDE_RATIO
    override var airspaceWithoutServer = false
    override var hidesFaiAreas = false
    override var liveAudience = ""
    override var dhvxcUser = ""
    override var dhvxcGlider = ""
    override var dhvxcPublishes = false
    override var todayNearPilot = false
    override var mapHidesWind = false
    override var mapHidesAirspace = false
    override var mapHidesSites = false
    override var mapHidesPilots = false
    override var mapHidesCables = false
    override var mapShowsAerial = false
    override var mapAirspaceCeilingM: Double? = null
    override var mapForecastLayer: String? = null
    override var mapForecastHeightM: Int? = null
    override var mapShowsThermals = false
    override var mapShowsForecast = false
    override var mapShowsRadar = false
    override var mapShowsCandidates = false
    override var mapSitesFilter: String? = null
    override var watchedFolder = ""
    override var watchedSeen: Set<String> = emptySet()
}
