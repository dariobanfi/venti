package fly.venti.app

import androidx.annotation.DrawableRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.setValue

/**
 * The app's sections. More join as the product grows: the community comes
 * next. Five is as many as a Material navigation bar takes.
 */
enum class AppTab(val title: String, @param:DrawableRes val icon: Int) {
    Map("Map", R.drawable.ic_map),
    Fly("Fly", R.drawable.ic_paragliding),
    Logbook("Logbook", R.drawable.ic_book),
    Assistant("Assistant", R.drawable.ic_auto_awesome),
    Settings("Settings", R.drawable.ic_settings),
}

/** A screen reached from the logbook's list. */
sealed interface LogbookRoute {
    /** One flight. */
    data class Flight(val id: String) : LogbookRoute

    /** The statistics. */
    data object Statistics : LogbookRoute

    /** Another pilot's public flight, by its address on the server. */
    data class PublicFlight(val id: String) : LogbookRoute

    /** A pilot's public flights: the pilot's handle, and the name to show until the server has said it. */
    data class Pilot(val handle: String, val name: String) : LogbookRoute
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
}

/** A screen reached from the Fly tab. */
enum class FlyRoute {
    /** The pilot's airspace: what the cockpit will warn from. */
    Airspace,

    /** The pilot's task: what the cockpit will lead along. */
    Task,

    /** Live tracking: who may watch a flight while it is under way. */
    Live,
}

/**
 * Where the pilot is in the app. Held in one place so that a file opened
 * from outside can bring its flight to the front.
 */
class AppNavigation {
    var tab: AppTab by mutableStateOf(AppTab.Map)

    /** The screens opened on top of the logbook's list, the newest last. */
    val logbookPath = mutableStateListOf<LogbookRoute>()

    /** The screen opened on top of the settings, if any. */
    var settingsRoute: SettingsRoute? by mutableStateOf(null)

    /** The screen opened on top of the Fly tab, if any. */
    var flyRoute: FlyRoute? by mutableStateOf(null)

    /** Whether the pilot is on one of the sections' first screens, where the tab bar shows. */
    val isAtTopLevel: Boolean
        get() = when (tab) {
            AppTab.Logbook -> logbookPath.isEmpty()
            AppTab.Settings -> settingsRoute == null
            AppTab.Fly -> flyRoute == null
            else -> true
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

    /** One step back. Returns false if there is nowhere to go back to. */
    fun back(): Boolean = when {
        tab == AppTab.Logbook && logbookPath.isNotEmpty() -> {
            logbookPath.removeAt(logbookPath.lastIndex)
            true
        }
        tab == AppTab.Settings && settingsRoute != null -> {
            settingsRoute = null
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
                ) +
                    navigation.logbookPath.map { route ->
                        when (route) {
                            is LogbookRoute.Flight -> FLIGHT + route.id
                            is LogbookRoute.PublicFlight -> PUBLIC_FLIGHT + route.id
                            // A handle has no space in it; a name may.
                            is LogbookRoute.Pilot -> "$PILOT${route.handle} ${route.name}"
                            LogbookRoute.Statistics -> STATISTICS
                        }
                    }
            },
            restore = { saved ->
                AppNavigation().apply {
                    tab = AppTab.valueOf(saved[0])
                    settingsRoute = SettingsRoute.entries.firstOrNull { it.name == saved[1] }
                    flyRoute = FlyRoute.entries.firstOrNull { it.name == saved[2] }
                    saved.drop(3).forEach { route ->
                        logbookPath += when {
                            route == STATISTICS -> LogbookRoute.Statistics
                            route.startsWith(PUBLIC_FLIGHT) -> LogbookRoute.PublicFlight(route.removePrefix(PUBLIC_FLIGHT))
                            route.startsWith(PILOT) -> route.removePrefix(PILOT).let { pilot ->
                                LogbookRoute.Pilot(pilot.substringBefore(' '), pilot.substringAfter(' ', ""))
                            }
                            else -> LogbookRoute.Flight(route.removePrefix(FLIGHT))
                        }
                    }
                }
            },
        )
    }
}
