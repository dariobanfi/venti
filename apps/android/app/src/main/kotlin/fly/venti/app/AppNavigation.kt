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
}

/** A screen reached from the settings. */
enum class SettingsRoute {
    /** Whose data and software the app uses. */
    Credits,

    /** The pilot's waypoints. */
    Waypoints,
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

    /** Whether the pilot is on one of the four sections' first screens, where the tab bar shows. */
    val isAtTopLevel: Boolean
        get() = when (tab) {
            AppTab.Logbook -> logbookPath.isEmpty()
            AppTab.Settings -> settingsRoute == null
            else -> true
        }

    fun show(flight: String) {
        tab = AppTab.Logbook
        logbookPath.clear()
        logbookPath += LogbookRoute.Flight(flight)
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
        else -> false
    }

    companion object {
        private const val STATISTICS = "statistics"
        private const val FLIGHT = "flight:"

        /** Keeps the pilot's place when Android rebuilds the screen. */
        val Saver = listSaver<AppNavigation, String>(
            save = { navigation ->
                listOf(navigation.tab.name, navigation.settingsRoute?.name.orEmpty()) +
                    navigation.logbookPath.map { route ->
                        when (route) {
                            is LogbookRoute.Flight -> FLIGHT + route.id
                            LogbookRoute.Statistics -> STATISTICS
                        }
                    }
            },
            restore = { saved ->
                AppNavigation().apply {
                    tab = AppTab.valueOf(saved[0])
                    settingsRoute = SettingsRoute.entries.firstOrNull { it.name == saved[1] }
                    saved.drop(2).forEach { route ->
                        logbookPath += if (route == STATISTICS) {
                            LogbookRoute.Statistics
                        } else {
                            LogbookRoute.Flight(route.removePrefix(FLIGHT))
                        }
                    }
                }
            },
        )
    }
}
