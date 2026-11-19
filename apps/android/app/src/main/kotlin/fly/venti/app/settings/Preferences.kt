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

    /** The model the assistant uses. */
    var assistantModel: String?

    /** The address of the Venti server that live wind comes from. */
    var server: String

    /** The glider's speed with the brakes up, km/h. */
    var trimSpeed: Double

    /** The glider's glide ratio at that speed. */
    var glideRatio: Double

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

    override var server: String
        get() = store.getString(SERVER, null).orEmpty()
        set(value) = store.edit { putString(SERVER, value) }

    override var trimSpeed: Double
        get() = store.getFloat(TRIM_SPEED, Preferences.DEFAULT_TRIM_SPEED.toFloat()).toDouble()
        set(value) = store.edit { putFloat(TRIM_SPEED, value.toFloat()) }

    override var glideRatio: Double
        get() = store.getFloat(GLIDE_RATIO, Preferences.DEFAULT_GLIDE_RATIO.toFloat()).toDouble()
        set(value) = store.edit { putFloat(GLIDE_RATIO, value.toFloat()) }

    // The same names as on the iPhone.
    private companion object {
        const val PILOT = "VentiPilotName"
        const val GLIDER = "VentiGliderName"
        const val VARIO_MUTED = "VentiVarioMuted"
        const val VARIO = "VentiVarioIdentifier"
        const val ASSISTANT_MODEL = "VentiAssistantModel"
        const val SERVER = "VentiServerAddress"
        const val TRIM_SPEED = "VentiGliderTrimSpeed"
        const val GLIDE_RATIO = "VentiGliderGlideRatio"
    }
}

/** Settings that last as long as the app runs. Tests use them. */
class MemoryPreferences : Preferences {
    override var pilot = ""
    override var glider = ""
    override var varioMuted = false
    override var rememberedVario: String? = null
    override var assistantModel: String? = null
    override var server = ""
    override var trimSpeed = Preferences.DEFAULT_TRIM_SPEED
    override var glideRatio = Preferences.DEFAULT_GLIDE_RATIO
}
