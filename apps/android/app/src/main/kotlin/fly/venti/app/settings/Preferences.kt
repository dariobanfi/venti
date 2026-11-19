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

    // The same names as on the iPhone.
    private companion object {
        const val PILOT = "VentiPilotName"
        const val GLIDER = "VentiGliderName"
        const val VARIO_MUTED = "VentiVarioMuted"
        const val VARIO = "VentiVarioIdentifier"
        const val ASSISTANT_MODEL = "VentiAssistantModel"
    }
}

/** Settings that last as long as the app runs. Tests use them. */
class MemoryPreferences : Preferences {
    override var pilot = ""
    override var glider = ""
    override var varioMuted = false
    override var rememberedVario: String? = null
    override var assistantModel: String? = null
}
