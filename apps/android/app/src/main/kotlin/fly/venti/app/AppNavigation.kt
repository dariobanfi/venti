package fly.venti.app

import androidx.annotation.DrawableRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.setValue
import fly.venti.app.nearby.HelpOpening

/**
 * The app's sections, as the design language's tab bar has them, with its
 * icons. The assistant and the settings are not among them: they open from
 * the Today tab's corner.
 */
enum class AppTab(val title: String, @param:DrawableRes val icon: Int) {
    Today("Today", R.drawable.venti_forecast),
    Map("Map", R.drawable.venti_map),
    Fly("Fly", R.drawable.venti_wing),
    Logbook("Logbook", R.drawable.venti_track),
    League("Community", R.drawable.venti_league),
}

/** A page of the Today tab, opened from its corner. */
enum class TodayRoute {
    /** The settings, with the pages reached from them. */
    Settings,

    /** The assistant. */
    Assistant,
}

/** A screen reached from the logbook's list. */
sealed interface LogbookRoute {
    /** One flight. */
    data class Flight(val id: String) : LogbookRoute

    /** The statistics. */
    data object Statistics : LogbookRoute

    /** "How you fly": the pilot's tendencies, from the statistics. */
    data object Tendencies : LogbookRoute
}

/** A screen reached from the League tab's lists, or from a flight flown from a site. */
sealed interface LeagueRoute {
    /** Another pilot's public flight, by its address on the server. */
    data class PublicFlight(val id: String) : LeagueRoute

    /** A pilot's public flights: the pilot's handle, and the name to show until the server has said it. */
    data class Pilot(val handle: String, val name: String) : LeagueRoute

    /** The whole feed of the pilots one follows, from Community. */
    data object Feed : LeagueRoute

    /** Everything the pilot was told, from Community. */
    data object Notifications : LeagueRoute

    /** The groups' section: the pilot's groups, invites, groups near and found. */
    data object Groups : LeagueRoute

    /** A group's chat: its identifier, and its name until the server has said it. */
    data class Chat(val id: String, val name: String) : LeagueRoute

    /** A group's page: its members and what the pilot may do in it. */
    data class Group(val id: String) : LeagueRoute
}

/** A screen reached from the settings. */
enum class SettingsRoute {
    /** Whose data and software the app uses. */
    Credits,

    /** The licences of the software inside the app, a page of "Data and licences". */
    Licences,

    /** The pilot's waypoints. */
    Waypoints,

    /** The pilot's airspace. */
    Airspace,

    /** The glider and the cockpit's switches: "Glider and cockpit". */
    GliderAndCockpit,

    /** The pilot's account on a Venti server. */
    Account,

    /** Maps for flying without a network. */
    OfflineMaps,

    /** The pilot's alarms and what they fired: a page of the account. */
    Alerts,
}

/** A screen reached from the cockpit's settings, which leaves the cockpit for it until it is closed. */
enum class FlyRoute {
    /** The pilot's airspace: what the cockpit will warn from. */
    Airspace,

    /** The pilot's task: what the cockpit will lead along. */
    Task,

    /** Maps for flying without a network. */
    OfflineMaps,

    /** The glider's numbers and what the cockpit shows: the settings' page of that name. */
    GliderAndCockpit,
}

/**
 * Where the pilot is in the app. Held in one place so that a file opened
 * from outside can bring its flight to the front.
 */
class AppNavigation {
    var tab: AppTab by mutableStateOf(AppTab.Today)

    /** The page opened from the Today tab's corner, if any. */
    var todayRoute: TodayRoute? by mutableStateOf(null)

    /** The screens opened on top of the logbook's list, the newest last. */
    val logbookPath = mutableStateListOf<LogbookRoute>()

    /** The screens opened on top of the League tab's lists, the newest last. */
    val leaguePath = mutableStateListOf<LeagueRoute>()

    /** The screen opened on top of the settings, if any. */
    var settingsRoute: SettingsRoute? by mutableStateOf(null)

    /** The screen opened from the cockpit's settings, if any. */
    var flyRoute: FlyRoute? by mutableStateOf(null)

    /** The sheet of help after landing, over any tab or the cockpit, while it is open. */
    var help: HelpOpening? by mutableStateOf(null)

    /** An open plan to show on Community, from a notification, until Community has opened it. */
    var openPlanAsked: String? by mutableStateOf(null)

