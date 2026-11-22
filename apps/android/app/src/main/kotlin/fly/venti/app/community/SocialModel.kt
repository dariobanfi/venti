package fly.venti.app.community

import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.account.AccountModel
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.CheerList
import fly.venti.core.CheerState
import fly.venti.core.CommentList
import fly.venti.core.CommentPosted
import fly.venti.core.CommentsSwitch
import fly.venti.core.CoreException
import fly.venti.core.Feed
import fly.venti.core.FollowList
import fly.venti.core.FollowState
import fly.venti.core.Notifications
import fly.venti.core.ReportReason
import fly.venti.core.ServerRequest
import fly.venti.core.SocialWords
import fly.venti.core.cheerAnswer
import fly.venti.core.cheerList
import fly.venti.core.cheerRequest
import fly.venti.core.cheersRequest
import fly.venti.core.commentList
import fly.venti.core.commentPosted
import fly.venti.core.commentRemoveRequest
import fly.venti.core.commentReportRequest
import fly.venti.core.commentRequest
import fly.venti.core.commentsOpenAnswer
import fly.venti.core.commentsOpenRequest
import fly.venti.core.commentsRequest
import fly.venti.core.feed
import fly.venti.core.feedRequest
import fly.venti.core.followAnswer
import fly.venti.core.followList
import fly.venti.core.followRequest
import fly.venti.core.followersRequest
import fly.venti.core.notifications
import fly.venti.core.notificationsRequest
import fly.venti.core.pilotsWords
import fly.venti.core.reportSent
import fly.venti.core.socialDone
import fly.venti.core.socialWords
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Pilots among each other: following a pilot, the flights of the pilots
 * one follows, cheers and comments under a public flight, and what the
 * pilot was told of them.
 *
 * No rule and no word of it is here. The core makes each request and reads
 * each answer into rows and words; this class sends what the core gives,
 * with the session's token where the request needs one and without it
 * where it does not, and keeps nothing: every screen asks when it shows.
 */
