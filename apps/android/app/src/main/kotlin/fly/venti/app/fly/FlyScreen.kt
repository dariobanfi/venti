package fly.venti.app.fly

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import fly.venti.app.design.Venti
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import app.venti.design.VentiSize
import app.venti.design.VentiSpace

/**
 * The Fly tab: the cockpit. Chosen, the tab asks Android for what the
 * cockpit needs and opens it, which then covers the whole app; nothing is
 * listed first. What was listed before take-off is behind the cockpit's
 * settings button.
 *
 * This screen shows only where the tab is in front without a cockpit:
 * after a session that ended by itself, or when Android rebuilt the app.
 * Its one button opens the cockpit as the tab does.
 */
@Composable
fun FlyScreen(
    fly: FlySession,
    vario: VarioLink,
    bars: PaddingValues,
    /** Whether the cockpit was asked for: the tab was chosen, or "Start flight" on a site's page. */
    wantsCockpit: Boolean,
    /** Called once the cockpit asked for is on its way. */
    onCockpitTaken: () -> Unit,
) {
    // Android asks the pilot before the app may use the position. The
    // flight starts whatever the answer: the cockpit says what is missing.
    val askThenStart = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        fly.startFlight()
    }
    // The cockpit opens once the answer is in, so no word of the app's own
    // stands over the system's question.
    LaunchedEffect(wantsCockpit) {
        if (wantsCockpit) {
            onCockpitTaken()
            if (!fly.isOpen) askThenStart.launch(permissionsToFly(vario))
        }
    }

    Column(
        Modifier.fillMaxSize().padding(bottom = bars.calculateBottomPadding()).padding(VentiSpace.s4),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        // The iPhone's title and line over the one button.
        Text(FlyWords.TITLE, style = MaterialTheme.typography.headlineMedium, modifier = Modifier.semantics { heading() })
        Text(
            FlyWords.LINE,
            style = MaterialTheme.typography.bodyLarge,
            color = Venti.colours.textSecondary,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(top = VentiSpace.s2, bottom = VentiSpace.s5),
        )
        // The screen's one accent: the action that opens the cockpit.
        Button(
            onClick = { askThenStart.launch(permissionsToFly(vario)) },
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touchFlight).testTag("open-cockpit"),
            colors = ButtonDefaults.buttonColors(
                containerColor = MaterialTheme.colorScheme.tertiary,
                contentColor = MaterialTheme.colorScheme.onTertiary,
            ),
        ) {
            Text(FlyWords.OPEN_COCKPIT, style = MaterialTheme.typography.titleMedium)
        }
    }
}

/**
 * What Android has to ask the pilot before a flight: the position, to
 * record it; notifications, to show that recording goes on with the screen
 * off; and Bluetooth, if a vario was used before.
 */
private fun permissionsToFly(vario: VarioLink): Array<String> = buildList {
    add(Manifest.permission.ACCESS_FINE_LOCATION)
    add(Manifest.permission.ACCESS_COARSE_LOCATION)
    if (Build.VERSION.SDK_INT >= 33) {
        add(Manifest.permission.POST_NOTIFICATIONS)
    }
    if (vario.hasRememberedVario) {
        addAll(VarioLink.permissions)
    }
}.distinct().toTypedArray()

/** What the Fly tab says about the cockpit. */
object FlyWords {
    const val OPEN_COCKPIT = "Open the cockpit"
    const val TITLE = "Fly"
    const val LINE =
        "The cockpit waits on the ground and records from the moment you take off, with the screen off too. " +
            "Two minutes after you have landed, the flight is saved to the logbook."
}