    /**
     * Whether the tab bar shows: on the sections' first screens, and on the
     * settings and the assistant, which are pages of the Today tab. A
     * flight or the statistics have the whole screen.
     */
    val showsTabBar: Boolean
        get() = when (tab) {
            AppTab.Today -> todayRoute != TodayRoute.Settings || settingsRoute == null
            AppTab.Logbook -> logbookPath.isEmpty()
            AppTab.Fly -> flyRoute == null
            AppTab.League -> leaguePath.isEmpty()
            AppTab.Map -> true
        }

    /** Whether there is somewhere to go back to within the section in front. */
    val canGoBack: Boolean
        get() = when (tab) {
            AppTab.Today -> todayRoute != null
            AppTab.Logbook -> logbookPath.isNotEmpty()
            AppTab.Fly -> flyRoute != null
            AppTab.League -> leaguePath.isNotEmpty()
            AppTab.Map -> false
        }

    /**
     * The pilot tapped a tab. Tapped again while it is in front, the Today
     * tab goes back to Today itself, whatever page lay over it.
     */
    fun choose(chosen: AppTab) {
        if (chosen == tab && chosen == AppTab.Today) {
            todayRoute = null
            settingsRoute = null
        }
        // The Fly tab is the cockpit: chosen, it opens it.
        if (chosen == AppTab.Fly) {
            startFlight()
            return
        }
        tab = chosen
    }

    /**
     * Set when the cockpit was asked for: the Fly tab was chosen, or
     * "Start flight" on a site's page. The Fly tab opens it and takes this
     * back.
     */
    var wantsCockpit: Boolean by mutableStateOf(false)

    /** The tab the pilot was on before the cockpit, where "End" on the ground leads back to. */
    private var beforeFly: AppTab = AppTab.Today

    /** The Fly tab comes to the front and opens the cockpit, which waits for the take-off. */
    fun startFlight() {
        showCockpit()
        wantsCockpit = true
    }

    /** The cockpit's tab in front, as when a take-off was seen from another tab: the cockpit covers it. */
    fun showCockpit() {
        if (tab != AppTab.Fly) beforeFly = tab
        tab = AppTab.Fly
        flyRoute = null
    }

    /** Leaves the cockpit's tab for the one the pilot was on before it. */
    fun leaveFly() {
        if (tab == AppTab.Fly) tab = beforeFly
        flyRoute = null
    }

    /** Opens the settings, from the Today tab's corner or from a screen that needs one set. */
    fun showSettings() {
        tab = AppTab.Today
        todayRoute = TodayRoute.Settings
        settingsRoute = null
    }

    /** Opens the pilot's account, from a screen that needs the pilot signed in. */
    fun showAccount() {
        showSettings()
        settingsRoute = SettingsRoute.Account
    }

    /** Opens an open plan's page on Community: from a notification of a new plan near. */
    fun showOpenPlan(id: String) {
        tab = AppTab.League
        leaguePath.clear()
        openPlanAsked = id
    }

    /** Opens the pilot's alarms and what they fired: from a notification of what fired. */
    fun showAlerts() {
        showSettings()
        settingsRoute = SettingsRoute.Alerts
    }

    fun show(flight: String) {
        tab = AppTab.Logbook
        logbookPath.clear()
        logbookPath += LogbookRoute.Flight(flight)
    }

    /** Brings the logbook's list to the front. */
    fun showLogbook() {
        tab = AppTab.Logbook
        logbookPath.clear()
    }

    fun open(route: LogbookRoute) {
        logbookPath += route
    }

    fun open(route: LeagueRoute) {
        leaguePath += route
    }

    /** The groups' section, on the League tab, from wherever it was asked for. */
    fun showGroups() {
        tab = AppTab.League
        leaguePath.clear()
        leaguePath += LeagueRoute.Groups
    }

    /** A group's chat, on the League tab, from a tapped push: by the id until the server says the name. */
    fun showChat(id: String) {
        tab = AppTab.League
        leaguePath.clear()
        leaguePath += LeagueRoute.Groups
        leaguePath += LeagueRoute.Chat(id, "")
    }

    /** One of everyone's flights, on the League tab, from wherever it was found. */
    fun showPublic(flight: String) {
        tab = AppTab.League
        leaguePath.clear()
        leaguePath += LeagueRoute.PublicFlight(flight)
    }

    /** A pilot's page, on the League tab, from wherever the pilot was named: by the handle until the server says the name. */
    fun showPilot(handle: String) {
        tab = AppTab.League
        leaguePath.clear()
        leaguePath += LeagueRoute.Pilot(handle, "@$handle")
    }

