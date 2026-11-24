package fly.venti.app.community

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.core.ServerRequest
import fly.venti.core.Unread
import fly.venti.core.unread
import fly.venti.core.unreadRequest
import fly.venti.core.unreadSeenRequest
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * How much is new for the pilot: the notifications and the lines of what
 * the alarms fired after the account's mark of what the pilot saw, the
 * same on every device (`docs/api.md`, `GET /v1/account/unread`), and the
 * groups' messages beside it. The Community tab shows [badge], TalkBack
 * its [label].
 *
 * No count is kept on the phone: the server keeps the mark, and this reads
 * it when the app comes to the front, after signing in and out, when a
 * push arrives, and moves it when the pilot opened the notifications or
 * the alerts. The words are the core's (`social.rs`, `unread`); as on the
 * iPhone (`UnreadModel.swift`).
 */
class UnreadModel(
    /** Sends a request with the session's token and returns the answer. */
    private val send: suspend (ServerRequest) -> HttpAnswer,
    private val isSignedIn: () -> Boolean,
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /**
     * The last count Venti gave; `null` signed out and before the first
     * answer. Kept as it was when a read fails: a badge is no promise.
     */
    var unread: Unread? by mutableStateOf(null)
        private set

    /**
     * What the badge says: "3", "99+"; `null` for none. The groups' unread
     * messages are a marker beside the notifications' (`groups`, not in
     * `total`), and the tab has one badge: the two together.
     */
    val badge: String?
        get() {
            val unread = unread ?: return null
            if (unread.groups == 0u) return unread.badge
            return badge(unread.total + unread.groups)
        }

    /** What TalkBack says of it: "3 new". */
    val label: String?
        get() {
            val unread = unread ?: return null
            if (unread.groups == 0u) return unread.label
            return badge?.let { "$it new" }
        }

    /** Reads the count anew; signed out there is none. */
    suspend fun read() {
        ask(unreadRequest())
    }

    /** The pilot was shown the notifications up to `cursor`, the cursor of the last answer the list read. */
    suspend fun sawNotifications(upTo: String?) {
        seen(notificationsCursor = upTo, alarmsCursor = NOT_SHOWN)
    }

    /**
     * The pilot was shown what the alarms fired: the list is everything the
     * last sync brought, so the mark goes up to the newest.
     */
    suspend fun sawAlarms() {
        seen(notificationsCursor = NOT_SHOWN, alarmsCursor = null)
    }

    /** Moves the mark, then takes the count the answer gives; where the mark could not be moved, reads it anew. */
    private suspend fun seen(notificationsCursor: String?, alarmsCursor: String?) {
        if (!isSignedIn()) {
            unread = null
            return
        }
        val request = runCatching { unreadSeenRequest(notificationsCursor, alarmsCursor) }.getOrNull()
        if (request == null || !ask(request)) read()
    }

    /** Sends a request whose answer the core's `unread` reads. Whether an answer was read. */
    private suspend fun ask(request: ServerRequest): Boolean {
        if (!isSignedIn()) {
            unread = null
            return true
        }
        return try {
            val answer = send(request)
            val read = withContext(background) {
                unread(answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), answer.body)
            }
            // Signed out while it was asked: nothing is shown.
            unread = if (isSignedIn()) read else null
            true
        } catch (e: Exception) {
            if (e is kotlinx.coroutines.CancellationException) throw e
            if (!isSignedIn()) unread = null
            false
        }
    }

    companion object {
        /**
         * The cursor that moves no mark: the server's marks only move
         * forward, so a list named with it is left as it was
         * (`crates/server/src/api/unread.rs`, `seen`). A list the request
         * leaves out is seen up to its newest line, which is not what the
         * pilot saw when they opened the other list.
         */
        const val NOT_SHOWN = "0"

        /** The core's form of a badge: the count, "99+" over 99, none for 0. */
        fun badge(count: UInt): String? = when {
            count == 0u -> null
            count <= 99u -> count.toString()
            else -> "99+"
        }
    }
}
