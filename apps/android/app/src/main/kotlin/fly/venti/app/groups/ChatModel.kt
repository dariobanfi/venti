package fly.venti.app.groups

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.core.ChatMessage
import fly.venti.core.ChatPage
import fly.venti.core.CommunityStanding
import fly.venti.core.Core
import fly.venti.core.LinkDraft
import fly.venti.core.MessageDraft
import fly.venti.core.SentGroupPicture
import fly.venti.core.ServerRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

/**
 * The messages of a chat as the screen shows them, the oldest first, and
 * where to read on from: the page before (`older`) and what is new
 * (`since`). Pages and what is new are laid in by their place in the chat
 * (`cursor`), each message once, as it was last read: a message removed or
 * a plan's card changed is shown as it is now. As the iPhone's `ChatLog`.
 */
data class ChatLog(
    val messages: List<ChatMessage> = emptyList(),
    /** What to hand the core for the page before; `null` at the first message, and before the first page. */
    val older: String? = null,
    /** What to hand the core for what is new; `null` before the first page. */
    val since: String? = null,
) {
    /** The newest page, which the chat opens on: what was there before goes. */
    fun takeNewest(page: ChatPage): ChatLog = ChatLog(lay(emptyList(), page.messages), page.older, page.since)

    /** The page before the oldest shown. */
    fun takeOlder(page: ChatPage): ChatLog = copy(messages = lay(messages, page.messages), older = page.older)

    /** What was posted or changed since the last answer. */
    fun takeNew(page: ChatPage): ChatLog = copy(messages = lay(messages, page.messages), since = page.since)

    /** One message, as posting or removing it answered. */
    fun take(message: ChatMessage): ChatLog = copy(messages = lay(messages, listOf(message)))

    companion object {
        /** A cursor's place in the chat: the server writes it as a number. */
        fun place(cursor: String?): ULong = cursor?.toULongOrNull() ?: 0u

        /** Lays messages in by their place, each once, as last read. */
        private fun lay(shown: List<ChatMessage>, given: List<ChatMessage>): List<ChatMessage> {
            val byId = LinkedHashMap<String, ChatMessage>()
            for (message in shown) byId[message.id] = message
            for (message in given) byId[message.id] = message
            return byId.values.sortedBy { place(it.cursor) }
        }
    }
}

/**
 * One group's chat while its screen is open: the newest page, older pages
 * as the pilot scrolls up, and what is new every five seconds
 * (`docs/api.md`, "Groups as communities with a chat": there is no
 * stream). It marks the chat read up to the newest message shown. The core
 * puts every request together and reads every answer; nothing is kept on
 * the phone. As the iPhone's `ChatModel`.
 */
class ChatModel(
    val group: String,
    private val core: Core,
    /** Sends a request with the session's token and returns the answer. */
    private val send: suspend (ServerRequest) -> HttpAnswer,
    private val background: CoroutineDispatcher,
    /** What went wrong, in words for the pilot. */
    private val words: (Exception) -> String,
) {
    var log: ChatLog by mutableStateOf(ChatLog())
        private set

    /** Whether the first page has come. */
    var hasLoaded: Boolean by mutableStateOf(false)
        private set

    var isLoadingOlder: Boolean by mutableStateOf(false)
        private set

    /** Why the chat could not be read, in words for the pilot; the messages read before stay. */
    var problem: String? by mutableStateOf(null)
        private set

    /** How the pilot stands to the group since the chat was last marked read. */
    var standing: CommunityStanding? by mutableStateOf(null)
        private set

    /** The place up to which the chat was marked read. */
    private var markedUpTo: ULong = 0u
    private var toMark: String? = null
    private var isMarking = false

    /**
     * The first page, then what is new every five seconds until the
     * coroutine that runs it is cancelled: the screen's, when it goes. No
     * other loop asks.
     */
    suspend fun run() {
        loadNewest()
        while (true) {
            delay(POLL_SECONDS * 1000L)
            loadNew()
        }
    }

    /** The newest page. */
    suspend fun loadNewest() {
        try {
            val page = ask({ core.messagesRequest(group, null) }) { status, body -> core.chatPage(status, body) }
            log = log.takeNewest(page)
            hasLoaded = true
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = words(error)
        }
    }

    /** The page before the oldest shown, where there is one. */
    suspend fun loadOlder() {
        val before = log.older ?: return
        if (isLoadingOlder) return
        isLoadingOlder = true
        try {
            val page = ask({ core.messagesRequest(group, before) }) { status, body -> core.chatPage(status, body) }
            log = log.takeOlder(page)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = words(error)
        } finally {
            isLoadingOlder = false
        }
    }

    /** What was posted or changed since the last answer; again at once while the server says there is more. */
    suspend fun loadNew() {
        var from = log.since ?: return loadNewest()
        repeat(MOST_AT_ONCE) {
            try {
                val page = ask({ core.messagesSinceRequest(group, from) }) { status, body -> core.chatPage(status, body) }
                log = log.takeNew(page)
                problem = null
                if (!page.more) return
                from = page.since
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                problem = words(error)
                return
            }
        }
    }

    /** Posts a message: its text, the pictures sent for it and the links chosen; the core finds Venti's links in the text. */
    suspend fun post(text: String, pictures: List<String>, links: List<LinkDraft> = emptyList()) {
        val draft = MessageDraft(text, pictures, links)
        val message = ask({ core.postMessageRequest(group, draft) }) { status, body -> core.chatMessage(status, body) }
        log = log.take(message)
        // Posting moves the pilot's own mark.
        markedUpTo = maxOf(markedUpTo, ChatLog.place(message.cursor))
    }

    /** Sends a picture for a message: its bytes and media type. */
    suspend fun sendPicture(picture: Pair<ByteArray, String>): SentGroupPicture =
        ask({ core.sendGroupPictureRequest(group, picture.first, picture.second) }) { status, body ->
            core.groupPictureSent(status, body)
        }

    /** Removes a message: the pilot's own, or as owner or admin anyone's. */
    suspend fun remove(message: ChatMessage) {
        val removed = ask({ core.removeMessageRequest(group, message.id) }) { status, body -> core.chatMessage(status, body) }
        log = log.take(removed)
    }

    /**
     * The pilot was shown a message: the chat is marked read up to it, one
     * request at a time and only forward. Called on the screen's thread.
     */
    suspend fun saw(message: ChatMessage) {
        val place = ChatLog.place(message.cursor)
        if (place <= markedUpTo || place <= ChatLog.place(toMark)) return
        toMark = message.cursor
        if (isMarking) return
        isMarking = true
        try {
            while (true) {
                val cursor = toMark ?: break
                toMark = null
                val at = ChatLog.place(cursor)
                if (at <= markedUpTo) continue
                try {
                    standing = ask({ core.groupReadRequest(group, cursor) }) { status, body -> core.groupStanding(status, body) }
                    markedUpTo = maxOf(markedUpTo, at)
                } catch (error: CancellationException) {
                    throw error
                } catch (_: Exception) {
                    // Marked again with the next message shown.
                }
            }
        } finally {
            isMarking = false
        }
    }

    private suspend fun <Value> ask(request: () -> ServerRequest, read: (UShort, ByteArray) -> Value): Value {
        val answer = send(withContext(background) { request() })
        return withContext(background) { read(answer.status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort(), answer.body) }
    }

    companion object {
        /** How long the chat waits between two questions of what is new (`docs/api.md`). */
        const val POLL_SECONDS = 5L

        /** How many answers with `more` are followed at once before the next wait. */
        private const val MOST_AT_ONCE = 10
    }
}
