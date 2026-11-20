package fly.venti.app.groups

import fly.venti.core.Coordinate
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.app.account.AccountModel
import fly.venti.core.Core
import fly.venti.core.GroupDetail
import fly.venti.core.GroupJoined
import fly.venti.core.GroupMade
import fly.venti.core.GroupView
import fly.venti.core.PlanBoard
import fly.venti.core.PlanDraft
import fly.venti.core.PlanView
import fly.venti.core.ServerRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.time.LocalDate

/**
 * Groups and their plans for a flying day, on the pilot's Venti server.
 *
 * Nothing of it is kept on the phone: the board is asked for when it is
 * opened and after every change. The core puts the requests together and
 * reads the answers; the account sends them with the session's token.
 */
class GroupsModel(
    private val core: Core,
    private val account: AccountModel,
    private val background: CoroutineDispatcher = Dispatchers.IO,
    /** The pilot's own today: a plan is for a day where the pilot is. */
    private val today: () -> LocalDate = LocalDate::now,
) {
    /**
     * The plans of the pilot's groups from today on, and the groups.
     * `null` until the server has answered once.
     */
    var board: PlanBoard? by mutableStateOf(null)
        private set

    var isLoading: Boolean by mutableStateOf(false)
        private set

    /** Why the board is not there, in words for the pilot. */
    var problem: String? by mutableStateOf(null)
        private set

    val groups: List<GroupView> get() = board?.groups.orEmpty()

    /** The plan with an identifier, as the board has it now. */
    /**
     * The plans of today and tomorrow that say where on the map they are:
     * who flies where, for the map to mark. A plan for next week is in the
     * board, not on the map.
     */
    fun pins(today: LocalDate = LocalDate.now()): List<PlanPin> {
        val near = setOf(PlanWords.day(today), PlanWords.day(today.plusDays(1)))
        return board?.days.orEmpty()
            .filter { it.day in near }
            .flatMap { it.plans }
            .mapNotNull { plan ->
                plan.position?.let { position -> PlanPin(plan.id, position, listOfNotNull(plan.place, plan.meetAt).joinToString(" ")) }
            }
            .let(PlanPin::gathered)
    }

    fun plan(id: String): PlanView? = board?.days.orEmpty().firstNotNullOfOrNull { day -> day.plans.firstOrNull { it.id == id } }

    /** Asks for the board: every plan of the pilot's groups from today on. */
    suspend fun load() {
        if (!account.isSignedIn) {
            board = null
            problem = null
            return
        }
        isLoading = true
        try {
            val from = PlanWords.day(today())
            board = send({ core.planBoardRequest(from, null) }) { status, body -> core.planBoard(status, body) }
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = account.words(error)
        } finally {
            isLoading = false
        }
    }

    /**
     * Does one thing on the server and returns what went wrong, in words
     * for the pilot; `null` when nothing did.
     */
    suspend fun attempt(work: suspend () -> Unit): String? = try {
        work()
        null
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        account.words(error)
    }

    // Groups

    suspend fun make(name: String): GroupMade {
        val made = send({ core.createGroupRequest(name) }) { status, body -> core.groupMade(status, body) }
        load()
        return made
    }

    suspend fun join(code: String): GroupJoined {
        val joined = send({ core.joinGroupRequest(code) }) { status, body -> core.groupJoined(status, body) }
        load()
        return joined
    }

    suspend fun detail(id: String): GroupDetail =
        send({ core.groupRequest(id) }) { status, body -> core.group(status, body) }

    suspend fun rename(id: String, name: String): GroupDetail {
        val detail = send({ core.renameGroupRequest(id, name) }) { status, body -> core.group(status, body) }
        load()
        return detail
    }

    suspend fun handOver(id: String, handle: String): GroupDetail {
        val detail = send({ core.handOverGroupRequest(id, handle) }) { status, body -> core.group(status, body) }
        load()
        return detail
    }

    /** A new invite code for a group. The one before it stops working. */
    suspend fun newInvite(id: String): String =
        send({ core.newGroupInviteRequest(id) }) { status, body -> core.groupInvite(status, body) }

    suspend fun remove(handle: String, from: String) {
        send({ core.removeGroupMemberRequest(from, handle) }) { status, body -> core.groupDone(status, body) }
        load()
    }

    suspend fun leave(id: String) {
        send({ core.leaveGroupRequest(id) }) { status, body -> core.groupDone(status, body) }
        load()
    }

    suspend fun deleteGroup(id: String) {
        send({ core.deleteGroupRequest(id) }) { status, body -> core.groupDone(status, body) }
        load()
    }

    // Plans

    suspend fun post(draft: PlanDraft, group: String) {
        send({ core.postPlanRequest(group, draft) }) { status, body -> core.plan(status, body) }
        load()
    }

    suspend fun change(plan: PlanView, draft: PlanDraft) {
        send({ core.changePlanRequest(plan.groupId, plan.id, draft) }) { status, body -> core.plan(status, body) }
        load()
    }

    suspend fun delete(plan: PlanView) {
        send({ core.deletePlanRequest(plan.groupId, plan.id) }) { status, body -> core.groupDone(status, body) }
        load()
    }

    /** Says that the pilot is in on a plan, or takes that back. */
    suspend fun setIn(plan: PlanView, isIn: Boolean) {
        send({ core.planInRequest(plan.groupId, plan.id, isIn) }) { status, body -> core.plan(status, body) }
        load()
    }

    /**
     * Puts a request together, sends it with the account's token and
     * reads the answer.
     */
    private suspend fun <Value> send(request: () -> ServerRequest, read: (UShort, ByteArray) -> Value): Value {
        val answer = account.answer(withContext(background) { request() })
        return withContext(background) { read(answer.status16, answer.body) }
    }

    private val HttpAnswer.status16: UShort get() = status.coerceIn(0, UShort.MAX_VALUE.toInt()).toUShort()
}

/** A plan marked on the map: where, and the place and time under the mark. */
data class PlanPin(val id: String, val position: Coordinate, val label: String) {
    companion object {
        /**
         * Plans at one place as one mark, their words one under the other:
         * drawn apart they lay their words on top of each other. A tap
         * opens the first.
         */
        fun gathered(pins: List<PlanPin>): List<PlanPin> =
            pins.groupBy { it.position }
                .map { (position, here) -> PlanPin(here.first().id, position, here.joinToString("\n") { it.label }) }
    }
}
