package fly.venti.app.nearby

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.app.account.AccountModel
import fly.venti.core.Coordinate
import fly.venti.core.HelpLists
import fly.venti.core.HelpView
import fly.venti.core.NearbyChoice
import fly.venti.core.ServerRequest
import fly.venti.core.helpAnswerRequest
import fly.venti.core.helpAskRequest
import fly.venti.core.helpCloseRequest
import fly.venti.core.helpListRequest
import fly.venti.core.helpOneRequest
import fly.venti.core.helpRequest
import fly.venti.core.helpRequests
import fly.venti.core.nearbySettings
import fly.venti.core.nearbySettingsChangeRequest
import fly.venti.core.nearbySettingsRequest
import fly.venti.core.phoneRequestDone
import fly.venti.core.positionForgetRequest
import fly.venti.core.positionRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * What pilots nearby do (docs/design.md, "Push to the phone and what
 * pilots nearby do" and "Help after landing"): the account's two settings,
 * the coarse position told while the pilot may be asked for help, and the
 * requests for help, the pilot's own and those they were asked.
 *
 * Every request and every reading of an answer is the core's
 * (`crates/core/src/phones.rs`); each goes with the session's token
 * (`AccountModel.answer`). Nothing of it is kept on the phone.
 */
class NearbyModel(
    private val account: AccountModel,
    /**
     * Venti's clock in milliseconds since 1970, for whether a request still
     * stands: the development server's clock stands still, and a request
     * there ends four hours after it, by its own clock. 0 while unread,
     * which the core takes for "stands".
     */
    private val clockMs: suspend () -> Long = { System.currentTimeMillis() },
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** The account's settings, once read; `null` before, and signed out. */
    var settings: NearbyChoice? by mutableStateOf(null)
        private set

    /** Why the settings could not be read or kept, in words for the pilot. */
    var settingsProblem: String? by mutableStateOf(null)
        private set

    /** Whether a change of the settings is under way. */
    var isSaving: Boolean by mutableStateOf(false)
        private set

    /** The requests for help that stand, once read. */
    var help: HelpLists? by mutableStateOf(null)
        private set

    /** Why they could not be read, in words for the pilot. */
    var helpProblem: String? by mutableStateOf(null)
        private set

    /**
     * The note of a call for help being written: kept here, not in the
     * sheet, so that it outlasts the sheet closed by mistake and the sheet
     * drawn anew; emptied once the call went out.
     */
    var draftNote: String by mutableStateOf("")

    /** Counts what changed a request, for a page that shows one. */
    var revision: Int by mutableIntStateOf(0)
        private set

    val isSignedIn: Boolean get() = account.isSignedIn

    /** Whether the pilot said that pilots nearby may ask them for help. */
    val mayBeAsked: Boolean get() = isSignedIn && settings?.mayBeAskedForHelp == true

    /** Reads the account's settings. Signed out there are none. */
    suspend fun loadSettings() {
        if (!isSignedIn) {
            settings = null
            return
        }
        settingsProblem = null
        try {
            settings = send({ nearbySettingsRequest() }) { status, body -> nearbySettings(status, body) }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            settingsProblem = account.words(error)
        }
    }

    /**
     * Sets both settings, whole, and keeps what the server answered.
     * Returns what went wrong, in words, or `null`.
     */
    suspend fun change(choice: NearbyChoice): String? {
        isSaving = true
        settingsProblem = null
        return try {
            settings = send({ nearbySettingsChangeRequest(choice) }) { status, body -> nearbySettings(status, body) }
            null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            account.words(error).also { settingsProblem = it }
        } finally {
            isSaving = false
        }
    }

    /**
     * Tells Venti roughly where the phone is, for being asked for help;
     * only while the pilot said they may be. Quiet: a failure is no news,
     * the next one follows.
     */
    suspend fun tellPosition(at: Coordinate) {
        if (!mayBeAsked) return
        quietly { send({ positionRequest(at) }) { status, body -> phoneRequestDone(status, body) } }
    }

    /** Has Venti forget the phone's position: before signing out. */
    suspend fun forgetPosition() {
        if (!mayBeAsked) return
        quietly { send({ positionForgetRequest() }) { status, body -> phoneRequestDone(status, body) } }
    }

    /** Forgets what was read for the account: after signing out. */
    fun signedOut() {
        settings = null
        help = null
        helpProblem = null
        settingsProblem = null
    }

    /** Reads the requests that stand: the pilot's own and those they were asked. */
    suspend fun loadHelp() {
        if (!isSignedIn) {
            help = null
            return
        }
        helpProblem = null
        try {
            val now = clockMs()
            help = send({ helpListRequest() }) { status, body -> helpRequests(status, body, now) }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            helpProblem = account.words(error)
        }
    }

    /** Asks the pilots nearby for help, from where the pilot is, with their note (which may be empty). */
    suspend fun ask(at: Coordinate, note: String): HelpView {
        val now = clockMs()
        val asked = send({ helpAskRequest(at, note) }) { status, body -> helpRequest(status, body, now) }
        draftNote = ""
        return wrote(asked)
    }

    /** One request, as the pilot may see it. */
    suspend fun one(id: String): HelpView {
        val now = clockMs()
        return send({ helpOneRequest(id) }) { status, body -> helpRequest(status, body, now) }
    }

    /** Closes the pilot's own request: "I'm fine, close". */
    suspend fun close(id: String) {
        send({ helpCloseRequest(id) }) { status, body -> phoneRequestDone(status, body) }
        revision += 1
        loadHelp()
    }

    /** Says whether the pilot can help, on a request they were asked. */
    suspend fun answer(id: String, canHelp: Boolean): HelpView {
        val now = clockMs()
        return wrote(send({ helpAnswerRequest(id, canHelp) }) { status, body -> helpRequest(status, body, now) })
    }

    /**
     * Does one thing on the server and returns what went wrong, in words
     * for the pilot: the core's refusal in its own words; `null` when
     * nothing did.
     */
    suspend fun attempt(work: suspend () -> Unit): String? = try {
        work()
        null
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        account.words(error)
    }

    private suspend fun wrote(view: HelpView): HelpView {
        revision += 1
        loadHelp()
        return view
    }

    private suspend fun quietly(send: suspend () -> Unit) {
        try {
            send()
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // The next one is due in a few minutes.
        }
    }

    /** Puts a request together, sends it with the account's token, and reads the answer. */
    private suspend fun <Value> send(request: () -> ServerRequest, read: (UShort, ByteArray) -> Value): Value {
        val asked = withContext(background) { request() }
        val answer = account.answer(asked)
        return withContext(background) { read(answer.status16, answer.body) }
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
}

/** How often the position is told while the pilot may be asked for help. */
object NearbyTiming {
    /** Every ten minutes while the app is in front (docs/design.md). */
    const val POSITION_EVERY_MILLISECONDS = 10 * 60 * 1000L

    /** How long a fix is waited for before the last one known is taken. */
    const val POSITION_WAIT_MILLISECONDS = 5_000L
}
