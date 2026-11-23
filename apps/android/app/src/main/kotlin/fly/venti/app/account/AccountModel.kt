package fly.venti.app.account

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.pilotMessage
import fly.venti.core.AccountView
import fly.venti.core.AllFlightsSet
import fly.venti.core.Core
import fly.venti.core.CoreException
import fly.venti.core.FiredLine
import fly.venti.core.FlightSharing
import fly.venti.core.FlightVisibility
import fly.venti.core.GliderClassKind
import fly.venti.core.GoogleSignUp
import fly.venti.core.ReportReason
import fly.venti.core.ReportSubject
import fly.venti.core.ServerRequest
import fly.venti.core.SignedIn
import fly.venti.core.SyncStep
import fly.venti.core.SyncSummary
import fly.venti.core.reportRequest
import fly.venti.core.reportSent
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.util.TimeZone

/**
 * Sends one request, with these headers and this body, waits so many
 * milliseconds for the server's answer and returns it.
 */
typealias AccountTransport = suspend (
    method: String,
    url: String,
    headers: List<Pair<String, String>>,
    body: ByteArray?,
    answerWithinMilliseconds: Int,
) -> HttpAnswer

/**
 * The pilot's account on a Venti server, as the screens see it.
 *
 * What to ask the server and what its answers mean is the core's. This
 * class sends the requests, keeps the session token where the phone keeps
 * secrets and runs the syncs that keep the logbook and the pilot's own
 * take-offs in step with the server.
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

    /**
     * Called after the pilot's own take-offs were brought in step with the
     * account, so that what lists them reads them again: the list may have
     * grown or shrunk, and it says anew where it is kept.
     */
    var onKeptSitesChanged: () -> Unit = {}

    /**
     * Called after the account's alarms and what they fired were fetched,
     * with what had fired since the run before, in the core's lines: the
     * screens read the list again, and each new line becomes a
     * notification.
     */
    var onAlertsFetched: (List<FiredLine>) -> Unit = {}

    /**
     * Called before this phone signs out, while the token still holds:
     * the phone's push token is removed from the account and the position
     * told for help forgotten. A failure there does not stop signing out.
     */
    var onSigningOut: suspend () -> Unit = {}

    /** One run of fetching the alarms at a time, or two would both call the same line new. */
    private val alertsRun = Mutex()

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
            keep(server, signedIn)
        } finally {
            isWorking = false
        }
    }

    // Signing in with Google

    /** Where the app gets Google's ID tokens: Google itself, or in a UI test the development server. */
    var google: GoogleIdTokens? = null

    /** Asks [google] for an ID token over the screen of `context`, for Venti at `server`. */
    fun googleIdTokens(context: Context, server: String): suspend () -> String = {
        (google ?: throw IllegalStateException("Signing in with Google is not set up in this build.")).idToken(context, server)
    }

    /**
     * A first sign-in with a Google account no Venti account is linked to:
     * the handle step waits for [createGoogleAccount], on this server.
     */
    var googleSignUp: Pair<String, GoogleSignUp>? by mutableStateOf(null)
        private set

    /**
     * "Continue with Google": asks Google for an ID token and signs in with
     * it, or goes on to the handle step for a Google account Venti does not
     * know yet. `idToken` asks Google ([googleIdTokens]). Closing Google's
     * sheet is no problem to report.
     */
    suspend fun signInWithGoogle(server: String, idToken: suspend () -> String) {
        if (isWorking) return
        isWorking = true
        try {
            val result = try {
                val token = fromGoogle(idToken) ?: return
                val answer = ask(withContext(background) { core.googleSignInRequest(token, device) }, server)
                withContext(background) { core.googleSignIn(server, answer.status16, answer.body) }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                problem = words(error)
                return
            }
            result.needsHandle?.let { googleSignUp = server to it }
            result.signedIn?.let { keep(server, it) }
        } finally {
            isWorking = false
        }
    }

    /** "Create account" of the handle step: the account is made, linked to the Google account, and signed in. */
    suspend fun createGoogleAccount(handle: String, displayName: String, invite: String) {
        val (server, signUp) = googleSignUp ?: return
        enter(server) {
            core.googleAccountRequest(signUp.signup, handle, displayName, invite.takeIf { it.isNotBlank() }, device)
        }
        if (isSignedIn) googleSignUp = null
    }

    /** Leaves the handle step without an account. */
    fun cancelGoogleSignUp() {
        googleSignUp = null
    }

    /** "Link Google": the account signs in with the Google account the pilot chooses from now on too. */
    suspend fun linkGoogle(idToken: suspend () -> String) {
        val server = account?.server ?: return
        changeGoogle {
            val token = fromGoogle(idToken) ?: return@changeGoogle null
            val answer = ask(withContext(background) { core.linkGoogleRequest(token) }, server)
            withContext(background) { core.googleLinked(answer.status16, answer.body) }
        }
    }

    /** "Unlink Google". The server refuses it, in the core's words, for an account Google is the only way into. */
    suspend fun unlinkGoogle() {
        val server = account?.server ?: return
        changeGoogle {
            val answer = ask(withContext(background) { core.unlinkGoogleRequest() }, server)
            withContext(background) { core.googleUnlinked(answer.status16, answer.body) }
        }
    }

    private suspend fun changeGoogle(change: suspend () -> AccountView?) {
        if (isWorking) return
        isWorking = true
        try {
            change()?.let { account = it }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = words(error)
        } finally {
            isWorking = false
        }
    }

    /** An ID token from Google, or `null` when the pilot closed Google's sheet. */
    private suspend fun fromGoogle(idToken: suspend () -> String): String? = try {
        idToken()
    } catch (_: GoogleCancelled) {
        null
    }

    /** Keeps the session of a sign-in, then brings the logbook in step. */
    private suspend fun keep(server: String, signedIn: SignedIn) {
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
        // Uploads carry the class of the glider the app is set to.
        mirrorGliderClass()
        runSync()
    }

    /** Signs this phone out. The flights in the logbook stay. */
    suspend fun signOut() {
        if (isWorking) return
        isWorking = true
        try {
            // The server is told if it can be reached; the phone forgets
            // the session either way.
            val server = account?.server
            if (isSignedIn) quietly { onSigningOut() }
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

    /**
     * Deletes the account with a fresh sign-in with its Google account in
     * place of the password: for an account made with Google, which has
     * none. Returns whether it is gone.
     */
    suspend fun deleteAccountWithGoogle(idToken: suspend () -> String): Boolean {
        val server = account?.server ?: return false
        if (isWorking) return false
        isWorking = true
        try {
            val token = fromGoogle(idToken) ?: return false
            val answer = ask(withContext(background) { core.deleteAccountWithGoogleRequest(token) }, server)
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
        allFlights = null
        isSessionLost = false
    }

    /**
     * Reads the account from the server, where the phone is signed in:
     * what the server says of it can change while the phone stays signed
     * in, its name and whether it is one of Venti's partners for flying
     * sites. Asked when the app starts and when the account's page opens.
     * It is quiet: without an answer the account stays as it is known.
     */
    suspend fun refresh() {
        val server = account?.server ?: return
        try {
            val request = withContext(background) { core.accountRequest() } ?: return
            val answer = ask(request, server, answerWithinMilliseconds = ANSWER_TO_OTHERS_WITHIN_MILLISECONDS)
            val read = withContext(background) { core.accountRead(answer.status16, answer.body) }
            // The server no longer knows the session: its token is of no use.
            if (read?.needsSignIn == true) withContext(background) { session.remove() }
            account = read
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // What the phone knows of the account stays.
            return
        }
        if (isSignedIn) mirrorGliderClass()
    }

    // Who sees the pilot's flights, and what is said with them

    /**
     * The class of the glider the pilot flies, as the app's own glider
     * setting has it ("Glider and cockpit"). The account is told it at
     * sign-in, when the account is read, and whenever it changes
     * ([gliderClassChanged]), so that a flight sent to the server carries it.
     */
    var gliderClass: () -> GliderClassKind? = { null }

    /** How the account's flights stood after the last "Make all public" or "Make all private". */
    var allFlights: AllFlightsSet? by mutableStateOf(null)
        private set

    /** "New flights are public": whether a flight sent from now on is public from the start. */
    suspend fun setNewFlightsPublic(public: Boolean) {
        changeAccount { core.newFlightsPublicRequest(public) }
    }

    /** The pilot's country, an ISO 3166-1 alpha-2 code, or `null` to say none. */
    suspend fun setCountry(country: String?) {
        changeAccount { core.countryRequest(country) }
    }

    /**
     * Sends a picture as the pilot's own: its bytes and media type, made
     * ready by [PictureFile]. The core refuses one the server would refuse,
     * before it is sent; the refusal is the [problem].
     */
    suspend fun setPicture(picture: ByteArray, mediaType: String) {
        changeAccount { core.pictureRequest(picture, mediaType) }
    }

    /** Removes the pilot's picture: the initials show in its place. */
    suspend fun removePicture() {
        changeAccount { core.pictureRemovalRequest() }
    }

    /** "Show me among pilots near a place": whether the pilot is in those lists. */
    suspend fun setShownNearPlaces(shown: Boolean) {
        changeAccount { core.shownNearPlacesRequest(shown) }
    }

    /**
     * The app's glider setting changed: the account takes the class, quietly.
     * A phone without an answer tells it again at the next read of the account.
     */
    suspend fun gliderClassChanged(kind: GliderClassKind?) {
        if (!isSignedIn || account?.gliderClass == kind) return
        sendGliderClass(kind)
    }

    /**
     * Tells the account the glider's class where the phone has one set and
     * the account says another. A phone without a class set leaves the
     * account's as it is: the pilot may have set it on another device.
     */
    private suspend fun mirrorGliderClass() {
        val kind = gliderClass() ?: return
        if (account?.gliderClass == kind) return
        sendGliderClass(kind)
    }

    private suspend fun sendGliderClass(kind: GliderClassKind?) {
        val server = account?.server ?: return
        quietly {
            val answer = ask(withContext(background) { core.gliderClassRequest(kind) }, server, answerWithinMilliseconds = ANSWER_TO_OTHERS_WITHIN_MILLISECONDS)
            withContext(background) { core.accountRead(answer.status16, answer.body) }?.let { account = it }
        }
    }

    /**
     * "Make all public" or "Make all private", after the pilot answered the
     * core's question: every flight the server has, at once. Then a sync
     * brings each changed flight as it is. The answer's line stands in
     * [allFlights]; a refusal is the [problem].
     */
    suspend fun setAllFlights(visibility: FlightVisibility) {
        val server = account?.server ?: return
        if (isWorking) return
        isWorking = true
        try {
            val answer = ask(withContext(background) { core.allFlightsRequest(visibility) }, server, answerWithinMilliseconds = ANSWER_TO_OTHERS_WITHIN_MILLISECONDS)
            allFlights = withContext(background) { core.allFlightsSet(answer.status16, answer.body) }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = words(error)
            return
        } finally {
            isWorking = false
        }
        sync(force = true)
    }

    /** Sends one of the core's changes of the account and keeps the account its answer holds. */
    private suspend fun changeAccount(request: () -> ServerRequest) {
        val server = account?.server ?: return
        if (isWorking) return
        isWorking = true
        try {
            val answer = ask(withContext(background) { request() }, server, answerWithinMilliseconds = ANSWER_TO_OTHERS_WITHIN_MILLISECONDS)
            withContext(background) { core.accountRead(answer.status16, answer.body) }?.let { account = it }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = words(error)
        } finally {
            isWorking = false
        }
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
        val end = toItsEnd(server, reportsProblems, { run.start(clock() / 1000) }, run::receive)
        if (end is SyncStep.Finished) lastSync = end.summary
        account = withContext(background) { runCatching { core.account() }.getOrNull() }
        onLogbookChanged()
        // The pilot's own take-offs follow the account wherever the logbook does.
        runKeptSitesSync(reportsProblems)
        // And so does what the pilot's alarms fired.
        runAlertsSync()
    }

    /**
     * Fetches the account's alarms and what they fired since the last run,
     * if the pilot is signed in: after an alarm was saved, and from the
     * app's periodic check. Returns what is new, the core's lines, the
     * oldest first. It is quiet: the list says what the phone last had.
     */
    suspend fun syncAlerts(): List<FiredLine> = if (isSignedIn) runAlertsSync() else emptyList()

    private suspend fun runAlertsSync(): List<FiredLine> {
        val server = account?.server?.takeIf { isSignedIn } ?: return emptyList()
        val new = alertsRun.withLock {
            val run = core.alertsSync()
            val end = toItsEnd(server, reportsProblems = false, { run.start() }, run::receive)
            if (end is SyncStep.Finished) {
                withContext(background) { run.newLines(utcOffsetSeconds(clock())) }
            } else {
                emptyList()
            }
        }
        onAlertsFetched(new)
        return new
    }

    /**
     * Brings the pilot's own take-offs in step with the account, after one
     * was kept or forgotten. Signed out it does nothing: the list is then
     * on this phone alone. It is quiet: the list itself says that
     * something waits for the next sync.
     */
    suspend fun syncKeptSites() {
        if (!isSignedIn) return
        if (isWorking) {
            // What the present run does not take, the next one does.
            isSyncDue = true
            return
        }
        isWorking = true
        try {
            runKeptSitesSync(reportsProblems = false)
            while (isSyncDue && isSignedIn) {
                isSyncDue = false
                runSync(reportsProblems = false)
            }
        } finally {
            isWorking = false
        }
    }

    private suspend fun runKeptSitesSync(reportsProblems: Boolean) {
        val server = account?.server?.takeIf { isSignedIn } ?: return
        val run = core.keptSitesSync()
        toItsEnd(server, reportsProblems, { run.start(clock() / 1000) }, run::receive)
        account = withContext(background) { runCatching { core.account() }.getOrNull() }
        onKeptSitesChanged()
    }

    /**
     * Runs one of the core's syncs to its end: sends each request the core
     * hands over, with the session's token, and hands the answer back,
     * until the core says that it has finished or why it stopped. Returns
     * the step it ended on.
     */
    private suspend fun toItsEnd(
        server: String,
        reportsProblems: Boolean,
        start: () -> SyncStep,
        receive: (UShort, ByteArray, UInt?) -> SyncStep,
    ): SyncStep {
        var step = withContext(background) { start() }
        while (true) {
            val request = (step as? SyncStep.Send)?.request ?: break
            step = try {
                val answer = ask(request, server)
                withContext(background) { receive(answer.status16, answer.body, answer.retryAfterSeconds) }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                // What was done so far is kept; the next sync goes on.
                if (reportsProblems) problem = words(error)
                break
            }
        }
        val end = step
        if (end is SyncStep.Failed) {
            if (end.needsSignIn) {
                withContext(background) { session.remove() }
            }
            if (reportsProblems || end.needsSignIn) problem = end.message
        }
        return end
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

    // The cockpit's requests

    /**
     * Carries a request of the cockpit's to the pilot's server: the
     * positions of a flight shared while it is under way. `null` when no
     * answer came. It waits twenty seconds for one and no longer: the next
     * positions are due by then.
     */
    suspend fun carry(request: ServerRequest): HttpAnswer? {
        val server = account?.server ?: return null
        return try {
            ask(request, server, answerWithinMilliseconds = CARRY_WITHIN_MILLISECONDS)
        } catch (error: CancellationException) {
            throw error
        } catch (_: NotSignedIn) {
            // No token to send it with: as good as the server's refusal.
            HttpAnswer(401, ByteArray(0))
        } catch (_: Exception) {
            null
        }
    }

    // Reports

    /** What became of a report: `sent` says it in a line, or `problem` says why it did not go. */
    data class ReportOutcome(val sent: String? = null, val problem: String? = null)

    /**
     * Tells whoever runs the pilot's server that a flight, a pilot's page
     * or a live track should not be there. The request and the words for
     * the answer are the core's; what the core refuses is not sent.
     * Nothing of a report is kept on this phone.
     */
    suspend fun report(subject: ReportSubject, reason: ReportReason, text: String): ReportOutcome = try {
        val answer = answer(withContext(background) { reportRequest(subject, reason, text) })
        ReportOutcome(sent = withContext(background) { reportSent(answer.status16, answer.body) })
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        ReportOutcome(problem = words(error))
    }

    // Requests of other parts of the app

    /**
     * Sends a request of another part of the app to the pilot's server,
     * with the session's token, and returns the answer: a report goes
     * this way, and so do the groups and their plans. Throws when the
     * phone is not signed in and when no answer came; [words] says either
     * to the pilot.
     */
    suspend fun answer(request: ServerRequest): HttpAnswer {
        val server = account?.server?.takeIf { isSignedIn } ?: throw NotSignedIn()
        return ask(request, server, answerWithinMilliseconds = ANSWER_TO_OTHERS_WITHIN_MILLISECONDS)
    }

    // Requests

    /**
     * Sends one of the core's requests to the server, with the session
     * token where the request needs it and nowhere else. `token` is given
     * for the one request that is made before the token is kept.
     */
    private suspend fun ask(
        request: ServerRequest,
        server: String,
        token: String? = null,
        answerWithinMilliseconds: Int = ANSWER_WITHIN_MILLISECONDS,
    ): HttpAnswer {
        val headers = mutableListOf("Accept" to "application/json")
        request.contentType?.let { headers += "Content-Type" to it }
        if (request.needsToken) {
            val bearer = token ?: withContext(background) { session.read() } ?: throw NotSignedIn()
            headers += "Authorization" to "Bearer $bearer"
        }
        return transport(request.method, server + request.path, headers, request.body, answerWithinMilliseconds)
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

    /** What went wrong with a request, in words for the pilot. */
    fun words(error: Exception): String = when (error) {
        is CoreException -> error.pilotMessage
        is NotSignedIn -> "This phone is no longer signed in. Sign in again."
        else -> "Venti could not be reached. ${error.pilotMessage}"
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** How far this phone's clock is ahead of UTC at `millis`, in seconds: for the core's times of day. */
        fun utcOffsetSeconds(millis: Long): Int = TimeZone.getDefault().getOffset(millis) / 1000

        /** How long a request may take: a flight's file is sent in one. */
        const val ANSWER_WITHIN_MILLISECONDS = 120_000

        /** How long a request of the cockpit's may take. */
        const val CARRY_WITHIN_MILLISECONDS = 20_000

        /** How long a report, or a request about groups and plans, may take. */
        const val ANSWER_TO_OTHERS_WITHIN_MILLISECONDS = 30_000

        /** How long after a sync the next one that nobody asked for waits. */
        const val PAUSE_MILLISECONDS = 60_000L

        /**
         * Sends with the app's own networking, which keeps nothing: answers
         * about an account are not for a cache, and there are no cookies.
         */
        val overHttp: AccountTransport = { method, url, headers, body, answerWithinMilliseconds ->
            Http.send(method, url, headers, body, answerWithinMilliseconds)
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
        FlightVisibility.PRIVATE -> "The flight is kept on Venti for your devices. Nobody else can see it."
        FlightVisibility.UNLISTED -> "Whoever has the link can see the flight. It is listed nowhere."
        FlightVisibility.PUBLIC -> "Anyone can see the flight, and it is listed on your page."
    }
