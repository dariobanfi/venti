package fly.venti.app.sites

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.account.AccountModel
import fly.venti.app.pilotMessage
import fly.venti.app.wind.WindTransport
import fly.venti.core.Bounds
import fly.venti.core.CandidateOffer
import fly.venti.core.Coordinate
import fly.venti.core.Core
import fly.venti.core.CoreException
import fly.venti.core.ServerRequest
import fly.venti.core.SiteAdding
import fly.venti.core.SiteCandidates
import fly.venti.core.SiteConfirmFlight
import fly.venti.core.SiteConfirmForm
import fly.venti.core.SiteConfirmations
import fly.venti.core.SiteCorrecting
import fly.venti.core.SiteDraftFault
import fly.venti.core.SiteDraftView
import fly.venti.core.SiteField
import fly.venti.core.SiteFlights
import fly.venti.core.SiteFlightsWords
import fly.venti.core.SiteForm
import fly.venti.core.SiteHistory
import fly.venti.core.SiteHistoryOffers
import fly.venti.core.SiteInTheWay
import fly.venti.core.SiteOffers
import fly.venti.core.SitePage
import fly.venti.core.SiteView
import fly.venti.core.SiteWritten
import fly.venti.core.siteAddRequest
import fly.venti.core.siteCandidates
import fly.venti.core.siteCandidatesPath
import fly.venti.core.siteChangeRequest
import fly.venti.core.siteConfirmForm
import fly.venti.core.siteConfirmationsOf
import fly.venti.core.siteConfirmationsPath
import fly.venti.core.siteConfirmed
import fly.venti.core.siteCorrecting
import fly.venti.core.siteDraftMoved
import fly.venti.core.siteDraftNew
import fly.venti.core.siteDraftOf
import fly.venti.core.siteDraftRebased
import fly.venti.core.siteFlights
import fly.venti.core.siteFlightsPath
import fly.venti.core.siteFlightsWords
import fly.venti.core.siteForm
import fly.venti.core.siteHistoryPage
import fly.venti.core.siteHistoryPath
import fly.venti.core.siteInTheWay
import fly.venti.core.sitePage
import fly.venti.core.sitePath
import fly.venti.core.siteRemoveRequest
import fly.venti.core.siteRemoved
import fly.venti.core.siteRestoreRequest
import fly.venti.core.siteWasChanged
import fly.venti.core.siteWithdrawRequest
import fly.venti.core.siteWithdrawn
import fly.venti.core.siteWords
import fly.venti.core.siteWritten
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Adding, correcting and confirming a flying site of Venti's own layers:
 * the partners' sites and the places from pilots.
 *
 * No rule of it is here: the core says what may be offered for a site to
 * the account this phone stores, what the forms show, checks a draft,
 * puts every request together and reads every answer into words. This
 * class sends what the core gives, a change with the account's token and
 * a look without one, and keeps nothing.
 */
