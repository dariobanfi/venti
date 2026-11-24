package fly.venti.app.groups

import fly.venti.core.Coordinate
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fly.venti.app.HttpAnswer
import fly.venti.app.account.AccountModel
import fly.venti.core.Core
import fly.venti.core.CommunityDetail
import fly.venti.core.CommunityDraft
import fly.venti.core.CommunityInvite
import fly.venti.core.CommunityStanding
import fly.venti.core.CommunityView
import fly.venti.core.CommunityWords
import fly.venti.core.FoundCommunity
import fly.venti.core.GroupJoined
import fly.venti.core.communityWords
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
 * Groups as communities with a chat (owner decision 26) and their plans for
 * a flying day, on the pilot's Venti server: the pilot's groups as cards,
 * their invites, the public groups near a place or found by name, what an
 * owner, an admin and a member can do, and each group's chat ([chat]).
 *
 * Nothing of it is kept on the phone: the board and the groups are asked
 * for when they are opened and after every change. The core puts the requests together and
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

    // Groups as communities with a chat (owner decision 26)

    /** The words of the groups' screens that no answer carries: the core's. */
    val words: CommunityWords = communityWords()

    /**
     * The pilot's groups as the groups' section shows them, each with its
     * unread messages and its last message. `null` until Venti answered.
     */
    var chats: List<CommunityView>? by mutableStateOf(null)
        private set

    /** Why the pilot's groups are not there, in words for the pilot. */
    var chatsProblem: String? by mutableStateOf(null)
        private set

    /** The pilot's invites into private groups. */
    var invites: List<CommunityInvite> by mutableStateOf(emptyList())
        private set

    /** Asks for the pilot's groups and their invites. */
    suspend fun loadChats() {
        if (!account.isSignedIn) {
            chats = null
            invites = emptyList()
            chatsProblem = null
            return
        }
        try {
            chats = send({ core.groupsRequest() }) { status, body -> core.communities(status, body) }
            chatsProblem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            chatsProblem = account.words(error)
        }
        try {
            invites = send({ core.groupInvitesRequest() }) { status, body -> core.groupInvites(status, body) }
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            // The invites stay as they were: the groups' line says what went wrong.
        }
    }

    /** The public groups near a place, by name or place, or both. */
    suspend fun find(near: Coordinate?, text: String): List<FoundCommunity> =
        send({ core.findGroupsRequest(near, text) }) { status, body -> core.foundGroups(status, body) }

    /** A group with its members, and what the pilot may do in it. */
    suspend fun detail(id: String): CommunityDetail =
        send({ core.groupRequest(id) }) { status, body -> core.community(status, body) }

    /**
     * Makes a group with the pilot as its owner, with its picture if one was
     * chosen. The answer to the making is the group without its members, so
     * the core's `groupMade` reads it and the group is asked for after.
     */
    suspend fun create(draft: CommunityDraft, isPublic: Boolean, picture: Pair<ByteArray, String>?): CommunityDetail {
        val made = send({ core.createCommunityRequest(draft, isPublic) }) { status, body -> core.groupMade(status, body) }
        val id = made.group.id
        val shown = if (picture != null) setPicture(id, picture) else detail(id)
        reloadAll()
        return shown
    }

    /** Changes a group's name, description and home. */
    suspend fun change(id: String, draft: CommunityDraft): CommunityDetail {
        val changed = send({ core.changeCommunityRequest(id, draft) }) { status, body -> core.community(status, body) }
        reloadAll()
        return changed
    }

    /** Makes a group public or private: its owner's. */
    suspend fun setPublic(id: String, isPublic: Boolean): CommunityDetail {
        val changed = send({ core.groupPublicRequest(id, isPublic) }) { status, body -> core.community(status, body) }
        loadChats()
        return changed
    }

    /** Gives a group its picture: the bytes and their media type. */
    suspend fun setPicture(id: String, picture: Pair<ByteArray, String>): CommunityDetail =
        send({ core.groupOwnPictureRequest(id, picture.first, picture.second) }) { status, body -> core.community(status, body) }

    suspend fun removePicture(id: String): CommunityDetail =
        send({ core.removeGroupOwnPictureRequest(id) }) { status, body -> core.community(status, body) }

    /** Takes the pilot into a public group, or one they were invited into. */
    suspend fun enter(id: String): GroupJoined {
        val joined = send({ core.enterGroupRequest(id) }) { status, body -> core.groupJoined(status, body) }
        reloadAll()
        return joined
    }

    /** Declines an invite into a group: by the pilot's own handle. */
    suspend fun decline(id: String) {
        val me = account.account?.handle.orEmpty()
        send({ core.dropGroupInviteRequest(id, me) }) { status, body -> core.groupDone(status, body) }
        loadChats()
    }

    /** Invites a pilot by handle: the core's line of what was done. */
    suspend fun invite(handle: String, to: String): String =
        send({ core.inviteToGroupRequest(to, handle) }) { status, body -> core.groupInviteSent(status, body) }

    /** Makes a member an admin, or an admin a member again: the owner's. */
    suspend fun setAdmin(handle: String, of: String, isAdmin: Boolean) {
        send({ core.groupAdminRequest(of, handle, isAdmin) }) { status, body -> core.groupDone(status, body) }
    }

    suspend fun handOver(id: String, handle: String): CommunityDetail {
        val detail = send({ core.handOverGroupRequest(id, handle) }) { status, body -> core.community(status, body) }
        reloadAll()
        return detail
    }

    /** Mutes a group's pushes, or unmutes them. */
    suspend fun mute(id: String, muted: Boolean): CommunityStanding {
        val standing = send({ core.groupMuteRequest(id, muted) }) { status, body -> core.groupStanding(status, body) }
        loadChats()
        return standing
    }

    suspend fun remove(handle: String, from: String) {
        send({ core.removeGroupMemberRequest(from, handle) }) { status, body -> core.groupDone(status, body) }
        reloadAll()
    }

    suspend fun leave(id: String) {
        send({ core.leaveGroupRequest(id) }) { status, body -> core.groupDone(status, body) }
        reloadAll()
    }

    suspend fun deleteGroup(id: String) {
        send({ core.deleteGroupRequest(id) }) { status, body -> core.groupDone(status, body) }
        reloadAll()
    }

    /** The model of one group's chat, sending with the account's token. */
    fun chat(id: String): ChatModel = ChatModel(id, core, send = { account.answer(it) }, background = background, words = account::words)

    /** Both lists anew after the pilot's standing in a group changed. */
    private suspend fun reloadAll() {
        load()
        loadChats()
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
