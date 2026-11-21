package fly.venti.app.sites

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.account.AccountModel
import fly.venti.app.community.ReportWords
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.Coordinate
import fly.venti.core.CoreException
import fly.venti.core.ServerRequest
import fly.venti.core.SiteDraftView
import fly.venti.core.SiteForm
import fly.venti.core.SiteHistory
import fly.venti.core.SiteInTheWay
import fly.venti.core.SitePage
import fly.venti.core.SiteView
import fly.venti.core.SiteWritten
import fly.venti.core.siteAddRequest
import fly.venti.core.siteChangeRequest
import fly.venti.core.siteChangesBetween
import fly.venti.core.siteDraftMoved
import fly.venti.core.siteDraftNew
import fly.venti.core.siteDraftOf
import fly.venti.core.siteForm
import fly.venti.core.siteHistory
import fly.venti.core.siteHistoryPath
import fly.venti.core.siteInTheWay
import fly.venti.core.sitePage
import fly.venti.core.sitePath
import fly.venti.core.siteRemoveRequest
import fly.venti.core.siteRemoved
import fly.venti.core.siteRestoreRequest
import fly.venti.core.siteWritten
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Adding and correcting a flying site of Venti's own layer, the sites that
 * pilots make.
 *
 * No rule of it is here: the core says what the form shows, checks a
 * draft, puts every request together and reads every answer into words.
 * This class sends what the core gives, a change with the account's token
 * and a look without one, and keeps nothing.
 */
