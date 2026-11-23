package fly.venti.app

import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Density
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.IntentCompat
import fly.venti.app.alerts.AlertNotifications
import fly.venti.app.design.VentiTheme
import fly.venti.app.map.MapSetup
import fly.venti.app.map.MapStyle
import fly.venti.app.widget.TakeoffWidget

/** The app's one screen-holder. Everything on screen is Compose inside it. */
class MainActivity : ComponentActivity() {
    /** Flight files another app has handed over and that are yet to be imported. */
    private var incoming: List<Uri> by mutableStateOf(emptyList())

    /** Whether a notification of what an alarm fired opened the app, and "Alerts" is yet to be shown. */
    private var opensAlerts: Boolean by mutableStateOf(false)

    /** Whether a tap on the home-screen widget opened the app, and Today is yet to come to the front. */
    private var opensToday: Boolean by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        val options = LaunchOptions.from(intent)
        // The app draws behind the status bar and the navigation bar, so a
        // map can fill the screen. In a forced appearance the bars' icons
        // follow it, not the phone.
        // A UI test may change the forced appearance while the app runs;
        // a screen Android rebuilt keeps what the test chose.
        if (savedInstanceState == null) ForcedAppearance.dark = options.forcedDark
        showBars(ForcedAppearance.dark)
        super.onCreate(savedInstanceState)

        val app = application as VentiApplication
        val session = when {
            !options.isUiTesting -> app.session
            // A screen Android rebuilt keeps the test's logbook.
            savedInstanceState != null -> app.testSession ?: app.startTestSession(options)
            else -> app.startTestSession(options)
        }
        MapSetup.source = if (options.usesBundledMap) MapStyle.Source.Bundled else MapStyle.Source.Online
        MapSetup.movesAtOnce = options.isUiTesting

        if (savedInstanceState == null) {
            incoming = flightFiles(intent)
            opensAlerts = intent.getBooleanExtra(AlertNotifications.OPENS_WHAT_FIRED, false)
            opensToday = intent.getBooleanExtra(TakeoffWidget.OPENS_TODAY, false)
        }

        setContent {
            val forced = ForcedAppearance.dark
            LaunchedEffect(forced) { showBars(forced) }
            // A UI test's look at a phone 360 dp wide with text at 130 %,
            // without Android making the screen anew.
            val density = if (ForcedAppearance.narrowWithLargeText) {
                Density(
                    density = LocalWindowInfo.current.containerSize.width / ForcedAppearance.NARROW_DP,
                    fontScale = ForcedAppearance.LARGE_TEXT,
                )
            } else {
                LocalDensity.current
            }
            CompositionLocalProvider(LocalDensity provides density) {
                VentiTheme(dark = forced ?: isSystemInDarkTheme()) {
                    when (session) {
                        is Session.Ready -> RootScreen(
                            session,
                            incoming,
                            onImported = { incoming = emptyList() },
                            opensAlerts = opensAlerts,
                            onAlertsOpened = { opensAlerts = false },
                            opensToday = opensToday,
                            onTodayOpened = { opensToday = false },
                        )
                        is Session.Failed -> DataFailure(session.message)
                    }
                }
            }
        }
    }

    // The app is already open and another app hands it a file.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        incoming = flightFiles(intent)
        opensAlerts = intent.getBooleanExtra(AlertNotifications.OPENS_WHAT_FIRED, false)
        opensToday = intent.getBooleanExtra(TakeoffWidget.OPENS_TODAY, false)
    }

    private fun showBars(forcedDark: Boolean?) {
        val bars = when (forcedDark) {
            true -> SystemBarStyle.dark(Color.TRANSPARENT)
            false -> SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
            null -> SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT)
        }
        enableEdgeToEdge(statusBarStyle = bars, navigationBarStyle = bars)
    }

    /** The files an intent brings: one opened with Venti, or one or more shared to it. */
    private fun flightFiles(intent: Intent?): List<Uri> = when (intent?.action) {
        Intent.ACTION_VIEW -> listOfNotNull(intent.data)
        Intent.ACTION_SEND ->
            listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
        Intent.ACTION_SEND_MULTIPLE ->
            IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        else -> emptyList()
    }
}

@Composable
private fun DataFailure(message: String) {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Column(
            modifier = Modifier.padding(32.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(
                painterResource(R.drawable.venti_caution),
                contentDescription = null,
                modifier = Modifier.size(48.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.size(16.dp))
            Text(
                "Venti could not open its data",
                style = MaterialTheme.typography.titleLarge,
                textAlign = TextAlign.Center,
            )
            Spacer(Modifier.size(8.dp))
            Text(
                message,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
        }
    }
}

/**
 * The appearance a UI test forced ([LaunchOptions.DARK]), null to follow
 * the phone. A test changes it while the app runs, to keep a picture of a
 * screen in the other appearance without walking to it again. Always null
 * in a release build, which ignores launch options.
 */
object ForcedAppearance {
    var dark: Boolean? by mutableStateOf(null)

    /**
     * The app laid out as on a phone [NARROW_DP] wide with text at
     * [LARGE_TEXT] times its size, what `design/AGENTS.md` asks every
     * screen to be looked at with: Compose's density, not Android's, so
     * that nothing is made anew. The map draws at the phone's own density.
     */
    var narrowWithLargeText: Boolean by mutableStateOf(false)

    const val NARROW_DP = 360f
    const val LARGE_TEXT = 1.3f
}
