package fly.venti.app

import kotlinx.coroutines.CancellationException
import fly.venti.app.maps.Connectivity
import fly.venti.app.maps.OfflineMapsModel
import fly.venti.app.flight.KeystoreUploadPasswordStore
import fly.venti.app.flight.MemoryUploadPasswordStore
import fly.venti.app.flight.Leagues
import android.app.Application
import android.content.Context
import android.content.Intent
import android.os.Build
import android.text.format.DateFormat
import fly.venti.app.account.AccountModel
import fly.venti.app.account.CredentialManagerGoogle
import fly.venti.app.account.DevelopmentGoogle
import fly.venti.app.alerts.AlertNotifications
import fly.venti.app.alerts.AlertsCheck
import fly.venti.app.alerts.AlertsModel
import fly.venti.app.account.KeystoreSessionStore
import fly.venti.app.account.MemorySessionStore
import fly.venti.app.airspace.AirspaceModel
import fly.venti.app.airspace.AirspaceVoice
import fly.venti.app.assistant.AssistantChat
import fly.venti.app.assistant.AssistantHttp
import fly.venti.app.assistant.EncryptedKeyStore
import fly.venti.app.assistant.FixedKey
import fly.venti.app.assistant.ScriptedProvider
import fly.venti.app.community.CommunityModel
import fly.venti.app.community.SocialModel
import fly.venti.app.community.OpenPlansModel
import fly.venti.app.community.PilotPictures
import fly.venti.app.design.Format
import fly.venti.app.fly.FlySession
import fly.venti.app.fly.PhoneSensors
import fly.venti.app.fly.SilentAnnouncer
import fly.venti.app.fly.SilentTone
import fly.venti.app.fly.VarioLink
import fly.venti.app.fly.VarioSound
import fly.venti.app.groups.GroupsModel
import fly.venti.app.live.LiveModel
import fly.venti.app.live.AirModel
import fly.venti.app.logbook.LogbookModel
import fly.venti.app.map.MapSetup
import fly.venti.app.settings.GliderWords
import fly.venti.app.settings.MemoryPreferences
import fly.venti.app.settings.Preferences
import fly.venti.app.settings.StoredPreferences
import fly.venti.app.sites.KeptSitesModel
import fly.venti.app.sites.SiteEditModel
import fly.venti.app.sites.SitesModel
import fly.venti.app.forecast.ThermalMapModel
import fly.venti.app.radar.RadarModel
import fly.venti.app.today.TodayModel
import fly.venti.app.widget.TakeoffWidgets
import fly.venti.app.logbook.AndroidFolders
import fly.venti.app.logbook.WatchedFolder
import fly.venti.app.task.TaskModel
import fly.venti.app.terrain.TerrainModel
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.wind.WindModel
import fly.venti.app.wind.WindServer
import fly.venti.app.cables.CablesModel
import fly.venti.core.todayArea
import fly.venti.core.WaypointKind
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.launch
import java.io.File
import java.util.UUID

