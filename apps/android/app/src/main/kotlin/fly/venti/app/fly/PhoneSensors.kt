package fly.venti.app.fly

import android.content.Context
import fly.venti.core.FlightSession

/**
 * The sensors of a real phone, wired to the core's session: positions from
 * the satellite receiver, pressure from the phone's own sensor, and the
 * bytes of a Bluetooth vario if the pilot has chosen one. While they run,
 * the recording service keeps the app alive with the screen off.
 */
class PhoneSensors(private val context: Context, private val vario: VarioLink) : FlightSensors {
    private val location = LocationProvider(context)
    private val barometer = BarometerProvider(context)

    override fun start(session: FlightSession, onProblem: (String) -> Unit) {
        location.onSample = session::onLocation
        location.onProblem = onProblem
        barometer.onPressure = session::onPressure
        vario.onBytes = { bytes -> session.onInstrumentBytes(System.currentTimeMillis() / 1000.0, bytes) }

        // The service first: Android lets it start only while the app is on
        // screen, and without the position permission it would be refused.
        if (LocationProvider.isAllowed(context)) {
            RecordingService.start(context)
        }
        location.start()
        barometer.start()
        vario.reconnectToRememberedVario()
    }

    override fun stop() {
        location.stop()
        barometer.stop()
        vario.onBytes = null
        RecordingService.stop(context)
    }
}
