package fly.venti.app.fly

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import fly.venti.app.MainActivity
import fly.venti.app.R
import fly.venti.app.Session
import fly.venti.app.VentiApplication

/**
 * Keeps a flight being recorded with the screen off.
 *
 * Android stops giving positions to an app nobody is looking at, and may
 * stop the app altogether, unless it runs a foreground service of the
 * location kind with a notification the pilot can see. This service is that
 * and nothing else: the sensors and the recording belong to the flying
 * session, which lives as long as the app's process, and this service is
 * what keeps the process alive and awake. It is started while the cockpit
 * is on screen, as Android requires, and stopped when the flight is
 * finished.
 *
 * If the system does end the app, Android starts this service again by
 * itself (it is sticky), which starts the app's process and with it the
 * session: what was written before is brought into the logbook as an
 * interrupted flight, and the cockpit opens again and records from the
 * take-off, which in the air the core notices within seconds. The core
 * joins no two files, so such a flight is two in the logbook. A session
 * that cannot be had (the logbook does not open) leaves the service
 * stopped.
 *
 * To verify on real phones: that a recording survives hours with the screen
 * off, and what the battery savers of the phone makers do to it.
 */
class RecordingService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        try {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION,
                notification(),
                if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION else 0,
            )
        } catch (_: RuntimeException) {
            // Android refused: the app is not on screen, or may not use the
            // position. The session goes on for as long as the app is open.
            stopSelf()
            return START_NOT_STICKY
        }
        // Started again by Android after it ended the app's process: the
        // cockpit is opened again, without a screen.
        if (intent == null) {
            val session = (application as? VentiApplication)?.session as? Session.Ready
            if (session == null) {
                stopSelf()
                return START_NOT_STICKY
            }
            session.fly.reopenAfterRestart()
        }
        if (wakeLock == null) {
            // Without it the processor sleeps between positions, and the
            // vario's tone and the Bluetooth vario's data stall.
            wakeLock = getSystemService(PowerManager::class.java)
                ?.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "venti:recording")
                ?.also { it.acquire(LONGEST_FLIGHT_MILLISECONDS) }
        }
        // Started again after the system ended the app; stopped for good
        // only when the flight is finished.
        return START_STICKY
    }

    override fun onDestroy() {
        wakeLock?.takeIf { it.isHeld }?.release()
        wakeLock = null
        super.onDestroy()
    }

    private fun notification(): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager?.createNotificationChannel(
            // Low importance: shown, without a sound of its own.
            NotificationChannel(CHANNEL, "Flight recording", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Shown while Venti records a flight."
            },
        )
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val builder = Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle("Recording your flight")
            .setContentText("Open Venti to see the cockpit or to finish the flight.")
            .setContentIntent(open)
            .setOngoing(true)
            .setCategory(Notification.CATEGORY_SERVICE)
        if (Build.VERSION.SDK_INT >= 31) {
            // Shown at once, not after Android's usual ten seconds.
            builder.setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE)
        }
        return builder.build()
    }

    companion object {
        private const val CHANNEL = "recording"
        private const val NOTIFICATION = 1

        /** The wake lock lets go by itself after this long, should a session never be finished. */
        private const val LONGEST_FLIGHT_MILLISECONDS = 14 * 60 * 60 * 1000L

        fun start(context: Context) {
            try {
                ContextCompat.startForegroundService(context, Intent(context, RecordingService::class.java))
            } catch (_: RuntimeException) {
                // Not allowed from where the app is now. See onStartCommand.
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, RecordingService::class.java))
        }
    }
}
