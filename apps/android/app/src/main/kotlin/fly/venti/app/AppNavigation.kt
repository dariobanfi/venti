package fly.venti.app

import androidx.annotation.DrawableRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.setValue

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
    League("League", R.drawable.venti_league),
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
}

/** A screen reached from the settings. */
enum class SettingsRoute {
    /** Whose data and software the app uses. */
    Credits,

    /** The pilot's waypoints. */
    Waypoints,

    /** The pilot's airspace. */
    Airspace,

    /** The pilot's account on a Venti server. */
    Account,

    /** Maps for flying without a network. */
    OfflineMaps,
}

/** A screen reached from the Fly tab. */
enum class FlyRoute {
    /** The pilot's airspace: what the cockpit will warn from. */
    Airspace,

    /** The pilot's task: what the cockpit will lead along. */
    Task,

    /** Live tracking: who may watch a flight while it is under way. */
    Live,

    /** Maps for flying without a network. */
    OfflineMaps,
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

    /** The screen opened on top of the Fly tab, if any. */
    var flyRoute: FlyRoute? by mutableStateOf(null)

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
        tab = chosen
    }

    /**
     * Set when the cockpit was asked for from another tab, a site's page
     * for one: the Fly tab opens it and takes this back.
     */
    var wantsCockpit: Boolean by mutableStateOf(false)

    /** "Start flight" outside the Fly tab: that tab comes to the front and opens the cockpit, which waits for the take-off. */
    fun startFlight() {
        tab = AppTab.Fly
        flyRoute = null
        wantsCockpit = true
    }

    /** Opens the settings, from the Today tab's corner or from a screen that needs one set. */
    fun showSettings() {
        tab = AppTab.Today
        todayRoute = TodayRoute.Settings
        settingsRoute = null
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

    /** One of everyone's flights, on the League tab, from wherever it was found. */
    fun showPublic(flight: String) {
        tab = AppTab.League
        leaguePath.clear()
        leaguePath += LeagueRoute.PublicFlight(flight)
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
            settingsRoute = null
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
