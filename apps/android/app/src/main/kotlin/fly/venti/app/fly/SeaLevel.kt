package fly.venti.app.fly

import android.content.Context
import android.location.Location
import android.location.LocationManager
import android.location.OnNmeaMessageListener
import android.location.altitude.AltitudeConverter
import android.os.Build
import android.os.Handler
import java.io.IOException

/**
 * Turns the altitude Android reports into the altitude a pilot means.
 *
 * Android gives height above the WGS 84 ellipsoid, a smooth mathematical
 * figure of the Earth. Altitude on a map, on an altimeter and in the core is
 * height above mean sea level, and the two differ by the height of the
 * geoid: about 45 to 50 metres in the Alps. From Android 14 the system can
 * convert; before that, the satellite receiver's own text output says how
 * far apart the two are where the phone is.
 *
 * To verify on real phones: both ways. If neither has an answer, the
 * ellipsoid height is passed on uncorrected rather than no altitude at all.
 */
class SeaLevel(private val context: Context) {
    private val converter = if (Build.VERSION.SDK_INT >= 34) AltitudeConverter() else null

    /** The geoid's height above the ellipsoid here, from the receiver. */
    @Volatile
    private var geoidHeightM: Double? = null

    private val listener = OnNmeaMessageListener { message, _ ->
        Nmea.geoidHeight(message)?.let { geoidHeightM = it }
    }

    /** Starts listening to the receiver. Needs the location permission. */
    fun start(manager: LocationManager, handler: Handler) {
        geoidHeightM = null
        try {
            manager.addNmeaListener(listener, handler)
        } catch (_: SecurityException) {
            // Without the permission there are no positions either.
        }
    }

    fun stop(manager: LocationManager) {
        manager.removeNmeaListener(listener)
    }

    /**
     * The position's altitude above mean sea level in metres, or `null` if
     * it has no altitude. May read a file the first time: not for the main
     * thread.
     */
    fun altitude(location: Location): Double? {
        if (!location.hasAltitude()) return null
        if (Build.VERSION.SDK_INT >= 34 && converter != null) {
            try {
                converter.addMslAltitudeToLocation(context, location)
            } catch (_: IOException) {
                // The system's geoid data could not be read.
            } catch (_: IllegalArgumentException) {
                // A position the converter does not accept.
            }
            if (location.hasMslAltitude()) return location.mslAltitudeMeters
        }
        return location.altitude - (geoidHeightM ?: 0.0)
    }
}

/** The one thing the app reads from a satellite receiver's text output. */
object Nmea {
    /**
     * The geoid's height above the ellipsoid, in metres, from a GGA
     * sentence: `$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47`
     * carries it as 46.9. Any other sentence, and one without a fix, gives
     * `null`.
     */
    fun geoidHeight(sentence: String): Double? {
        val fields = sentence.substringBefore('*').split(',')
        // "$GPGGA", "$GNGGA", ...: two letters for the satellite system.
        if (fields.size < 13 || fields[0].length != 6 || !fields[0].endsWith("GGA")) return null
        // Field 6 is the quality of the fix; zero and empty mean none.
        if (fields[6].isEmpty() || fields[6] == "0") return null
        // Field 11 is the geoid's height and field 12 its unit, metres.
        if (fields[12] != "M") return null
        return fields[11].toDoubleOrNull()
    }
}
