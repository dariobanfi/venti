package fly.venti.app.fly

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler
import android.os.HandlerThread

/**
 * The phone's own pressure sensor. It makes a usable vario; an external
 * vario replaces it. Not every phone has one, and then the climb rate comes
 * from satellite altitude, which is slower still.
 */
class BarometerProvider(context: Context) {
    /**
     * Called with the time (seconds since 1970) and the pressure in
     * hectopascals, on a thread of the sensor's own.
     */
    var onPressure: ((Double, Double) -> Unit)? = null

    private val manager: SensorManager? = context.getSystemService(SensorManager::class.java)
    private val sensor: Sensor? = manager?.getDefaultSensor(Sensor.TYPE_PRESSURE)
    private var thread: HandlerThread? = null

    private val listener = object : SensorEventListener {
        override fun onSensorChanged(event: SensorEvent) {
            // The sensor reports hectopascals. Its own timestamp counts from
            // when the phone started, so the reading is dated on arrival.
            onPressure?.invoke(System.currentTimeMillis() / 1000.0, event.values[0].toDouble())
        }

        override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
    }

    fun start() {
        val sensor = sensor ?: return
        // Twenty readings a second: the core's filter smooths them, and more
        // readings make a quicker vario. To verify in the air.
        val thread = HandlerThread("venti-barometer").also { it.start() }
        this.thread = thread
        manager?.registerListener(listener, sensor, READING_INTERVAL_MICROSECONDS, Handler(thread.looper))
    }

    fun stop() {
        manager?.unregisterListener(listener)
        thread?.quitSafely()
        thread = null
    }

    private companion object {
        const val READING_INTERVAL_MICROSECONDS = 50_000
    }
}
