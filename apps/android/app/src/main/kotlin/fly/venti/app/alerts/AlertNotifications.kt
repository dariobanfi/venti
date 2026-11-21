package fly.venti.app.alerts

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import fly.venti.app.MainActivity
import fly.venti.app.R
import fly.venti.app.Session
import fly.venti.app.VentiApplication
import fly.venti.core.FiredLine
import java.util.concurrent.TimeUnit

/**
 * What an alarm fired, as a notification of the phone's: the core's title
 * and line, opening the app's "What fired". No push service: the lines come
 * from the app's own fetching, in front and from [AlertsCheck].
 */
object AlertNotifications {
    private const val CHANNEL = "alerts"

    /** The extra of the intent a notification opens the app with. */
    const val OPENS_WHAT_FIRED = "opens-what-fired"

    /** Whether the phone lets the app show notifications: from Android 13 a permission asked for. */
    fun areAllowed(context: Context): Boolean =
        Build.VERSION.SDK_INT < 33 ||
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    /** One notification for each line, the oldest first. Nothing without the permission. */
    fun show(context: Context, lines: List<FiredLine>) {
        if (lines.isEmpty() || !areAllowed(context)) return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, AlertWords.CHANNEL, NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = AlertWords.CHANNEL_ABOUT
            },
        )
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(OPENS_WHAT_FIRED, true),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        for (line in lines) {
            val notification = Notification.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(line.title)
                .setContentText(line.line)
                .setStyle(Notification.BigTextStyle().bigText(line.line))
                .setWhen(line.firedAtUnixS * 1000)
                .setShowWhen(true)
                .setContentIntent(open)
                .setAutoCancel(true)
                .build()
            // One for each thing that fired: by the alarm and the moment.
            manager.notify("${line.alertId}-${line.firedAtUnixS}", 0, notification)
        }
    }
}

/**
 * The app's periodic check of the pilot's alarms: every 30 minutes, with a
 * network, it fetches what fired and shows what is new. Android decides
 * the moment within that; there is no push.
 */
class AlertsCheck(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        val session = (applicationContext as VentiApplication).session as? Session.Ready ?: return Result.success()
        // The session's own sync shows what is new: see Session.start.
        session.account.syncAlerts()
        return Result.success()
    }

    companion object {
        private const val NAME = "alerts-check"

        /** Asks Android to run the check from now on; one already asked for stays as it is. */
        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<AlertsCheck>(30, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        /** Stops the check, signed out: there are no alarms then. */
        fun cancel(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(NAME)
        }
    }
}
