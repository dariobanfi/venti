package fly.venti.app.fly

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.os.Handler
import android.os.HandlerThread
import androidx.core.content.ContextCompat
import fly.venti.core.LocationSample

/**
 * The phone's satellite positions, for the cockpit.
 *
 * They come from Android's own location manager and its satellite receiver,
 * not from Google Play Services, so the app runs on a phone without them.
 * Updates are asked for when a flight starts and keep coming with the
 * screen off while the recording service runs.
 */
class LocationProvider(private val context: Context) {
    var onSample: ((LocationSample) -> Unit)? = null

    /** Called with words for the pilot when positions cannot be had. */
    var onProblem: ((String) -> Unit)? = null

    private val manager: LocationManager? = context.getSystemService(LocationManager::class.java)
    private val seaLevel = SeaLevel(context)
    private var thread: HandlerThread? = null

    private val listener = object : LocationListener {
        override fun onLocationChanged(location: Location) {
            onSample?.invoke(
                LocationSample(
                    timeUnixS = location.time / 1000.0,
                    lat = location.latitude,
                    lon = location.longitude,
                    altitudeM = seaLevel.altitude(location),
                    groundSpeedMs = if (location.hasSpeed()) location.speed.toDouble() else null,
                    courseDeg = if (location.hasBearing()) location.bearing.toDouble() else null,
                ),
            )
        }

        override fun onProviderDisabled(provider: String) {
            onProblem?.invoke(SWITCHED_OFF)
        }

        // Older Android versions call these without a default to fall back on.
        override fun onProviderEnabled(provider: String) = Unit

        @Deprecated("Not called from Android 10 on.")
        override fun onStatusChanged(provider: String?, status: Int, extras: android.os.Bundle?) = Unit
    }

    fun start() {
        val manager = manager
        if (manager == null || !manager.allProviders.contains(LocationManager.GPS_PROVIDER)) {
            onProblem?.invoke("This device has no satellite receiver, so Venti cannot record a flight.")
            return
        }
        if (!isAllowed(context)) {
            onProblem?.invoke(DENIED)
            return
        }
        // Readings arrive on a thread of their own and go straight to the
        // core, so a busy screen never delays a fix.
        val thread = HandlerThread("venti-location").also { it.start() }
        this.thread = thread
        val handler = Handler(thread.looper)
        try {
            seaLevel.start(manager, handler)
            // One position a second, the most a flight file holds, however
            // little the glider moved: it can hang nearly still in the air.
            manager.requestLocationUpdates(LocationManager.GPS_PROVIDER, 1000L, 0f, listener, thread.looper)
        } catch (_: SecurityException) {
            onProblem?.invoke(DENIED)
            return
        }
        if (!manager.isProviderEnabled(LocationManager.GPS_PROVIDER)) {
            onProblem?.invoke(SWITCHED_OFF)
        }
    }

    fun stop() {
        manager?.removeUpdates(listener)
        manager?.let(seaLevel::stop)
        thread?.quitSafely()
        thread = null
    }

    companion object {
        /** Whether the pilot has let the app use the precise position. */
        fun isAllowed(context: Context): Boolean =
            ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION) ==
                PackageManager.PERMISSION_GRANTED

        const val DENIED =
            "Venti is not allowed to use your precise position, so it cannot record a flight. " +
                "You can allow it in the phone's Settings, under Apps, Venti, Permissions."

        const val SWITCHED_OFF =
            "Location is switched off on this phone, so Venti gets no positions. " +
                "Switch it on in the phone's Settings."
    }
}
