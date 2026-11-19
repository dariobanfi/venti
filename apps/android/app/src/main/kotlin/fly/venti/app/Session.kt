package fly.venti.app

import android.app.Application
import android.content.Context
import android.content.Intent
import android.text.format.DateFormat
import fly.venti.app.design.Format
import fly.venti.app.fly.FlySession
import fly.venti.app.fly.PhoneSensors
import fly.venti.app.fly.SilentTone
import fly.venti.app.fly.VarioLink
import fly.venti.app.fly.VarioSound
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.settings.MemoryPreferences
import fly.venti.app.settings.Preferences
import fly.venti.app.settings.StoredPreferences
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.launch
import java.io.File
import java.util.UUID

/**
 * What the app holds for as long as it runs: the logbook, the flying
 * session and the vario link.
 *
 * It belongs to the app's process, not to a screen. Android rebuilds the
 * screen whenever it likes, and a flight being recorded has to carry on
 * through that.
 */
sealed interface Session {
    class Ready(
        val logbook: LogbookModel,
        val fly: FlySession,
        val vario: VarioLink,
        val preferences: Preferences,
        val files: AppFiles,
    ) : Session

    /** The data folder could not be opened; the text says why. */
    class Failed(val message: String) : Session

    companion object {
        /** Opens the logbook in `folder` and gets the cockpit ready. */
        fun start(context: Context, folder: File, isUnderTest: Boolean): Session {
            val app = context.applicationContext
            val files = AssetFiles(app)
            val logbook = try {
                LogbookModel.open(folder, files)
            } catch (error: Exception) {
                return Failed(error.pilotMessage)
            }
            // A test starts from nothing and must leave the pilot's own
            // settings alone; it also has nobody listening.
            val preferences = if (isUnderTest) MemoryPreferences() else StoredPreferences(app)
            val vario = VarioLink(app, preferences)
            val scope = MainScope()
            val fly = FlySession(
                dataFolder = folder,
                preferences = preferences,
                sensors = PhoneSensors(app, vario),
                sound = if (isUnderTest) SilentTone() else VarioSound(),
                scope = scope,
            )

            // A flight that was being recorded when the app last stopped.
            // Listed now, before a new recording can exist, and brought
            // into the logbook in the background.
            val interrupted = logbook.interruptedRecordings()
            if (interrupted.isNotEmpty()) {
                scope.launch { logbook.adopt(interrupted) }
            }
            return Ready(logbook, fly, vario, preferences, files)
        }
    }
}

class VentiApplication : Application() {
    /** The session of this run of the app. Made when the first screen asks for it. */
    val session: Session by lazy {
        Session.start(this, folder = filesDir.resolve("venti"), isUnderTest = false)
    }

    /** The session of a UI test, if one is running: an empty logbook in a throwaway folder. */
    var testSession: Session? = null
        private set

    override fun onCreate() {
        super.onCreate()
        Format.uses24HourClock = DateFormat.is24HourFormat(this)
    }

    /** Starts over with an empty logbook, for a UI test. */
    fun startTestSession(): Session {
        val folder = cacheDir.resolve("venti-ui-test-${UUID.randomUUID()}")
        return Session.start(this, folder, isUnderTest = true).also { testSession = it }
    }
}

/**
 * Switches set by whoever starts the app, as extras of the launching
 * intent. The UI tests use them; nothing in the app offers them to the
 * pilot, and only a debug build listens to them.
 */
data class LaunchOptions(
    /** Start with an empty logbook in a throwaway folder. */
    val isUiTesting: Boolean = false,
    /**
     * Draw the map from the region shipped with the app instead of fetching
     * tiles, so a screenshot does not depend on the network.
     */
    val usesBundledMap: Boolean = false,
    /** Dark or light appearance whatever the phone is set to, for screenshots. `null` follows the phone. */
    val forcedDark: Boolean? = null,
) {
    companion object {
        const val UI_TESTING = "ui-testing"
        const val BUNDLED_MAP = "bundled-map"
        const val DARK = "dark"

        fun from(intent: Intent?): LaunchOptions {
            if (!BuildConfig.DEBUG || intent == null) return LaunchOptions()
            return LaunchOptions(
                isUiTesting = intent.getBooleanExtra(UI_TESTING, false),
                usesBundledMap = intent.getBooleanExtra(BUNDLED_MAP, false),
                forcedDark = if (intent.hasExtra(DARK)) intent.getBooleanExtra(DARK, false) else null,
            )
        }
    }
}
