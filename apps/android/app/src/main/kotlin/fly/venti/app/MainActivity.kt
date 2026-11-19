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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.IntentCompat
import fly.venti.app.design.VentiTheme
import fly.venti.app.map.MapSetup
import fly.venti.app.map.MapStyle

/** The app's one screen-holder. Everything on screen is Compose inside it. */
class MainActivity : ComponentActivity() {
    /** Flight files another app has handed over and that are yet to be imported. */
    private var incoming: List<Uri> by mutableStateOf(emptyList())

    override fun onCreate(savedInstanceState: Bundle?) {
        val options = LaunchOptions.from(intent)
        // The app draws behind the status bar and the navigation bar, so a
        // map can fill the screen. In a forced appearance the bars' icons
        // follow it, not the phone.
        val bars = when (options.forcedDark) {
            true -> SystemBarStyle.dark(Color.TRANSPARENT)
            false -> SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
            null -> SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT)
        }
        enableEdgeToEdge(statusBarStyle = bars, navigationBarStyle = bars)
        super.onCreate(savedInstanceState)

        val app = application as VentiApplication
        val session = when {
            !options.isUiTesting -> app.session
            // A screen Android rebuilt keeps the test's logbook.
            savedInstanceState != null -> app.testSession ?: app.startTestSession()
            else -> app.startTestSession()
        }
        MapSetup.source = if (options.usesBundledMap) MapStyle.Source.Bundled else MapStyle.Source.Online
        MapSetup.movesAtOnce = options.isUiTesting

        if (savedInstanceState == null) {
            incoming = flightFiles(intent)
        }

        setContent {
            VentiTheme(dark = options.forcedDark ?: isSystemInDarkTheme()) {
                when (session) {
                    is Session.Ready -> RootScreen(session, incoming, onImported = { incoming = emptyList() })
                    is Session.Failed -> DataFailure(session.message)
                }
            }
        }
    }

    // The app is already open and another app hands it a file.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        incoming = flightFiles(intent)
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
                painterResource(R.drawable.ic_warning),
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
