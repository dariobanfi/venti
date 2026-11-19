package fly.venti.app.account

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.core.AccountView
import fly.venti.core.Core
import fly.venti.core.CoreException
import fly.venti.core.FlightSharing
import fly.venti.core.FlightVisibility
import fly.venti.core.ServerRequest
import fly.venti.core.SyncStep
import fly.venti.core.SyncSummary
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Sends one request, with these headers and this body, and returns the server's answer. */
typealias AccountTransport = suspend (
    method: String,
    url: String,
    headers: List<Pair<String, String>>,
    body: ByteArray?,
) -> HttpAnswer

/**
 * The pilot's account on a Venti server, as the screens see it.
 *
 * What to ask the server and what its answers mean is the core's. This
 * class sends the requests, keeps the session token where the phone keeps
 * secrets and runs the sync that keeps the logbook and the server in step.
 *
 * Its calls are made from the app's own scope, not a screen's: a sync or a
 * sign-in carries on when the pilot has moved to another screen.
 */
class AccountModel(
    private val core: Core,
    private val session: SessionStore,
    /** What this phone is called in the account's list of devices. */
    private val device: String?,
    private val transport: AccountTransport = overHttp,
    /** This device's clock, in milliseconds since 1970. */
    private val clock: () -> Long = System::currentTimeMillis,
    /** Where the core reads and writes, and the token is sealed, away from the screen. */
    private val background: CoroutineDispatcher = Dispatchers.IO,
) {
    /** The account this phone is signed in to. */
    var account: AccountView? by mutableStateOf(runCatching { core.account() }.getOrNull())
        private set

    /** Whether a request or a sync is under way. */
    var isWorking: Boolean by mutableStateOf(false)
        private set

    /** What the last sync did, for the account screen. */
    var lastSync: SyncSummary? by mutableStateOf(null)
        private set

    /** Something the pilot must be told. */
    var problem: String? by mutableStateOf(null)

    /**
     * Whether the phone knows the account but has no token for it: after
     * the app's files were moved to a new phone, where the old phone's
     * keystore did not follow.
     */
    private var isSessionLost: Boolean by mutableStateOf(account != null && session.read() == null)

    /** Called after a sync changed the logbook, so its screens can catch up. */
    var onLogbookChanged: () -> Unit = {}

    private var lastAutomaticSync: Long? = null

    /**
     * Set when a sync was asked for while one was under way: the logbook
     * changed again, and another run follows the present one.
     */
    private var isSyncDue = false

    /** Whether the session has ended, by the server's word or because its token is gone. */
    val needsSignIn: Boolean get() = account?.needsSignIn == true || isSessionLost

    /** Whether the pilot is signed in and the session still holds. */
    val isSignedIn: Boolean get() = account != null && !needsSignIn

    // Signing in and out

    /** Signs in on the given server; `server` has no slash at its end. */
    suspend fun signIn(server: String, handle: String, password: String) {
        enter(server) { core.signInRequest(handle, password, device) }
    }

    /** Creates an account on the given server and signs in. */
    suspend fun createAccount(server: String, handle: String, displayName: String, password: String, invite: String) {
        enter(server) { core.createAccountRequest(handle, displayName, password, invite, device) }
    }

    private suspend fun enter(server: String, request: () -> ServerRequest) {
        if (isWorking) return
        isWorking = true
        try {
            val signedIn = try {
                val answer = ask(withContext(background) { request() }, server)
                withContext(background) { core.signedIn(server, answer.status16, answer.body) }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                problem = words(error)
                return
            }
            // Without the token every later request would be refused. If
            // the keystore did not keep it, say that, and do not leave a
            // session on the server that this phone cannot use.
            val isKept = withContext(background) { session.save(signedIn.token) && session.read() == signedIn.token }
            if (!isKept) {
                val signOut = withContext(background) { runCatching { core.signOutRequest() }.getOrNull() }
                if (signOut != null) {
                    quietly { ask(signOut, server, token = signedIn.token) }
                }
                withContext(background) {
                    runCatching { core.signedOut() }
                    session.remove()
                }
                problem = "This phone could not keep the session in its keystore, so you are not signed in. " +
                    "Restart the phone and try again."
                return
            }
            isSessionLost = false
            account = signedIn.account
            runSync()
        } finally {
            isWorking = false
        }
    }

    /** Signs this phone out. The flights in the logbook stay. */
    suspend fun signOut() {
        if (isWorking) return
        isWorking = true
        try {
            // The server is told if it can be reached; the phone forgets
            // the session either way.
            val server = account?.server
            val request = withContext(background) { runCatching { core.signOutRequest() }.getOrNull() }
            if (server != null && request != null) {
                quietly { ask(request, server) }
            }
            forget()
        } finally {
            isWorking = false
        }
    }

    /**
     * Deletes the account on the server, with its flights there. The
     * logbook on this phone stays. Returns whether it is gone.
     */
    suspend fun deleteAccount(password: String): Boolean {
        val server = account?.server ?: return false
        if (isWorking) return false
        isWorking = true
        try {
            val answer = ask(withContext(background) { core.deleteAccountRequest(password) }, server)
            withContext(background) { core.accountDeleted(answer.status16, answer.body) }
            forget()
            return true
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = words(error)
            return false
        } finally {
            isWorking = false
        }
    }

    /** Forgets the account and its token on this phone. */
    private suspend fun forget() {
        withContext(background) {
            runCatching { core.signedOut() }
            session.remove()
        }
        account = null
        lastSync = null
        isSessionLost = false
    }

    // Sync

    /**
     * Brings the logbook and the server in step, if the pilot is signed
     * in. Called when the app comes to the front and after the logbook
     * changed. `force` skips the pause between runs. A run the pilot did
     * not ask for is quiet: being offline after a flight is no news.
     */
    suspend fun sync(force: Boolean = false, quietly: Boolean = true) {
        if (!isSignedIn) return
        if (isWorking) {
            // An import of many flights changes the logbook again and
            // again: what the present run does not take, the next one does.
            if (force) isSyncDue = true
            return
        }
        val last = lastAutomaticSync
        if (!force && last != null && clock() - last < PAUSE_MILLISECONDS) return
        isWorking = true
        try {
            do {
                isSyncDue = false
                runSync(reportsProblems = !quietly)
            } while (isSyncDue && isSignedIn)
        } finally {
            isWorking = false
        }
    }

    private suspend fun runSync(reportsProblems: Boolean = true) {
        val server = account?.server ?: return
        lastAutomaticSync = clock()
        val run = core.sync()
        var step = withContext(background) { run.start(clock() / 1000) }
        while (true) {
            val request = (step as? SyncStep.Send)?.request ?: break
            step = try {
                val answer = ask(request, server)
                withContext(background) { run.receive(answer.status16, answer.body, answer.retryAfterSeconds) }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                // What was done so far is kept; the next sync goes on.
                if (reportsProblems) problem = words(error)
                break
            }
        }
        when (val end = step) {
            is SyncStep.Finished -> lastSync = end.summary
            is SyncStep.Failed -> {
                if (end.needsSignIn) {
                    withContext(background) { session.remove() }
                }
                if (reportsProblems || end.needsSignIn) problem = end.message
            }
            is SyncStep.Send -> Unit
        }
        account = withContext(background) { runCatching { core.account() }.getOrNull() }
        onLogbookChanged()
    }

    // Sharing

    /** How a flight is shared, if the server has it. */
    fun sharing(flight: String): FlightSharing? = runCatching { core.flightSharing(flight) }.getOrNull()

    /** Changes who may see a flight, and returns how it is shared now. */
    suspend fun share(flight: String, visibility: FlightVisibility): FlightSharing? {
        val server = account?.server ?: return null
        if (isWorking) return null
        isWorking = true
        try {
            val answer = ask(withContext(background) { core.shareRequest(flight, visibility) }, server)
            return withContext(background) { core.shared(flight, answer.status16, answer.body) }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = words(error)
            return null
        } finally {
            isWorking = false
        }
    }

    // Requests

    /**
     * Sends one of the core's requests to the server, with the session
     * token where the request needs it and nowhere else. `token` is given
     * for the one request that is made before the token is kept.
     */
    private suspend fun ask(request: ServerRequest, server: String, token: String? = null): HttpAnswer {
        val headers = mutableListOf("Accept" to "application/json")
        request.contentType?.let { headers += "Content-Type" to it }
        if (request.needsToken) {
            val bearer = token ?: withContext(background) { session.read() } ?: throw NotSignedIn()
            headers += "Authorization" to "Bearer $bearer"
        }
        return transport(request.method, server + request.path, headers, request.body)
    }

    /** Sends something whose answer changes nothing: a failure is no news. */
    private suspend fun quietly(send: suspend () -> Unit) {
        try {
            send()
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // The phone goes on as if the server had heard.
        }
    }

    /** The phone has no token to send with a request that needs one. */
    private class NotSignedIn : Exception()

    private fun words(error: Exception): String = when (error) {
        is CoreException -> error.pilotMessage
        is NotSignedIn -> "This phone is no longer signed in. Sign in again."
        else -> "The server could not be reached. ${error.pilotMessage}"
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** How long a request may take: a flight's file is sent in one. */
        private const val ANSWER_WITHIN_MILLISECONDS = 120_000

        /** How long after a sync the next one that nobody asked for waits. */
        const val PAUSE_MILLISECONDS = 60_000L

        /**
         * Sends with the app's own networking, which keeps nothing: answers
         * about an account are not for a cache, and there are no cookies.
         */
        val overHttp: AccountTransport = { method, url, headers, body ->
            Http.send(method, url, headers, body, ANSWER_WITHIN_MILLISECONDS)
        }
    }
}

/** The setting in words, for a list to choose from. */
val FlightVisibility.title: String
    get() = when (this) {
        FlightVisibility.PRIVATE -> "Only me"
        FlightVisibility.UNLISTED -> "Anyone with the link"
        FlightVisibility.PUBLIC -> "Everyone"
    }

/** A name for the setting that tests find its row by. */
val FlightVisibility.key: String get() = name.lowercase()

/** What the setting means. */
val FlightVisibility.explanation: String
    get() = when (this) {
        FlightVisibility.PRIVATE -> "The flight is kept on the server for your devices. Nobody else can see it."
        FlightVisibility.UNLISTED -> "Whoever has the link can see the flight. It is listed nowhere."
        FlightVisibility.PUBLIC -> "Anyone can see the flight, and it is listed on your page."
    }
