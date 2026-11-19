package fly.venti.app

import android.app.Application
import android.content.Context
import android.content.Intent
import android.os.Build
import android.text.format.DateFormat
import fly.venti.app.account.AccountModel
import fly.venti.app.account.KeystoreSessionStore
import fly.venti.app.account.MemorySessionStore
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.airspace.AirspaceVoice
import fly.venti.app.assistant.AssistantChat
import fly.venti.app.assistant.AssistantHttp
import fly.venti.app.assistant.EncryptedKeyStore
import fly.venti.app.assistant.FixedKey
import fly.venti.app.assistant.ScriptedProvider
import fly.venti.app.design.Format
import fly.venti.app.fly.FlySession
import fly.venti.app.fly.PhoneSensors
import fly.venti.app.fly.SilentAnnouncer
import fly.venti.app.fly.SilentTone
import fly.venti.app.fly.VarioLink
import fly.venti.app.fly.VarioSound
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.settings.MemoryPreferences
import fly.venti.app.settings.Preferences
import fly.venti.app.settings.StoredPreferences
import fly.venti.app.sites.SitesModel
import fly.venti.app.task.TaskModel
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.wind.WindModel
import fly.venti.app.wind.WindServer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.launch
import java.io.File
import java.util.UUID

/**
 * What the app holds for as long as it runs: the logbook, the flying
 * session, the vario link, the conversation with the assistant, the wind
 * and the flying sites on the map, and the pilot's account on a server.
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
        val chat: AssistantChat,
        val wind: WindModel,
        val waypoints: WaypointsModel,
        val airspace: AirspaceModel,
        val sites: SitesModel,
        val account: AccountModel,
        val tasks: TaskModel,
        val preferences: Preferences,
        val files: AppFiles,
        /** Where work runs that must outlast the screen that started it: an import, a request. */
        val scope: CoroutineScope,
    ) : Session

    /** The data folder could not be opened; the text says why. */
    class Failed(val message: String) : Session

    companion object {
        /**
         * What this phone is called in an account's list of devices: its
         * maker and model, "Google Pixel 8". Not the name the pilot gave
         * it, which is often the pilot's own.
         */
        fun deviceName(maker: String = Build.MANUFACTURER.orEmpty(), model: String = Build.MODEL.orEmpty()): String {
            val made = maker.replaceFirstChar { it.uppercase() }
            return if (model.startsWith(made, ignoreCase = true)) model else "$made $model".trim()
        }

        /**
         * Opens the logbook in `folder` and gets the cockpit and the
         * assistant ready. For a test the pilot's settings are left alone
         * and no sound is made; `scriptedAssistant` answers the assistant's
         * requests from a script, without a key or a network; and the wind
         * comes from `server`, or from nowhere.
         */
        fun start(
            context: Context,
            folder: File,
            isUnderTest: Boolean = false,
            scriptedAssistant: Boolean = false,
            server: String? = null,
            sampleWaypoints: Boolean = false,
            sampleAirspace: Boolean = false,
            sampleTask: Boolean = false,
        ): Session {
            val app = context.applicationContext
            val files = AssetFiles(app)
            val logbook = try {
                LogbookModel.open(folder, files)
            } catch (error: Exception) {
                return Failed(error.pilotMessage)
            }
            val preferences = if (isUnderTest) MemoryPreferences() else StoredPreferences(app)
            val vario = VarioLink(app, preferences)
            val scope = MainScope()
            val fly = FlySession(
                dataFolder = folder,
                preferences = preferences,
                sensors = PhoneSensors(app, vario),
                sound = if (isUnderTest) SilentTone() else VarioSound(),
                // Tests have nobody listening.
                voice = if (isUnderTest) SilentAnnouncer() else AirspaceVoice(app),
                scope = scope,
                // A UI test's screen must come to rest for the test to look at it.
                refreshMilliseconds = if (isUnderTest) {
                    FlySession.SLOW_REFRESH_MILLISECONDS
                } else {
                    FlySession.REFRESH_MILLISECONDS
                },
            )

            // A flight that was being recorded when the app last stopped.
            // Listed now, before a new recording can exist, and brought
            // into the logbook in the background.
            val interrupted = logbook.interruptedRecordings()
            if (interrupted.isNotEmpty()) {
                scope.launch { logbook.adopt(interrupted) }
            }
            val wind = WindModel(
                remembered = preferences,
                // A test never asks the server the pilot has set.
                address = if (isUnderTest) server.orEmpty() else preferences.server,
                scope = scope,
            )
            val chat = AssistantChat(
                makeAssistant = logbook::makeAssistant,
                keys = when {
                    scriptedAssistant -> FixedKey("scripted")
                    // A test never reads or replaces the pilot's own key.
                    isUnderTest -> FixedKey(null)
                    else -> EncryptedKeyStore(app)
                },
                preferences = preferences,
                scope = scope,
                transport = if (scriptedAssistant) ScriptedProvider::answer else AssistantHttp::send,
                // The assistant looks up live wind on the same server as
                // the map, and measures ages against the same clock.
                server = { (wind.server as? WindServer.Usable)?.url },
                now = { wind.now },
            )
            val waypoints = logbook.makeWaypoints()
            if (sampleWaypoints) {
                waypoints.importText(LaunchOptions.SAMPLE_WAYPOINT_FILE)
            }
            val airspace = logbook.makeAirspace(remembered = preferences, scope = scope)
            // Airspace comes from the same server as the wind.
            airspace.server = { (wind.server as? WindServer.Usable)?.url }
            if (sampleAirspace) {
                airspace.importText("sample-zones-not-real.txt", LaunchOptions.SAMPLE_AIRSPACE_FILE)
            }
            // The cockpit warns of the pilot's files and of what is kept
            // from the server, also when either changes during a flight.
            fly.airspace = { airspace.set }
            // Flying sites come from that server too.
            val sites = SitesModel(scope)
            sites.server = { (wind.server as? WindServer.Usable)?.url }
            // The pilot's account on that server, if there is one. A test
            // starts signed out, and its token stays out of the keystore.
            val account = logbook.makeAccount(
                session = if (isUnderTest) MemorySessionStore() else KeystoreSessionStore(app),
                device = deviceName(),
            )
            // A flight shared while it is under way goes to the pilot's
            // server, with the account's token.
            fly.server = { account.account?.server.takeIf { account.isSignedIn } }
            fly.carry = account::carry
            // The task the pilot keeps, which the cockpit flies.
            val tasks = logbook.makeTasks()
            if (sampleTask) {
                tasks.keep(LaunchOptions.SAMPLE_TASK_FILE)
            }
            fly.taskFile = { tasks.file }
            return Ready(
                logbook, fly, vario, chat, wind, waypoints, airspace, sites, account, tasks, preferences, files, scope,
            )
        }
    }
}

