package fly.venti.app.nearby

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.content.edit
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import fly.venti.app.BuildConfig
import fly.venti.app.HttpAnswer
import fly.venti.app.MainActivity
import fly.venti.app.R
import fly.venti.app.Session
import fly.venti.app.VentiApplication
import fly.venti.app.account.AccountModel
import fly.venti.app.alerts.AlertNotifications
import fly.venti.core.PhonePlatform
import fly.venti.core.deviceRegisterRequest
import fly.venti.core.deviceRemoveRequest
import fly.venti.core.phoneRequestDone
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume

/**
 * Push to the phone through Google's Firebase Cloud Messaging (owner
 * decision 24; docs/api.md, "Push to the phone apps").
 *
 * Firebase is started by the app itself, with the four values the build
 * read from `app/google-services.json` (`app/build.gradle.kts`), and only
 * where it had them: a build without the file has no push, registers no
 * phone and is otherwise the same. Nothing starts it at the app's start
 * but this, and no token is made before the pilot is signed in.
 */
object PhonePush {
    /** Whether the build had the Firebase project's file. */
    val isConfigured: Boolean
        get() = BuildConfig.FIREBASE_APP_ID.isNotEmpty() && BuildConfig.FIREBASE_SENDER_ID.isNotEmpty()

    /** Whether Firebase was started in this run. */
    var isStarted: Boolean = false
        private set

    /** Starts Firebase where the build was given its project; nothing where not. */
    fun start(context: Context) {
        if (!isConfigured || isStarted) return
        isStarted = try {
            if (FirebaseApp.getApps(context).isEmpty()) {
                FirebaseApp.initializeApp(
                    context,
                    FirebaseOptions.Builder()
                        .setApplicationId(BuildConfig.FIREBASE_APP_ID)
                        .setApiKey(BuildConfig.FIREBASE_API_KEY)
                        .setProjectId(BuildConfig.FIREBASE_PROJECT_ID)
                        .setGcmSenderId(BuildConfig.FIREBASE_SENDER_ID)
                        .build(),
                )
            }
            true
        } catch (_: RuntimeException) {
            // A phone without Google's services: no push, the rest works.
            false
        }
    }

    /** This phone's token from Firebase, or `null` without push or without Google's services. */
    suspend fun token(): String? {
        if (!isStarted) return null
        return try {
            suspendCancellableCoroutine { asked ->
                FirebaseMessaging.getInstance().token.addOnCompleteListener { task ->
                    asked.resume(if (task.isSuccessful) task.result else null)
                }
            }
        } catch (error: CancellationException) {
            throw error
        } catch (_: RuntimeException) {
            null
        }
    }
}

/**
 * The phone's token on the pilot's account: registered when the pilot is
 * signed in (each time the app starts so, and whenever Firebase gives a
 * new one), removed before signing out. The token last registered is kept
 * in the app's preferences, to be removed again.
 */
class PushRegistration(context: Context, private val account: AccountModel) {
    private val kept = context.getSharedPreferences("venti-push", Context.MODE_PRIVATE)

    /** Registers this phone's token, or `token` where Firebase just gave one. Quiet. */
    suspend fun register(token: String? = null) {
        if (!PhonePush.isStarted || !account.isSignedIn) return
        val now = token ?: PhonePush.token() ?: return
        quietly {
            val answer = account.answer(deviceRegisterRequest(PhonePlatform.GOOGLE, now, false))
            phoneRequestDone(answer.status16, answer.body)
            kept.edit { putString(TOKEN, now) }
        }
    }

    /** Removes this phone from the account: sent before signing out, which removes it as well. */
    suspend fun remove() {
        val token = kept.getString(TOKEN, null) ?: return
        quietly {
            val answer = account.answer(deviceRemoveRequest(PhonePlatform.GOOGLE, token))
            phoneRequestDone(answer.status16, answer.body)
        }
        kept.edit { remove(TOKEN) }
    }

    private suspend fun quietly(send: suspend () -> Unit) {
        try {
            send()
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // Asked again the next time the app starts signed in.
        }
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    private companion object {
        const val TOKEN = "token"
    }
}

/**
 * What Venti's server pushes: a notification with a title and a line, and
 * what it is about (`kind`, `id`), which a tap opens. While the app is in
 * the back the system shows it by itself on the channel "Pilots nearby",
 * and a tap opens the app with `kind` and `id` among its extras; while the
 * app is in front it comes here and is shown the same way.
 */
class PushService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        val session = (application as VentiApplication).session as? Session.Ready ?: return
        session.scope.launch { session.push?.register(token) }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        // Any push from Venti: the Community tab's badge may have grown.
        ((application as VentiApplication).session as? Session.Ready)?.let { ready ->
            ready.scope.launch { ready.unread.read() }
        }
        val kind = message.data[PushNotifications.KIND] ?: return
        val id = message.data[PushNotifications.ID] ?: return
        val title = message.notification?.title ?: return
        PushNotifications.show(this, title, message.notification?.body.orEmpty(), kind, id)
        // A call for help or its answer: the lists are read again.
        val session = (application as VentiApplication).session as? Session.Ready ?: return
        if (kind.startsWith("help")) session.scope.launch { session.nearby.loadHelp() }
    }
}

/** A pushed message as a notification of the phone's, opening what it is about. */
object PushNotifications {
    /** The channel FCM shows on in the back too (the manifest names it). */
    const val CHANNEL = "nearby"

    /** The extras a tap opens the app with: the keys of the message's data, as FCM hands them over. */
    const val KIND = "kind"
    const val ID = "id"

    /** Makes the channel, so that a message arriving in the back has it. Before any is shown. */
    fun prepare(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, HelpWords.CHANNEL, NotificationManager.IMPORTANCE_HIGH).apply {
                description = HelpWords.CHANNEL_ABOUT
            },
        )
    }

    fun show(context: Context, title: String, line: String, kind: String, id: String) {
        if (!AlertNotifications.areAllowed(context)) return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        prepare(context)
        val open = PendingIntent.getActivity(
            context,
            "$kind-$id".hashCode(),
            Intent(context, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(KIND, kind)
                .putExtra(ID, id),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val notification = Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            .setContentText(line)
            .setStyle(Notification.BigTextStyle().bigText(line))
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        manager.notify("$kind-$id", 0, notification)
    }
}
