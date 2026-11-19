package fly.venti.app.map

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import fly.venti.app.R
import fly.venti.app.logbook.LogbookModel

/**
 * The map tab. Today it shows terrain and the pilot's own flights; live
 * wind, sites and forecasts arrive with the server.
 */
@Composable
fun MapScreen(logbook: LogbookModel, bars: PaddingValues) {
    val context = LocalContext.current
    var isFollowing by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    val map = remember { OverviewMap() }
    map.onFollowingEnded = { isFollowing = false }

    val askForPosition = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) { answers ->
        if (answers.values.any { it }) {
            isFollowing = true
            problem = positionProblem(context)
        } else {
            problem = NOT_ALLOWED
        }
    }

    val statusBar = WindowInsets.statusBars.getTop(LocalDensity.current)
    val outlines = logbook.view.years.flatMap { year -> year.flights.map { it.outline } }

    Box(Modifier.fillMaxSize().padding(bottom = bars.calculateBottomPadding())) {
        MapSurface(
            content = map,
            modifier = Modifier.fillMaxSize().semantics { contentDescription = "Map" },
            onUpdate = {
                map.show(outlines, Covered(top = statusBar))
                map.follow(isFollowing)
            },
        )
        StatusBarScrim()

        // One round button, bottom right: it centres the map on the pilot
        // and follows until the map is dragged.
        FloatingActionButton(
            onClick = {
                when {
                    isFollowing -> isFollowing = false
                    mayUsePosition(context) -> {
                        isFollowing = true
                        problem = positionProblem(context)
                    }
                    else -> askForPosition.launch(POSITION_PERMISSIONS)
                }
            },
            modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
            shape = CircleShape,
            containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
            contentColor = if (isFollowing) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onSurface
            },
        ) {
            Icon(
                painterResource(if (isFollowing) R.drawable.ic_my_location else R.drawable.ic_location_searching),
                contentDescription = "Show my position",
            )
        }
    }

    problem?.let { text ->
        AlertDialog(
            onDismissRequest = { problem = null },
            title = { Text("Position unavailable") },
            text = { Text(text) },
            confirmButton = { TextButton(onClick = { problem = null }) { Text("OK") } },
        )
    }
}

private val POSITION_PERMISSIONS =
    arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION)

private const val NOT_ALLOWED =
    "Venti is not allowed to use your position. You can allow it in the phone's Settings, " +
        "under Apps, Venti, Permissions."

/** For the map an approximate position will do. */
private fun mayUsePosition(context: Context): Boolean =
    POSITION_PERMISSIONS.any { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }

/** Words for the pilot if no position can come, or `null`. */
private fun positionProblem(context: Context): String? {
    val manager = context.getSystemService(LocationManager::class.java)
    val anyProvider = manager?.getProviders(true).orEmpty().any { it != LocationManager.PASSIVE_PROVIDER }
    return if (anyProvider) null else "Your position could not be found. Location is switched off on this phone."
}