class VentiApplication : Application() {
    /** The session of this run of the app. Made when the first screen asks for it. */
    val session: Session by lazy {
        Session.start(this, folder = filesDir.resolve("venti"))
    }

    /** The session of a UI test, if one is running: an empty logbook in a throwaway folder. */
    var testSession: Session? = null
        private set

    override fun onCreate() {
        super.onCreate()
        Format.uses24HourClock = DateFormat.is24HourFormat(this)
    }

    /** Starts over with an empty logbook, for a UI test. */
    fun startTestSession(options: LaunchOptions): Session {
        val folder = cacheDir.resolve("venti-ui-test-${UUID.randomUUID()}")
        return Session.start(
            this,
            folder,
            isUnderTest = true,
            scriptedAssistant = options.usesScriptedAssistant,
            server = options.server,
            sampleWaypoints = options.usesSampleWaypoints,
            sampleAirspace = options.usesSampleAirspace,
            sampleTask = options.usesSampleTask,
        ).also { testSession = it }
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
    /**
     * Answer the assistant's requests from a script instead of a provider,
     * so its screen can be tested without a key or a network.
     */
    val usesScriptedAssistant: Boolean = false,
    /** Dark or light appearance whatever the phone is set to, for screenshots. `null` follows the phone. */
    val forcedDark: Boolean? = null,
    /** The Venti server a test's map takes its wind from. `null` for none. */
    val server: String? = null,
    /**
     * Start with a few made-up waypoints around the sample flight, so the
     * cockpit's navigation can be tested without picking a file.
     */
    val usesSampleWaypoints: Boolean = false,
    /**
     * Start with a made-up control zone beside the valley the sample flight
     * crosses, so the cockpit's airspace warnings can be tested.
     */
    val usesSampleAirspace: Boolean = false,
    /**
     * Start with a made-up task along the sample flight, so the cockpit's
     * task can be tested without picking a file.
     */
    val usesSampleTask: Boolean = false,
) {
    companion object {
        const val UI_TESTING = "ui-testing"
        const val BUNDLED_MAP = "bundled-map"
        const val SCRIPTED_ASSISTANT = "scripted-assistant"
        const val DARK = "dark"
        const val SERVER = "server"
        const val SAMPLE_WAYPOINTS = "sample-waypoints"
        const val SAMPLE_AIRSPACE = "sample-airspace"
        const val SAMPLE_TASK = "sample-task"

        /**
         * The made-up task, as a file in the XCTSK format: out of a start
         * cylinder around the sample flight's take-off two minutes into it,
         * to a turnpoint the flight passes, and to a goal it reaches. Its
         * names say that it is not real.
         */
        val SAMPLE_TASK_FILE = """
            {"taskType":"CLASSIC","version":1,"earthModel":"WGS84","turnpoints":[
            {"type":"TAKEOFF","radius":400,"waypoint":{"name":"Sample launch","lat":47.6642,"lon":11.7967,"altSmoothed":1620}},
            {"type":"SSS","radius":1000,"waypoint":{"name":"Sample start","lat":47.6642,"lon":11.7967,"altSmoothed":1620}},
            {"radius":400,"waypoint":{"name":"Sample turnpoint west","lat":47.6894,"lon":11.7506,"altSmoothed":800}},
            {"radius":400,"waypoint":{"name":"Sample goal","lat":47.6638,"lon":11.7432,"altSmoothed":760}}],
            "takeoff":{"timeOpen":"09:00:00Z","timeClose":"11:00:00Z"},
            "sss":{"type":"RACE","direction":"EXIT","timeGates":["10:32:00Z"]},
            "goal":{"type":"CYLINDER","deadline":"12:00:00Z"}}
        """.trimIndent()

        /**
         * The made-up airspace, in OpenAir format. Its names say that it is
         * not real. It lies where the sample flight, once it has come near,
         * stays near until it lands: a test that looks at a warning then
         * finds one however slowly the device replays the flight.
         */
        val SAMPLE_AIRSPACE_FILE = """
            * Made-up airspace for testing Venti. Not real.
            AC D
            AY CTR
            AN SAMPLE ZONE (NOT REAL)
            AL GND
            AH 9000ft AMSL
            V X=47:41:10 N 011:43:40 E
            DC 1
        """.trimIndent()

        /** The made-up waypoints. Their names say that they are not real places to land. */
        val SAMPLE_WAYPOINT_FILE = """
            name,code,country,lat,lon,elev,style
            "Sample landing field",SLF,DE,4741.000N,01145.300E,735m,21
            "Sample turnpoint",STP,DE,4737.200N,01152.000E,1700m,1
            "Sample summit far away",SSF,DE,4725.260N,01059.090E,2962m,7
        """.trimIndent()

        fun from(intent: Intent?): LaunchOptions {
            if (!BuildConfig.DEBUG || intent == null) return LaunchOptions()
            return LaunchOptions(
                isUiTesting = intent.getBooleanExtra(UI_TESTING, false),
                usesBundledMap = intent.getBooleanExtra(BUNDLED_MAP, false),
                usesScriptedAssistant = intent.getBooleanExtra(SCRIPTED_ASSISTANT, false),
                forcedDark = if (intent.hasExtra(DARK)) intent.getBooleanExtra(DARK, false) else null,
                server = intent.getStringExtra(SERVER),
                usesSampleWaypoints = intent.getBooleanExtra(SAMPLE_WAYPOINTS, false),
                usesSampleAirspace = intent.getBooleanExtra(SAMPLE_AIRSPACE, false),
                usesSampleTask = intent.getBooleanExtra(SAMPLE_TASK, false),
            )
        }
    }
}