class SiteEditModel(
    private val account: AccountModel,
    /** Asks for a site and its history, which anyone may read. */
    private val read: WindTransport = { url -> Http.get(url) },
    private val background: CoroutineDispatcher = Dispatchers.Default,
) {
    /** The app's Venti, without a slash at its end; `null` without one. */
    var server: () -> String? = { null }

    /** Whether that server takes sites from pilots, as its health answer says. */
    var takesSites: () -> Boolean = { false }

    /** What the form shows: the core's fields, choices and words. */
    val form: SiteForm by lazy { siteForm() }

    /** Whether a site can be sent from here: signed in, on the server the sites are on. */
    val isSignedIn: Boolean
        get() = ReportWords.obstacle(account.isSignedIn, account.account?.server, server()) == null

    /** What became of a draft that was sent. */
    sealed interface Sent {
        /** The server took it: the site as it stands, and the core's line. */
        data class Written(val written: SiteWritten) : Sent

        /**
         * It did not: why, in the core's words; the site that stands in
         * the way, if that is why; and the site as it stands now, if
         * someone else changed it meanwhile.
         */
        data class Refused(val words: String, val inTheWay: SiteInTheWay? = null, val now: SiteView? = null) : Sent
    }

    /** Sends a draft: a new site, or the correction of the one it was made from. */
    suspend fun send(draft: SiteDraftView, comment: String): Sent {
        val id = draft.siteId
        return try {
            val answer = account.answer(
                withContext(background) {
                    if (id == null) siteAddRequest(draft, comment) else siteChangeRequest(draft, comment)
                },
            )
            try {
                Sent.Written(withContext(background) { siteWritten(answer.status16, answer.body) })
            } catch (error: CoreException) {
                val inTheWay = withContext(background) { siteInTheWay(answer.status16, answer.body) }
                // A correction that was refused: the site as it stands, if
                // it stands at another revision than the draft was made from.
                val now = if (id != null && inTheWay == null) {
                    runCatching { page(id).site }.getOrNull()?.takeIf { it.byPilots?.revision != draft.revision }
                } else {
                    null
                }
                Sent.Refused(error.pilotMessage, inTheWay, now)
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Sent.Refused(account.words(error))
        }
    }

    /** Removes a site, as a new revision; the core's line about it. Throws what [words] puts into words. */
    suspend fun remove(id: String, comment: String): String =
        write({ siteRemoveRequest(id, comment) }) { status, body -> siteRemoved(status, body) }

    /** Puts a site back to what one of its revisions said: how a change is undone and a removed site comes back. */
    suspend fun restore(id: String, revision: UInt, comment: String): SiteWritten =
        write({ siteRestoreRequest(id, revision, comment) }) { status, body -> siteWritten(status, body) }

    /** One site as it stands now. */
    suspend fun page(id: String): SitePage {
        val answer = look(withContext(background) { sitePath(id) })
        return withContext(background) { sitePage(answer.status16, answer.body) }
    }

    /**
     * A site's whole history: the server's pages one after the other, as
     * one. What the first revision of a later page changed is asked of
     * the core, which a page alone cannot say.
     */
    suspend fun history(id: String): SiteHistory {
        var whole: SiteHistory? = null
        var after: UInt? = null
        for (page in 0 until PAGES_AT_MOST) {
            val answer = look(withContext(background) { siteHistoryPath(id, after) })
            val next = withContext(background) { siteHistory(answer.status16, answer.body) }
            val before = whole
            whole = if (before == null) {
                next
            } else {
                val last = before.revisions.lastOrNull()
                val joined = next.revisions.mapIndexed { index, revision ->
                    if (index == 0 && last != null && revision.changes.isEmpty()) {
                        revision.copy(changes = withContext(background) { siteChangesBetween(last.site, revision.site) })
                    } else {
                        revision
                    }
                }
                next.copy(revisions = before.revisions + joined)
            }
            after = next.nextAfter ?: break
        }
        return checkNotNull(whole)
    }

    /** What went wrong, in words for the pilot. */
    fun words(error: Exception): String = account.words(error)

    private suspend fun <Value> write(request: () -> ServerRequest, answered: (UShort, ByteArray) -> Value): Value {
        val answer = account.answer(withContext(background) { request() })
        return withContext(background) { answered(answer.status16, answer.body) }
    }

    private suspend fun look(path: String): HttpAnswer {
        val server = server() ?: throw NoServer()
        return read(server + path)
    }

    /** There is no Venti to ask. */
    private class NoServer : Exception("Venti is not connected.")

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()

    companion object {
        /** The most pages of a history that are asked for: a site with more shows its first ones. */
        const val PAGES_AT_MOST = 20
    }
}

/**
 * A draft while its form is open: what the pilot has entered so far, kept
 * outside the sheet so that it is there again after the place was moved
 * on the map, and after a correction that the server refused.
 */
class SiteFormDraft private constructor(
    base: SiteDraftView,
    /** The site the draft corrects, as it stood when the draft was made; `null` for a new one. */
    site: SiteView?,
) {
    /** The draft as it was made, from a place or from a site: what the pilot's entries are held against. */
    var base: SiteDraftView = base
        private set

    var site: SiteView? = site
        private set

    var draft: SiteDraftView by mutableStateOf(base)

    /** The height as the pilot typed it. */
    var height: String by mutableStateOf(heightAsTyped(base.elevationM))
        private set

    /** The line about the change, which belongs to the revision and not to the site. */
    var comment: String by mutableStateOf("")

    /** Whether the pilot has tried to send: from then on the faults stand at their fields. */
    var hasTried: Boolean by mutableStateOf(false)

    /** Why the server did not take the draft, in the core's words. */
    var problem: String? by mutableStateOf(null)

    /** The site that stands where this one was to be added, to open in its place. */
    var inTheWay: SiteInTheWay? by mutableStateOf(null)

    /** What someone else changed while the form was open, a line a field, in the core's words. */
    var meanwhile: List<String> by mutableStateOf(emptyList())

    /** Whether the draft is of a site that is new. */
    val isNew: Boolean get() = base.siteId == null

    /** Takes what was typed as the height, if it can be one: digits, with a sign and a fraction. */
    fun typeHeight(text: String) {
        if (!HEIGHT.matches(text)) return
        height = text
        draft = draft.copy(elevationM = text.toDoubleOrNull())
    }

    /** The site at another place. An outline goes with the old place, which the core knows. */
    fun moved(place: Coordinate) {
        draft = siteDraftMoved(draft, place)
    }

    /**
     * Someone else changed the site while the form was open: the draft is
     * made again from the site as it stands now, and what the pilot had
     * changed in the form is laid over it, so that nothing entered is lost
     * and nothing the other pilot changed is undone unseen.
     */
    fun carryOnto(now: SiteView) {
        fun <Value> mine(was: Value, entered: Value, stands: Value): Value = if (entered != was) entered else stands
        val stands = siteDraftOf(now)
        val old = base
        val entered = draft
        meanwhile = site?.let { siteChangesBetween(it, now) }.orEmpty()
        val placed = if (entered.position != old.position) siteDraftMoved(stands, entered.position) else stands
        draft = placed.copy(
            name = mine(old.name, entered.name, stands.name),
            elevationM = mine(old.elevationM, entered.elevationM, stands.elevationM),
            uses = mine(old.uses, entered.uses, stands.uses),
            windFromDeg = mine(old.windFromDeg, entered.windFromDeg, stands.windFromDeg),
            description = mine(old.description, entered.description, stands.description),
            fee = mine(old.fee, entered.fee, stands.fee),
            website = mine(old.website, entered.website, stands.website),
            noFlightTime = mine(old.noFlightTime, entered.noFlightTime, stands.noFlightTime),
            guestGuidelines = mine(old.guestGuidelines, entered.guestGuidelines, stands.guestGuidelines),
        )
        if (entered.elevationM == old.elevationM) height = heightAsTyped(stands.elevationM)
        base = stands
        site = now
    }

    companion object {
        /** A draft for a new site at a place, with nothing said yet. */
        fun new(place: Coordinate): SiteFormDraft = SiteFormDraft(siteDraftNew(place), null)

        /** A draft that corrects a site of Venti's own layer. Throws the core's refusal for a site of another source. */
        fun of(site: SiteView): SiteFormDraft = SiteFormDraft(siteDraftOf(site), site)

        private val HEIGHT = Regex("""-?\d{0,5}(\.\d{0,2})?""")

        /** A height as a pilot would type it: whole metres without a fraction. */
        fun heightAsTyped(metres: Double?): String = when {
            metres == null -> ""
            metres == Math.floor(metres) && Math.abs(metres) < 1e9 -> metres.toLong().toString()
            else -> metres.toString()
        }
    }
}