    /** One step back. Returns false if there is nowhere to go back to. */
    fun back(): Boolean = when {
        tab == AppTab.Logbook && logbookPath.isNotEmpty() -> {
            logbookPath.removeAt(logbookPath.lastIndex)
            true
        }
        tab == AppTab.League && leaguePath.isNotEmpty() -> {
            leaguePath.removeAt(leaguePath.lastIndex)
            true
        }
        tab == AppTab.Today && todayRoute == TodayRoute.Settings && settingsRoute != null -> {
            // The alarms are a page of the account, the licences one of the
            // credits: back leads there.
            settingsRoute = when (settingsRoute) {
                SettingsRoute.Alerts -> SettingsRoute.Account
                SettingsRoute.Licences -> SettingsRoute.Credits
                else -> null
            }
            true
        }
        tab == AppTab.Today && todayRoute != null -> {
            todayRoute = null
            true
        }
        tab == AppTab.Fly && flyRoute != null -> {
            flyRoute = null
            true
        }
        else -> false
    }

    companion object {
        private const val STATISTICS = "statistics"
        private const val TENDENCIES = "tendencies"
        private const val FLIGHT = "flight:"
        private const val PUBLIC_FLIGHT = "public-flight:"
        private const val PILOT = "pilot:"
        private const val FEED = "feed"
        private const val NOTIFICATIONS = "notifications"
        private const val GROUPS = "groups"
        private const val CHAT = "chat:"
        private const val GROUP = "group:"

        /** Keeps the pilot's place when Android rebuilds the screen. */
        val Saver = listSaver<AppNavigation, String>(
            save = { navigation ->
                listOf(
                    navigation.tab.name,
                    navigation.settingsRoute?.name.orEmpty(),
                    navigation.flyRoute?.name.orEmpty(),
                    navigation.todayRoute?.name.orEmpty(),
                ) +
                    navigation.logbookPath.map { route ->
                        when (route) {
                            is LogbookRoute.Flight -> FLIGHT + route.id
                            LogbookRoute.Statistics -> STATISTICS
                            LogbookRoute.Tendencies -> TENDENCIES
                        }
                    } +
                    navigation.leaguePath.map { route ->
                        when (route) {
                            is LeagueRoute.PublicFlight -> PUBLIC_FLIGHT + route.id
                            // A handle has no space in it; a name may.
                            is LeagueRoute.Pilot -> "$PILOT${route.handle} ${route.name}"
                            LeagueRoute.Feed -> FEED
                            LeagueRoute.Notifications -> NOTIFICATIONS
                            LeagueRoute.Groups -> GROUPS
                            // An identifier has no space in it; a name may.
                            is LeagueRoute.Chat -> "$CHAT${route.id} ${route.name}"
                            is LeagueRoute.Group -> GROUP + route.id
                        }
                    }
            },
            restore = { saved ->
                AppNavigation().apply {
                    tab = AppTab.entries.firstOrNull { it.name == saved[0] } ?: AppTab.Today
                    settingsRoute = SettingsRoute.entries.firstOrNull { it.name == saved[1] }
                    flyRoute = FlyRoute.entries.firstOrNull { it.name == saved[2] }
                    todayRoute = TodayRoute.entries.firstOrNull { it.name == saved[3] }
                    saved.drop(4).forEach { route ->
                        when {
                            route == STATISTICS -> logbookPath += LogbookRoute.Statistics
                            route == TENDENCIES -> logbookPath += LogbookRoute.Tendencies
                            route == FEED -> leaguePath += LeagueRoute.Feed
                            route == NOTIFICATIONS -> leaguePath += LeagueRoute.Notifications
                            route == GROUPS -> leaguePath += LeagueRoute.Groups
                            route.startsWith(CHAT) -> route.removePrefix(CHAT).let { chat ->
                                leaguePath += LeagueRoute.Chat(chat.substringBefore(' '), chat.substringAfter(' ', ""))
                            }
                            route.startsWith(GROUP) -> leaguePath += LeagueRoute.Group(route.removePrefix(GROUP))
                            route.startsWith(PUBLIC_FLIGHT) -> leaguePath += LeagueRoute.PublicFlight(route.removePrefix(PUBLIC_FLIGHT))
                            route.startsWith(PILOT) -> route.removePrefix(PILOT).let { pilot ->
                                leaguePath += LeagueRoute.Pilot(pilot.substringBefore(' '), pilot.substringAfter(' ', ""))
                            }
                            else -> logbookPath += LogbookRoute.Flight(route.removePrefix(FLIGHT))
                        }
                    }
                }
            },
        )
    }
}
