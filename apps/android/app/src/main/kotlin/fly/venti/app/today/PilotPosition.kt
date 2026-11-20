package fly.venti.app.today

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationManager
import android.os.CancellationSignal
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import androidx.core.location.LocationManagerCompat
import androidx.core.util.Consumer
import fly.venti.core.Coordinate

/**
 * Where the pilot is, for the take-offs around: read only once the pilot
 * has asked for it on the Today tab, and then read again without asking
 * whenever the tab is shown. A rough position does: the take-offs within
 * 30 km do not change with a few hundred metres.
 */
class PilotPosition(private val context: Context) {
    /** The latest position, once there is one. */
    var place: Coordinate? by mutableStateOf(null)
        private set

    /** Whether the pilot has said no to the system's question. */
    var isDenied: Boolean by mutableStateOf(false)

    /** Whether the app may read the position without asking. */
    val isAllowed: Boolean
        get() = PERMISSIONS.any { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }

    /** Reads the position if the app may; asks nobody. */
    fun readIfAllowed() {
        if (!isAllowed) return
        val manager = context.getSystemService(LocationManager::class.java) ?: return
        val providers = try {
            manager.getProviders(true)
        } catch (_: SecurityException) {
            return
        }
        // The freshest the phone knows, at once; then a new one.
        providers.mapNotNull { provider -> lastKnown(manager, provider) }
            .maxByOrNull { it.elapsedRealtimeNanos }
            ?.let(::use)
        val provider = listOf(LocationManager.NETWORK_PROVIDER, LocationManager.GPS_PROVIDER)
            .firstOrNull { it in providers } ?: return
        try {
            val found = Consumer<Location?> { location -> location?.let(::use) }
            val notCancelled: CancellationSignal? = null
            LocationManagerCompat.getCurrentLocation(manager, provider, notCancelled, ContextCompat.getMainExecutor(context), found)
        } catch (_: SecurityException) {
            // Taken back in the meantime: the place stays what it was.
        }
    }

    private fun lastKnown(manager: LocationManager, provider: String): Location? = try {
        manager.getLastKnownLocation(provider)
    } catch (_: SecurityException) {
        null
    }

    private fun use(location: Location) {
        if (location.hasAccuracy() && location.accuracy >= 0) {
            place = Coordinate(location.latitude, location.longitude)
        }
    }

    companion object {
        val PERMISSIONS = arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION, Manifest.permission.ACCESS_FINE_LOCATION)
    }
}