class SocialModel(
    private val account: AccountModel,
    /** Fetches an address without a token: what everyone may read. */
    private val transport: WindTransport = { url -> Http.get(url) },
    /**
     * The time now, in seconds since 1970: by it a comment is "3 h ago".
     * The app sets Venti's own clock, as the radar's ages are counted, so
     * that a phone whose clock is off says no wrong age.
     */
    private val now: suspend () -> Long = { System.currentTimeMillis() / 1000 },
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** Venti's address, without a slash at its end. The app sets this to the one the lists are read from. */
    var server: () -> String? = { null }

    /** The words no answer carries: titles, the field's placeholder, the actions' names. */
    val words: SocialWords by lazy { socialWords() }

    /** Whether the phone is signed in: without it no control that needs an account is shown. */
    val isSignedIn: Boolean get() = account.isSignedIn

    /** The signed-in pilot's handle. */
    val ownHandle: String? get() = account.account?.handle?.takeIf { isSignedIn }

    /** What a question came to: the core's reading, or why there is none, in words for the pilot. */
    sealed interface Told<out T> {
        data class Read<T>(val value: T) : Told<T>

        data class Refused(val words: String) : Told<Nothing>
    }

    /** Who follows a pilot and whom they follow, in counts, and whether the pilot of this phone does. */
    suspend fun follows(handle: String): Told<FollowList> = told {
        val answer = send(followersRequest(handle, null, isSignedIn))
        followList(answer.status16, answer.body)
    }

    /** Follows a pilot, or follows them no more. */
    suspend fun follow(handle: String, follow: Boolean): Told<FollowState> = told {
        val answer = send(followRequest(handle, follow))
        followAnswer(answer.status16, answer.body)
    }

    /** A page of the flights of the pilots one follows; `before` is the `next` of the page before. */
    suspend fun feed(before: String?): Told<Feed> = told {
        val answer = send(feedRequest(before))
        feed(answer.status16, answer.body)
    }

    /** How many cheered a flight, and whether the pilot of this phone did. */
    suspend fun cheers(flight: String): Told<CheerList> = told {
        val answer = send(cheersRequest(flight, null, isSignedIn))
        cheerList(answer.status16, answer.body)
    }

    /** Cheers a flight, or takes the cheer back. */
    suspend fun cheer(flight: String, cheer: Boolean): Told<CheerState> = told {
        val answer = send(cheerRequest(flight, cheer))
        cheerAnswer(answer.status16, answer.body)
    }

    /**
     * The comments under a flight, all of its pages: the core's list with
     * every page's comments in it. `flightHandle` is the handle of the
     * flight's pilot, who may remove what stands under it.
     */
    suspend fun comments(flight: String, flightHandle: String?): Told<CommentList> = told {
        var after: String? = null
        var all: CommentList? = null
        var pages = 0
        do {
            val answer = send(commentsRequest(flight, after))
            val page = commentList(answer.status16, answer.body, now(), ownHandle, flightHandle)
            all = all?.let { page.copy(comments = it.comments + page.comments) } ?: page
            after = page.next
            pages += 1
        } while (after != null && pages < PAGES_AT_MOST)
        checkNotNull(all)
    }

    /** Posts a comment under a flight. */
    suspend fun post(flight: String, text: String): Told<CommentPosted> = told {
        val answer = send(commentRequest(flight, text))
        commentPosted(answer.status16, answer.body, now())
    }

    /** Removes a comment: the pilot's own, or one under a flight of theirs. */
    suspend fun remove(flight: String, comment: String): Told<Unit> = told {
        val answer = send(commentRemoveRequest(flight, comment))
        socialDone(answer.status16, answer.body)
    }

    /** Reports a comment to whoever runs Venti; the answer's words are the core's. */
    suspend fun reportComment(flight: String, comment: String, reason: ReportReason, text: String): AccountModel.ReportOutcome =
        when (
            val told = told {
                val answer = send(commentReportRequest(flight, comment, reason, text))
                reportSent(answer.status16, answer.body)
            }
        ) {
            is Told.Read -> AccountModel.ReportOutcome(sent = told.value)
            is Told.Refused -> AccountModel.ReportOutcome(problem = told.words)
        }

    /** Says whether a flight of the pilot's own takes comments. */
    suspend fun commentsOpen(flight: String, open: Boolean): Told<CommentsSwitch> = told {
        val answer = send(commentsOpenRequest(flight, open))
        commentsOpenAnswer(answer.status16, answer.body)
    }

    /**
     * What the pilot was told, from the beginning: the phone keeps no
     * cursor, so each look reads all the server still has, the oldest
     * first, as the core hands it over.
     */
    suspend fun notifications(): Told<Notifications> = told {
        var since: String? = null
        var all: Notifications? = null
        var rounds = 0
        do {
            val answer = send(notificationsRequest(since))
            val read = notifications(answer.status16, answer.body, now())
            all = all?.takeUnless { read.reset }?.let { read.copy(lines = it.lines + read.lines) } ?: read
            since = read.cursor
            rounds += 1
        } while (read.more && rounds < PAGES_AT_MOST)
        checkNotNull(all)
    }

    /**
     * Sends one of the core's requests: with the session's token where it
     * needs one, and as anyone where it does not.
     */
    private suspend fun send(request: ServerRequest): HttpAnswer =
        if (request.needsToken) {
            account.answer(request)
        } else {
            val server = server() ?: throw Unreachable()
            transport(server + request.path)
        }

    private suspend fun <T> told(ask: suspend () -> T): Told<T> = try {
        Told.Read(withContext(background) { ask() })
    } catch (error: CancellationException) {
        throw error
    } catch (error: CoreException) {
        Told.Refused(error.pilotMessage)
    } catch (_: Unreachable) {
        Told.Refused(pilotsWords().unreachable)
    } catch (error: Exception) {
        Told.Refused(account.words(error))
    }

    /** Venti's address is not known. */
    private class Unreachable : Exception()

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    private companion object {
        /** How many pages of one list are read in a row. */
        const val PAGES_AT_MOST = 10
    }
}
