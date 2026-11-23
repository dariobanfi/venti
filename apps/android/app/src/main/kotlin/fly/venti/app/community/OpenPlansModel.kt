package fly.venti.app.community

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.Http
import fly.venti.app.HttpAnswer
import fly.venti.app.account.AccountModel
import fly.venti.app.groups.PlanWords
import fly.venti.app.wind.WindTransport
import fly.venti.core.Coordinate
import fly.venti.core.Core
import fly.venti.core.OpenPlanDraft
import fly.venti.core.OpenPlanView
import fly.venti.core.OpenPlans
import fly.venti.core.ServerRequest
import fly.venti.core.TodayTakeoff
import fly.venti.core.pilotsWords
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.time.LocalDate

/**
 * Open plans as Community sees them (docs/design.md, "Open plans"): the
 * plans for everyone flying from an area near a place for the coming
 * days, one plan on its own page, and the pilot's posting, changing,
 * deleting and "I'm in".
 *
 * Nothing of it is kept on the phone. The core puts the requests together
 * and reads the answers; a request that needs the pilot goes with the
 * session's token (`AccountModel.answer`), the others as anyone. This
 * holds the last answer for the place asked about and lays what a write
 * answered over it. Nothing notifies: the list is asked for when Community
 * shows, when the pilot signs in or out, and after every write.
 */
class OpenPlansModel(
    private val core: Core,
    private val account: AccountModel,
    /** Fetches an address without a token: what everyone may read. */
    private val transport: WindTransport = { url -> Http.get(url) },
    private val background: CoroutineDispatcher = Dispatchers.IO,
    /** The pilot's own today: a plan is for a day where the pilot is. */
    private val todayIs: () -> LocalDate = LocalDate::now,
) {
    /** Venti's address, without a slash at its end. The app sets this to the one the lists are read from. */
    var server: () -> String? = { null }

    /** The take-offs around a place, nearest first, for the form's choice of a place. The app sets it to Today's. */
    var takeoffsNear: suspend (Coordinate) -> List<TodayTakeoff> = { emptyList() }

    /** The open plans near the place last asked about. `null` until the server has answered. */
    var near: OpenPlans? by mutableStateOf(null)
        private set

    /** Where [near] is for. */
    var place: Coordinate? by mutableStateOf(null)
        private set

    var isLoading: Boolean by mutableStateOf(false)
        private set

    /** Why the list is not there or not current, in words for the pilot. */
    var problem: String? by mutableStateOf(null)
        private set

    /** Counts the writes that went through, for a page that shows one plan. */
    var revision: Int by mutableIntStateOf(0)
        private set

    /** Whether a pilot is signed in, for whom "I'm in" and posting are. */
    val isSignedIn: Boolean get() = account.isSignedIn

    /** The pilot's own today, written `2026-11-21`: the first day asked for and the first a plan may be for. */
    fun today(): LocalDate = todayIs()

    /** The plan with an identifier, as the list has it now. */
    fun listed(id: String): OpenPlanView? = near?.days.orEmpty().firstNotNullOfOrNull { day -> day.plans.firstOrNull { it.id == id } }

    /** Asks for the open plans near a place, for the coming days. */
    suspend fun showNear(place: Coordinate) {
        if (place != this.place) near = null
        this.place = place
        load()
    }

    /** Asks again for the place last given, from the pilot's own today on. */
    suspend fun load() {
        val place = place ?: return
        isLoading = true
        problem = null
        try {
            val first = today()
            val read = send({
                core.openPlansNearRequest(place, null, PlanWords.day(first), PlanWords.day(first.plusDays(OpenPlanWords.DAYS - 1)))
            }) { status, body -> core.openPlans(status, body) }
            // An answer for a place asked about before is not this one's.
            if (place == this.place) near = read
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = words(error)
        } finally {
            isLoading = false
        }
    }

    /** One open plan as the server has it now; anyone may read it. */
    suspend fun plan(id: String): OpenPlanView = send({ core.openPlanRequest(id) }) { status, body -> core.openPlan(status, body) }

    /** Posts an open plan, which everyone can read. The list is asked for again. */
    suspend fun post(draft: OpenPlanDraft): OpenPlanView {
        val posted = send({ core.postOpenPlanRequest(draft) }) { status, body -> core.openPlan(status, body) }
        wrote()
        return posted
    }

    /** Changes one of the pilot's own open plans to what the draft says, whole. */
    suspend fun change(id: String, draft: OpenPlanDraft): OpenPlanView {
        val changed = send({ core.changeOpenPlanRequest(id, draft) }) { status, body -> core.openPlan(status, body) }
        wrote()
        return changed
    }

    suspend fun delete(id: String) {
        send({ core.deleteOpenPlanRequest(id) }) { status, body -> core.groupDone(status, body) }
        wrote()
    }

    /** Says that the pilot is in on an open plan, or takes that back; the answer is laid over the list. */
    suspend fun setIn(id: String, isIn: Boolean): OpenPlanView {
        val said = send({ core.openPlanInRequest(id, isIn) }) { status, body -> core.openPlan(status, body) }
        near = near?.let { shown ->
            shown.copy(days = shown.days.map { day -> day.copy(plans = day.plans.map { if (it.id == said.id) said else it }) })
        }
        revision += 1
        return said
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
        words(error)
    }

    /** What went wrong, in words for the pilot. */
    fun words(error: Exception): String = if (error is Unreachable) pilotsWords().unreachable else account.words(error)

    private suspend fun wrote() {
        revision += 1
        load()
    }

    /**
     * Puts a request together, sends it (with the account's token where
     * the request needs one, as anyone where not) and reads the answer.
     */
    private suspend fun <Value> send(request: () -> ServerRequest, read: (UShort, ByteArray) -> Value): Value {
        val asked = withContext(background) { request() }
        val answer = if (asked.needsToken) {
            account.answer(asked)
        } else {
            val server = server() ?: throw Unreachable()
            transport(server + asked.path)
        }
        return withContext(background) { read(answer.status16, answer.body) }
    }

    /** Venti's address is not known. */
    private class Unreachable : Exception()

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
}