/**
 * What the app holds for as long as it runs: the logbook, the flying
 * session, the vario link, the conversation with the assistant, the wind,
 * the flying sites and the other pilots on the map, and the pilot's account
 * on a server.
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
        val live: LiveModel,
        /** Where pilots climb now, where thermals have been, and who is in the air at a site. */
        val air: AirModel,
        val community: CommunityModel,
        /** Pilots among each other: following, the feed, cheers, comments and what the pilot was told. */
        val social: SocialModel,
        val account: AccountModel,
        val groups: GroupsModel,
        /** The open plans near a place, for everyone flying from there. */
        val openPlans: OpenPlansModel,
        /** The pilot's alarms and what they fired. */
        val alerts: AlertsModel,
        /** Adding and correcting the sites that pilots make. */
        val siteEdit: SiteEditModel,
        val tasks: TaskModel,
        /** Sending a flight to a league, from this phone. */
        val leagues: Leagues,
        /** Maps for flying without a network. */
        val maps: OfflineMapsModel,
        /** The ground: under the glider in the cockpit, and under a flight for its profile. */
        val terrain: TerrainModel,
        /** The take-offs around, and what the forecast says of each. */
        val today: TodayModel,
        /** The pilot's own take-offs, kept on this phone: Today lists them first. */
        val keptSites: KeptSitesModel,
        /** The kind of day, square by square, for the map's "Forecast" layer. */
        val thermalMap: ThermalMapModel,
        /** The rain radar, for the map's "Radar" layer. */
        val radar: RadarModel,
        /** The cables of OpenStreetMap, for the maps and the cockpit's warning. */
        val cables: CablesModel,
        /** The folder new flight files are brought in from whenever the app opens: XCTrack's, as a rule. */
        val watched: WatchedFolder,
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
            slowGlider: Boolean = false,
            /** A UI test's watched folder that Android no longer lets the app into. */
            lostWatchedFolder: Boolean = false,
            /** A UI test's Google account, for the development server's made-up Google; `null` for Google itself. */
            googleTestAccount: String? = null,
            googleTestPicture: Boolean = false,
        ): Session {
            val app = context.applicationContext
            val files = AssetFiles(app)
            val logbook = try {
                LogbookModel.open(folder, files)
            } catch (error: Exception) {
                return Failed(error.pilotMessage)
            }
            val preferences = if (isUnderTest) MemoryPreferences() else StoredPreferences(app)
            if (slowGlider) {
                preferences.trimSpeed = LaunchOptions.SLOW_GLIDER_TRIM_SPEED
            }
            if (lostWatchedFolder) {
                preferences.watchedFolder = LaunchOptions.LOST_FOLDER
                preferences.watchedFolderName = LaunchOptions.LOST_FOLDER_NAME
            }
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
                clock = if (isUnderTest) {
                    { System.currentTimeMillis() / 1000.0 + TestClock.aheadSeconds }
                } else {
                    { System.currentTimeMillis() / 1000.0 }
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
                // The one Venti, at the address the app was built with. A
                // test names its own, or none.
                address = if (isUnderTest) server.orEmpty() else AppServer.address,
                scope = scope,
            )
            val chat = AssistantChat(
                makeAssistant = logbook::makeAssistant,
                keys = when {
                    scriptedAssistant -> FixedKey("scripted")
                    // A test never reads or replaces the pilot's own key.
                    isUnderTest -> FixedKey(null)
                    else -> EncryptedKeyStore.anthropic(app)
                },
                // The key for another service, kept apart from Anthropic's.
                // A test types a made-up one.
                otherKeys = if (isUnderTest) FixedKey(null) else EncryptedKeyStore.otherService(app),
                makeAssistantAt = logbook::makeAssistantAt,
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
            // Cables come from the same server, around where the map looks
            // and the flight is; the cockpit warns of them.
            val cables = CablesModel(scope = scope)
            cables.server = { (wind.server as? WindServer.Usable)?.url }
            fly.cables = { cables.set }
            // The other networks' aircraft around the glider, asked of the same server.
            fly.fetch = { path ->
                (wind.server as? WindServer.Usable)?.url?.let { address ->
                    try {
                        Http.get(address + path)
                    } catch (error: CancellationException) {
                        throw error
                    } catch (_: Exception) {
                        null
                    }
                }
            }
            // What the map style asks of Venti itself, the photographs of
            // Austria, is asked of the same server.
            MapSetup.server = { (wind.server as? WindServer.Usable)?.url }
            // Flying sites come from that server too.
            val sites = SitesModel(scope)
            sites.server = { (wind.server as? WindServer.Usable)?.url }
            // The cockpit's wind on the ground, from the same stations.
            fly.windOnTheGround = { place, landing -> wind.stationsAround(place, landing) }
            // And the forecast of the glider's place, for the thermal view's wind before the flight has one.
            fly.forecastAt = { place -> wind.forecast(place) }
            // And the forecasts for the take-offs of the Today tab, whose
            // hours are gone by the server's clock.
            val keptSites = logbook.makeKeptSites()
            // The take-offs on the cockpit's map: the pilot's own and the
            // sites the app holds around the glider, asked for again once
            // the glider has left the area of the answer held.
            fly.takeoffsAround = { place ->
                val address = (wind.server as? WindServer.Usable)?.url
                if (sites.heldAround(place) == null && address != null) {
                    runCatching { todayArea(place.lat, place.lon) }.getOrNull()?.let { area -> sites.load(area, address) }
                }
                keptSites.cockpitTakeoffs(place, sites.heldAround(place))
            }
            // The landings for the cockpit's standing reading of the nearest
            // one within glide: the core picks them, from those sites, the
            // pilot's own and the pilot's landing waypoints.
            fly.landingsAround = { place -> keptSites.cockpitLandings(place, sites.heldAround(place)) }
            val today = TodayModel(keptSites)
            today.server = { (wind.server as? WindServer.Usable)?.url }
            today.clock = { wind.serverTimeOrZero() }
            // The kind of day square by square, for the map, from the same
            // server and by the same clock.
            val thermalMap = ThermalMapModel()
            thermalMap.server = { (wind.server as? WindServer.Usable)?.url }
            thermalMap.clock = { wind.serverTimeOrZero() }
            // The rain radar too: its frames' ages are counted against Venti's clock.
            val radar = RadarModel()
            radar.server = { (wind.server as? WindServer.Usable)?.url }
            radar.clock = { wind.serverTimeOrZero() }
            // So do the other pilots in the air.
            val live = LiveModel(scope)
            live.server = { (wind.server as? WindServer.Usable)?.url }
            // The cockpit's map shows them too, around the glider.
            fly.others = { place -> live.around(place) }
            // What only a server that sees many gliders can say: two layers
            // of the map and a line of a site's page, and in the cockpit
            // the climbs around the glider once the pilot switches them on.
            val air = AirModel()
            air.server = { (wind.server as? WindServer.Usable)?.url }
            fly.wantsClimbs = { preferences.cockpitShowsClimbs }
            // And everyone's public flights, on the Logbook tab.
            val community = CommunityModel()
            // The ranking's years by Venti's clock, or the core's all years while it is unread.
            community.clock = { wind.serverTimeOrZero() }
            community.server = { (wind.server as? WindServer.Usable)?.url }
            // The pilots' pictures come from the same Venti.
            PilotPictures.server = { (wind.server as? WindServer.Usable)?.url }
            // The pilot's account on that server, if there is one. A test
            // starts signed out, and its token stays out of the keystore.
            val account = logbook.makeAccount(
                session = if (isUnderTest) MemorySessionStore() else KeystoreSessionStore(app),
                device = deviceName(),
            )
            // "Continue with Google" asks Google on the phone, for Venti's
            // client ID; a UI test asks the development server instead.
            account.google = googleTestAccount?.let { DevelopmentGoogle(it, name = LaunchOptions.GOOGLE_TEST_NAME, withPicture = googleTestPicture) }
                ?: CredentialManagerGoogle(BuildConfig.GOOGLE_CLIENT_ID)
            // A flight sent to the server carries the class the app's glider setting has.
            account.gliderClass = { GliderWords.kind(preferences.gliderClass) }
            // Pilots among each other, asked of the same Venti: with the
            // account's token where a request needs one.
            // Its ages are counted by Venti's clock alone: 0 until that
            // has been read, and the core then says the day.
            val social = SocialModel(account, now = { wind.serverTimeOrZero() })
            social.server = { (wind.server as? WindServer.Usable)?.url }
            // The pilot's own take-offs are on the account too, once signed in.
            logbook.follow(account, keptSites, scope)
            // The home-screen widgets draw what Today's model reads of the
            // pilot's own take-offs. A test's take-offs are not the pilot's.
            if (!isUnderTest) TakeoffWidgets.follow(app, today, keptSites, scope)
            // The pilot's groups and their plans for a flying day, asked
            // of the account's server and kept nowhere.
            val groups = logbook.makeGroups(account)
            // The open plans near a place, read as anyone from Venti and
            // written with the account's token; the form's take-offs near
            // are Today's.
            val openPlans = logbook.makeOpenPlans(account)
            openPlans.server = { (wind.server as? WindServer.Usable)?.url }
            openPlans.takeoffsNear = today::takeoffsNear
            // The pilot's alarms, on the account too: what a fetch finds
            // new becomes a notification, whether the app or its periodic
            // check fetched it, and the list is read again. The check is
            // asked for once a fetch has run signed in, and stopped when the
            // app starts signed out. A test asks for no periodic check.
            val alerts = logbook.makeAlerts(account)
            account.onAlertsFetched = { new ->
                AlertNotifications.show(app, new)
                if (!isUnderTest) AlertsCheck.schedule(app)
                scope.launch { alerts.read() }
            }
            if (!isUnderTest && !account.isSignedIn) AlertsCheck.cancel(app)
            // The sites that pilots make are sent to the app's Venti, where
            // that server takes them.
            val siteEdit = logbook.makeSiteEdit(account)
            siteEdit.server = { (wind.server as? WindServer.Usable)?.url }
            siteEdit.takesSites = { wind.takesSitesFromPilots }
            // The cockpit names the landing within reach among the
            // pilot's own waypoints.
            fly.landings = { waypoints.all.filter { it.kind == WaypointKind.LANDING } }
            // A flight shared while it is under way goes to the pilot's
            // server, with the account's token.
            fly.server = { account.account?.server.takeIf { account.isSignedIn } }
            fly.carry = account::carry
            // The ground under the glider, from the map's elevation tiles:
            // brought as the glider moves, and kept on the phone beside
            // the map's region, outside any backup. A test keeps none.
            val ground = TerrainModel(
                source = { MapSetup.terrain(app) },
                folder = if (isUnderTest) null else app.noBackupFilesDir.resolve("terrain"),
            )
            fly.terrain = { ground.terrain }
            // Maps for flying without a network, from Venti. The ground is
            // read from their elevation wherever they reach, and the
            // cockpit's map is drawn from them wherever the glider is
            // inside one. A test keeps its downloads in its own folder.
            val maps = OfflineMapsModel(
                folder = if (isUnderTest) folder.resolve("Maps") else app.noBackupFilesDir.resolve("maps"),
                scope = scope,
            )
            maps.server = { (wind.server as? WindServer.Usable)?.url }
            maps.onKeptChanged = { archives -> ground.use(archives) }
            ground.use(maps.terrainArchives)
            MapSetup.browsing = { maps.browsingMap() }
            if (!isUnderTest) {
                Connectivity.watch(app) { online -> scope.launch { maps.isOnline = online } }
            }
            fly.wantTerrain = { place ->
                maps.flying(place)
                scope.launch { ground.load(place) }
            }
            // The task the pilot keeps, which the cockpit flies.
            val tasks = logbook.makeTasks()
            if (sampleTask) {
                tasks.keep(LaunchOptions.SAMPLE_TASK_FILE)
            }
            fly.taskFile = { tasks.file }
            // A flight that ends by itself goes into the logbook as one the
            // pilot ends, also while the screen is off; the screen shows it.
            fly.savedByItself = { recording ->
                scope.launch {
                    try {
                        fly.savedFlight = logbook.adopt(recording)
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Exception) {
                        fly.problem = "The flight was recorded but could not be added to the logbook: " +
                            "${error.pilotMessage} It will be tried again the next time Venti starts."
                    }
                }
            }
            // A flight goes to a league from this phone. The DHV-XC's upload
            // password is kept where the phone keeps secrets; a test keeps
            // none and never sends.
            val leagues = Leagues(
                logbook,
                preferences,
                if (isUnderTest) MemoryUploadPasswordStore() else KeystoreUploadPasswordStore(app),
            )
            // A league's deadline by Venti's clock; named without calling the flight late while it is unread.
            leagues.clock = { wind.serverTimeOrZero() }
            return Ready(
                logbook, fly, vario, chat, wind, waypoints, airspace, sites, live, air, community, social, account, groups, openPlans, alerts, siteEdit, tasks,
                leagues, maps, ground, today, keptSites, thermalMap, radar, cables, WatchedFolder(AndroidFolders(app), preferences, logbook), preferences, files, scope,
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
            lostWatchedFolder = options.watchesALostFolder,
            slowGlider = options.usesSlowGlider,
            googleTestAccount = options.googleTestAccount,
            googleTestPicture = options.googleTestPicture,
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
    /**
     * Start with a glider slower than any the settings offer, so that the
     * sample flight's wind leaves it little way: the cockpit then says so,
     * and that can be tested.
     */
    val usesSlowGlider: Boolean = false,
    /**
     * Start with a watched folder that Android never let the app into, as
     * after the pilot took the permission away, so that the logbook's
     * notice of a lost folder can be tested without the folder picker.
     */
    val watchesALostFolder: Boolean = false,
    /**
     * Sign in "with Google" as this email through the development server's
     * made-up Google (`GET /v1/dev/google-id-token`) instead of Google's
     * sheet, so that the sign-in can be tested without a Google account.
     */
    val googleTestAccount: String? = null,
    /** That made-up Google account has a picture, which the development server makes up. */
    val googleTestPicture: Boolean = false,
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
        const val SLOW_GLIDER = "slow-glider"
        const val LOST_WATCHED_FOLDER = "lost-watched-folder"
        const val GOOGLE_TEST_ACCOUNT = "google-test-account"
        const val GOOGLE_TEST_PICTURE = "google-test-picture"

        /** The watched folder of [watchesALostFolder]: one the app holds no permission for, and its name. */
        const val LOST_FOLDER = "content://com.android.externalstorage.documents/tree/primary%3AXCTrack%2FTracklogs"
        const val LOST_FOLDER_NAME = "Tracklogs"

        /** The name the made-up Google gives the test's account. */
        const val GOOGLE_TEST_NAME = "UI Test"

        /**
         * The slow glider's trim speed, km/h: 7 km/h more than the wind of
         * the sample flight, which blows at 11.
         */
        const val SLOW_GLIDER_TRIM_SPEED = 18.0

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
                usesSlowGlider = intent.getBooleanExtra(SLOW_GLIDER, false),
                watchesALostFolder = intent.getBooleanExtra(LOST_WATCHED_FOLDER, false),
                googleTestAccount = intent.getStringExtra(GOOGLE_TEST_ACCOUNT),
                googleTestPicture = intent.getBooleanExtra(GOOGLE_TEST_PICTURE, false),
            )
        }
    }
}

/**
 * For a UI test: how many seconds the cockpit's clock runs ahead of the
 * phone's, so that a test need not wait out what the core times by the
 * clock, such as the two minutes between a landing and the save. Only a
 * test's session reads it.
 */
object TestClock {
    @Volatile
    var aheadSeconds = 0.0
}