class SiteEditModel(
    private val core: Core,
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

    /** The words around a site's page, its form and its history: the core's. */
    val says: fly.venti.core.SiteWords by lazy { siteWords() }

    /**
     * Whether this phone holds a session: only for where "Sign in" leads
     * to the account. What a pilot may do with a site is [offers] and
     * [adding], never this.
     */
    val hasSession: Boolean get() = account.isSignedIn

    /**
     * What may be offered for a site to whoever is signed in on this
     * phone, as the core's one answer; with the site's confirmations it
     * knows the pilot's own among them. `null` when the core cannot say.
     */
    fun offers(site: SiteView, confirmations: SiteConfirmations? = null): SiteOffers? {
        // Read with the account as it is known now: a screen that shows the answer follows the account.
        account.account
        return runCatching { core.siteOffers(site, takesSites(), confirmations) }.getOrNull()
    }

    /** Whether a site may be added from here, into which tier, and the words of the entry and the form: the core's. */
    fun adding(): SiteAdding? {
        account.account
        return runCatching { core.siteAdding(takesSites()) }.getOrNull()
    }

    /**
     * What a site's history offers to whoever is signed in on this phone:
     * putting a revision back, undoing the last change, bringing a removed
     * site back and reporting a revision, with the core's line about what
     * is missing. The core reads the tier from the history's own
     * identifier, so no site's page is needed for it. `null` when the core
     * cannot say.
     */
    fun historyOffers(history: SiteHistory): SiteHistoryOffers? {
        account.account
        return runCatching { core.siteHistoryOffers(history, takesSites()) }.getOrNull()
    }

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
                // A correction refused because someone else changed the
                // site meanwhile, as the core reads the answer: the site
                // as it stands now, to carry the draft onto.
                val now = if (id != null && withContext(background) { siteWasChanged(answer.status16, answer.body) }) {
                    runCatching { page(id).site }.getOrNull()
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
     * one. Each page is read with the last revision of the page before,
     * so that the core says what its first revision changed.
     */
    suspend fun history(id: String): SiteHistory {
        var whole: SiteHistory? = null
        var after: UInt? = null
        for (page in 0 until PAGES_AT_MOST) {
            val answer = look(withContext(background) { siteHistoryPath(id, after) })
            val before = whole
            val next = withContext(background) {
                siteHistoryPage(answer.status16, answer.body, before?.revisions?.lastOrNull())
            }
            whole = next.copy(revisions = before?.revisions.orEmpty() + next.revisions)
            after = next.nextAfter ?: break
        }
        return checkNotNull(whole)
    }

    // Confirming: "I took off here and it was good"

    /**
     * Who says they took off at a site, which anyone may read: as the core
     * reads the list for that site's page, which says nothing of "nobody
     * yet" for a site nobody takes off at.
     */
    suspend fun confirmations(site: SiteView): SiteConfirmations {
        val answer = look(withContext(background) { siteConfirmationsPath(site.id) })
        return withContext(background) { siteConfirmationsOf(site, answer.status16, answer.body) }
    }

    /** The form for a confirmation: the days to choose from, counted back from this phone's date, and the core's words. */
    fun confirmForm(today: LocalDate = LocalDate.now()): SiteConfirmForm = siteConfirmForm(today.toString())

    /** The pilot's own flights of a day that began at the site and are on the server; mostly none. */
    suspend fun confirmFlights(site: SiteView, day: String): List<SiteConfirmFlight> = withContext(background) {
        runCatching {
            // How far this phone's clock is ahead of UTC on that day.
            val ahead = ZoneId.systemDefault().rules.getOffset(LocalDate.parse(day).atTime(12, 0)).totalSeconds
            core.siteConfirmFlights(site, day, ahead)
        }.getOrDefault(emptyList())
    }

    /** Says of a site that the pilot took off there on a day; again, it changes the day. The site with it counted, and the core's line. */
    suspend fun confirm(id: String, day: String, note: String, flight: String?, today: LocalDate = LocalDate.now()): SiteWritten =
        write({ core.siteConfirmRequest(id, day, today.toString(), note, flight) }) { status, body -> siteConfirmed(status, body, day) }

    /** Takes the pilot's own confirmation of a site back; the core's line about it. */
    suspend fun withdraw(id: String): String =
        write({ siteWithdrawRequest(id) }) { status, body -> siteWithdrawn(status, body) }

    // What public flights say about where pilots take off

    /** The words around a site's public flights and the places no site names: the core's. */
    val flightsWords: SiteFlightsWords by lazy { siteFlightsWords() }

    /** What the public flights that began at a site say about it, which anyone may read. */
    suspend fun flights(siteId: String): SiteFlights {
        val answer = look(withContext(background) { siteFlightsPath(siteId) })
        return withContext(background) { siteFlights(answer.status16, answer.body) }
    }

    /**
     * The places in a part of the map where pilots take off and no site
     * stands, which anyone may read. A part larger than the server answers
     * for is refused by the core before it is asked, in the words to show
     * over the map.
     */
    suspend fun candidates(area: Bounds): SiteCandidates {
        val answer = look(withContext(background) { siteCandidatesPath(area.south, area.west, area.north, area.east) })
        return withContext(background) { siteCandidates(answer.status16, answer.body) }
    }

    /**
     * The places of an answer this pilot took off at and may add, as the
     * core offers them, the newest flight's place first. None for a
     * partner: the phone shows a partner no entry to add, as it shows
     * none on the map. None when the core cannot say.
     */
    suspend fun candidateOffers(candidates: SiteCandidates, today: LocalDate = LocalDate.now()): List<CandidateOffer> {
        if (candidates.candidates.isEmpty()) return emptyList()
        if (adding()?.asPartner != false) return emptyList()
        return withContext(background) {
            // How far this phone's clock is ahead of UTC, for the day and hour of the pilot's flight.
            val ahead = ZoneId.systemDefault().rules.getOffset(today.atTime(12, 0)).totalSeconds
            runCatching { core.candidateForPilot(candidates, takesSites(), ahead) }.getOrDefault(emptyList())
        }
    }

    /** "Not this one": the place is not offered to this pilot again, on this phone. */
    suspend fun setAside(place: Coordinate) {
        withContext(background) { runCatching { core.candidateSetAside(place) } }
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
    made: SiteDraftView,
    /** The site the draft corrects, as it stood when the draft was made; `null` for a new one. */
    site: SiteView?,
    /** The words of the form, by the tier the site goes to or is of: the core's. */
    val words: SiteFormWords,
) {
    var site: SiteView? = site
        private set

    var draft: SiteDraftView by mutableStateOf(made)

    /** The height as the pilot typed it. */
    var height: String by mutableStateOf(heightAsTyped(made.elevationM))
        private set

    /** The line about the change, which belongs to the revision and not to the site. */
    var comment: String by mutableStateOf("")

    /** Whether the pilot has tried to send: from then on the faults stand at their fields. */
    var hasTried: Boolean by mutableStateOf(false)

    /** Why the server did not take the draft, in the core's words. */
    var problem: String? by mutableStateOf(null)

    /** The site that stands where this one was to be added, to open in its place. */
    var inTheWay: SiteInTheWay? by mutableStateOf(null)

    /** After someone else changed the site while the form was open: what became of the draft, in the core's words. */
    var meanwhileWords: String? by mutableStateOf(null)
        private set

    /** What they changed, a line a field, in the core's words. */
    var meanwhile: List<String> by mutableStateOf(emptyList())
        private set

    /**
     * The fields both changed, each with the core's sentence to show at
     * the field: "Height: you entered 1840 m, it now stands at 1850 m.
     * Yours is kept."
     */
    var clashes: List<SiteDraftFault> by mutableStateOf(emptyList())
        private set

    /** Whether the draft is of a site that is new. */
    val isNew: Boolean = made.siteId == null

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
     * Someone else changed the site while the form was open: the core
     * carries the draft onto the site as it stands now, with what the
     * pilot had entered laid over it, and says what the others changed
     * and where both did. Returns whether it could.
     */
    fun rebase(now: SiteView): Boolean {
        val was = site ?: return false
        val rebased = runCatching { siteDraftRebased(was, draft, now) }.getOrNull() ?: return false
        draft = rebased.draft
        // A height the pilot left alone is the site's, also as text.
        if (SiteField.HEIGHT !in rebased.kept) height = heightAsTyped(rebased.draft.elevationM)
        meanwhileWords = rebased.words
        meanwhile = rebased.theirs
        clashes = rebased.clashesInWords
        site = now
        return true
    }

    companion object {
        /** A draft for a new site at a place, with nothing said yet, for the tier the core names. */
        fun new(place: Coordinate, adding: SiteAdding): SiteFormDraft =
            SiteFormDraft(siteDraftNew(place), null, SiteFormWords(adding))

        /**
         * A draft for a place pilots take off from that the core offered
         * this pilot: at their own take-off, with its height and the use,
         * as the offer's draft has it.
         */
        fun offered(offer: CandidateOffer, adding: SiteAdding): SiteFormDraft =
            SiteFormDraft(offer.draft, null, SiteFormWords(adding))

        /**
         * A draft that corrects a site of Venti's own layers, under the
         * words of that site's tier. Throws the core's refusal for a site
         * of another source.
         */
        fun of(site: SiteView): SiteFormDraft = SiteFormDraft(siteDraftOf(site), site, SiteFormWords(siteCorrecting(site)))

        private val HEIGHT = Regex("""-?\d{0,5}(\.\d{0,2})?""")

        /** A height as a pilot would type it: whole metres without a fraction. */
        fun heightAsTyped(metres: Double?): String = when {
            metres == null -> ""
            metres == Math.floor(metres) && Math.abs(metres) < 1e9 -> metres.toLong().toString()
            else -> metres.toString()
        }
    }
}

/**
 * What the form says around its fields: its title, what giving a site
 * means, what a site of the tier is worth, and the button that sends. The
 * core's, for a new site by the tier the account adds to, for a correction
 * by the tier of the site corrected.
 */
class SiteFormWords(val title: String, val giving: String, val note: String, val send: String) {
    constructor(adding: SiteAdding) : this(adding.title, adding.giving, adding.note, adding.sendWords)

    constructor(correcting: SiteCorrecting) : this(correcting.title, correcting.giving, correcting.note, correcting.sendWords)
}
