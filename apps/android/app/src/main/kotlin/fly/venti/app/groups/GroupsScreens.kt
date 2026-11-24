package fly.venti.app.groups

import android.Manifest
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.venti.design.VentiRadius
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.account.PictureFile
import fly.venti.app.community.FaceSize
import fly.venti.app.community.PilotFace
import fly.venti.app.community.PilotPictures
import fly.venti.app.community.ReportSheet
import fly.venti.app.community.Reported
import fly.venti.app.design.Common
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.map.OverviewMap
import fly.venti.app.today.PilotPosition
import fly.venti.core.CommunityDetail
import fly.venti.core.CommunityDraft
import fly.venti.core.CommunityMember
import fly.venti.core.CommunityRole
import fly.venti.core.CommunityView
import fly.venti.core.Coordinate
import fly.venti.core.FoundCommunity
import fly.venti.core.HomeDraft
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// Groups as communities with a chat (owner decision 26): the pilot's
// groups as cards, the public groups near and found by name or place,
// invites into private groups, a group's page and the form that makes or
// changes one. Every word an answer carries is the core's; the core's
// `communityWords()` has the rest, and `GroupWords` the iPhone's others.

/**
 * The groups' section, a page of the League tab: invites, the pilot's own
 * groups as cards, and the public groups near the phone (or the map's
 * middle) or found by the field.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GroupsScreen(
    groups: GroupsModel,
    account: AccountModel,
    requests: CoroutineScope,
    onBack: () -> Unit,
    /** Opens a group's chat: its identifier and name. */
    onOpenChat: (String, String) -> Unit,
) {
    val words = groups.words
    val context = LocalContext.current
    val position = remember { PilotPosition(context.applicationContext) }
    var typed by remember { mutableStateOf("") }
    var found by remember { mutableStateOf<List<FoundCommunity>?>(null) }
    var findProblem by remember { mutableStateOf<String?>(null) }
    var isFinding by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var isMaking by remember { mutableStateOf(false) }
    var finds by remember { mutableStateOf(0) }
    val scope = rememberCoroutineScope()
    val isSearching = typed.isNotBlank()

    LaunchedEffect(account.isSignedIn) {
        position.readIfAllowed()
        groups.loadChats()
    }
    // The search rests 350 ms before it asks: Venti counts reads. A search
    // by name or place looks everywhere; the list under no text is the
    // groups near.
    LaunchedEffect(typed, position.place, finds, account.isSignedIn) {
        if (!account.isSignedIn) return@LaunchedEffect
        if (isSearching) delay(SEARCH_REST_MILLISECONDS)
        isFinding = true
        try {
            val place = position.place ?: OverviewMap.lastLookedAt
            found = groups.find(if (isSearching) null else place, typed)
            findProblem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            findProblem = account.words(error)
        } finally {
            isFinding = false
        }
    }

    fun act(work: suspend () -> Unit) {
        problem = null
        scope.launch { problem = groups.attempt(work) }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(words.title) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
                actions = {
                    if (account.isSignedIn) {
                        IconButton(onClick = { isMaking = true }, modifier = Modifier.testTag("groups-new")) {
                            Icon(painterResource(R.drawable.ic_add), contentDescription = GroupWords.NEW_GROUP)
                        }
                    }
                },
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(padding).imePadding().testTag("groups"),
            contentPadding = PaddingValues(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            if (!account.isSignedIn) {
                item(key = "signed-out") { GroupsLine(GroupWords.SIGNED_OUT) }
                return@LazyColumn
            }
            item(key = "search") {
                OutlinedTextField(
                    value = typed,
                    onValueChange = { typed = it },
                    placeholder = { Text(words.search) },
                    leadingIcon = { Icon(painterResource(R.drawable.ic_search), contentDescription = null) },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(autoCorrectEnabled = false),
                    modifier = Modifier.fillMaxWidth().testTag("groups-search"),
                )
            }
            if (!isSearching && groups.invites.isNotEmpty()) {
                item(key = "invites-head") { GroupsHeader(GroupWords.INVITES) }
                items(groups.invites, key = { "invite-${it.group.id}" }) { invite ->
                    Card(Modifier.testTag("group-invite")) {
                        Row(
                            Modifier.clearAndSetSemantics { contentDescription = "${invite.words}. ${invite.group.summary}" },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            GroupPicture(invite.group, VentiSpace.s7)
                            Spacer(Modifier.width(VentiSpace.s3))
                            Column(Modifier.weight(1f)) {
                                Text(invite.words, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
                                Text(invite.group.summary, style = MaterialTheme.typography.bodySmall, color = Venti.colours.textSecondary)
                            }
                        }
                        Row(Modifier.padding(top = VentiSpace.s3), horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                            Button(
                                onClick = { act { groups.enter(invite.group.id) } },
                                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("group-invite-accept"),
                            ) { Text(words.join) }
                            TextButton(
                                onClick = { act { groups.decline(invite.group.id) } },
                                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("group-invite-decline"),
                            ) { Text(GroupWords.DECLINE) }
                        }
                    }
                }
            }
            if (!isSearching) {
                item(key = "mine-head") { GroupsHeader(words.yours) }
                val chats = groups.chats
                val chatsProblem = groups.chatsProblem
                when {
                    chats != null -> {
                        if (chats.isEmpty()) item(key = "mine-none") { GroupsLine(GroupWords.NONE_YET, Modifier.testTag("groups-none")) }
                        items(chats, key = { "mine-${it.id}" }) { group ->
                            CommunityCard(group, Modifier.testTag("community-card"), onClick = { onOpenChat(group.id, group.name) })
                        }
                    }
                    chatsProblem != null -> item(key = "mine-problem") { GroupsLine(chatsProblem, isProblem = true) }
                    else -> item(key = "mine-loading") { GroupsLoading() }
                }
            }
            item(key = "near-head") { GroupsHeader(if (isSearching) GroupWords.FOUND else words.nearYou) }
            val shown = found
            when {
                shown != null -> {
                    if (shown.isEmpty() && !isFinding) {
                        item(key = "near-none") { GroupsLine(words.noneFound, Modifier.testTag("groups-none-found")) }
                    }
                    items(shown, key = { "found-${it.group.id}" }) { one ->
                        FoundCommunityCard(one, groups, onOpen = { onOpenChat(one.group.id, one.group.name) }) {
                            problem = groups.attempt { groups.enter(one.group.id) }
                            finds++
                        }
                    }
                }
                findProblem != null -> item(key = "near-problem") { GroupsLine(findProblem.orEmpty(), isProblem = true) }
                else -> item(key = "near-loading") { GroupsLoading() }
            }
            problem?.let { said -> item(key = "problem") { GroupsLine(said, Modifier.testTag("groups-problem"), isProblem = true) } }
        }
    }

    if (isMaking) {
        GroupFormSheet(
            editing = null,
            groups = groups,
            requests = requests,
            onDismiss = { isMaking = false },
            onSaved = { made ->
                isMaking = false
                onOpenChat(made.group.id, made.group.name)
            },
        )
    }
}

/** How long the search waits after the last letter typed. */
private const val SEARCH_REST_MILLISECONDS = 350L

/** A card of the pack: the surface, its line round it and its corners. */
@Composable
internal fun Card(modifier: Modifier = Modifier, onClick: (() -> Unit)? = null, content: @Composable () -> Unit) {
    val colours = Venti.colours
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = colours.surface,
        border = BorderStroke(1.dp, colours.border),
    ) {
        Column(
            (if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
                .padding(VentiSpace.s4),
        ) { content() }
    }
}

/**
 * A group as a card: its picture or initials, its name, its place and how
 * many members, the last message (else its description), and how many
 * messages are unread. One element for TalkBack: the name, then the rest.
 */
@Composable
fun CommunityCard(
    group: CommunityView,
    modifier: Modifier = Modifier,
    /** How far its home is, for a group found near a place. */
    distance: String? = null,
    onClick: () -> Unit,
    /** What stands at the card's foot, beside the link: "Join". */
    foot: (@Composable () -> Unit)? = null,
) {
    val colours = Venti.colours
    val summary = listOfNotNull(group.summary, distance).joinToString(" · ")
    val line = group.lastMessage ?: group.description
    // The tag goes with the link, which is one element with the name: a
    // test finds the card by both.
    Card {
        Row(
            modifier
                .fillMaxWidth()
                .clickable(role = Role.Button, onClick = onClick)
                .clearAndSetSemantics {
                    contentDescription = group.name
                    stateDescription = listOfNotNull(summary, line, group.unreadBadge?.let(GroupWords::unread)).joinToString(". ")
                },
            verticalAlignment = Alignment.Top,
        ) {
            GroupPicture(group, VentiSpace.s7)
            Spacer(Modifier.width(VentiSpace.s3))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(group.name, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                    group.unreadBadge?.let { badge -> UnreadBadge(badge) }
                }
                Text(summary, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                if (line != null) {
                    Text(
                        line,
                        style = MaterialTheme.typography.bodyMedium,
                        color = if (group.lastMessage != null) colours.text else colours.textSecondary,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        foot?.let { button ->
            Row(Modifier.fillMaxWidth().padding(top = VentiSpace.s2), horizontalArrangement = Arrangement.End) { button() }
        }
    }
}

/** How many messages are unread, on the primary colour. */
@Composable
private fun UnreadBadge(badge: String) {
    val colours = Venti.colours
    Box(
        Modifier
            .testTag("group-unread")
            .heightIn(min = VentiSpace.s5)
            .widthIn(min = VentiSpace.s5)
            .background(colours.primary, CircleShape)
            .padding(horizontal = VentiSpace.s2),
        contentAlignment = Alignment.Center,
    ) {
        Text(badge, style = MaterialTheme.typography.labelMedium.numbers, fontWeight = FontWeight.SemiBold, color = colours.onPrimary)
    }
}

/** A public group found: its card, which opens its chat, and "Join" while the pilot is not in it. */
@Composable
private fun FoundCommunityCard(found: FoundCommunity, groups: GroupsModel, onOpen: () -> Unit, join: suspend () -> Unit) {
    val scope = rememberCoroutineScope()
    var isJoining by remember { mutableStateOf(false) }
    CommunityCard(
        found.group,
        Modifier.testTag("found-group"),
        distance = found.distance,
        onClick = onOpen,
        foot = if (found.isMember) {
            null
        } else {
            {
                OutlinedButton(
                    onClick = {
                        isJoining = true
                        scope.launch {
                            join()
                            isJoining = false
                        }
                    },
                    enabled = !isJoining,
                    modifier = Modifier
                        .heightIn(min = VentiSize.touch)
                        .testTag("group-join")
                        .semantics { stateDescription = found.group.name },
                ) { Text(groups.words.join) }
            }
        },
    )
}

/** A group's picture, square with the pack's corners, or its initials where it has none. Decoration: its name says who. */
@Composable
fun GroupPicture(group: CommunityView, size: Dp) {
    val colours = Venti.colours
    val path = group.picturePath
    var picture by remember(path) { mutableStateOf<ImageBitmap?>(path?.let(PilotPictures::kept)) }
    LaunchedEffect(path) {
        if (path != null && picture == null) picture = PilotPictures.fetch(path)
    }
    val shape = MaterialTheme.shapes.small
    Box(
        Modifier
            .size(size)
            .clip(shape)
            .background(colours.surfaceRaised)
            .border(1.dp, colours.border, shape)
            .clearAndSetSemantics {},
        contentAlignment = Alignment.Center,
    ) {
        val shown = picture
        if (shown != null) {
            Image(shown, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.size(size))
        } else {
            Text(
                PilotPictures.initials(group.name),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                color = colours.text,
                maxLines = 1,
            )
        }
    }
}

@Composable
internal fun GroupsHeader(title: String, modifier: Modifier = Modifier) {
    Text(
        title,
        style = MaterialTheme.typography.titleLarge,
        modifier = modifier.padding(top = VentiSpace.s3).semantics { heading() },
    )
}

/** A sentence in place of what is not there, or what went wrong, with the pack's caution sign. */
@Composable
internal fun GroupsLine(words: String, modifier: Modifier = Modifier, isProblem: Boolean = false) {
    val colours = Venti.colours
    Row(modifier, verticalAlignment = Alignment.Top) {
        if (isProblem) {
            Icon(
                painterResource(R.drawable.venti_caution),
                contentDescription = null,
                tint = colours.caution,
                modifier = Modifier.size(VentiSize.icon),
            )
            Spacer(Modifier.width(VentiSpace.s2))
        }
        Text(words, style = MaterialTheme.typography.bodyLarge, color = colours.textSecondary)
    }
}

@Composable
internal fun GroupsLoading() {
    Box(Modifier.fillMaxWidth().heightIn(min = VentiSize.touch), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(Modifier.size(VentiSpace.s6))
    }
}

// A group's page

/** Something on a group's page that is asked about before it is done. */
private enum class Asking { Leave, Delete }

/**
 * One group: its description, home and members with their roles; for its
 * owner and admins inviting by handle, the members' roles, removing members
 * and changing the group; for every member muting and leaving.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GroupPage(
    id: String,
    groups: GroupsModel,
    account: AccountModel,
    requests: CoroutineScope,
    onBack: () -> Unit,
    /** The pilot left the group or deleted it: there is nothing behind this page. */
    onGone: () -> Unit,
) {
    val words = groups.words
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var detail by remember { mutableStateOf<CommunityDetail?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    var outcome by remember { mutableStateOf<String?>(null) }
    var isWorking by remember { mutableStateOf(false) }
    var isInviting by remember { mutableStateOf(false) }
    var handle by remember { mutableStateOf("") }
    var isEditing by remember { mutableStateOf(false) }
    var acting by remember { mutableStateOf<CommunityMember?>(null) }
    var asking by remember { mutableStateOf<Asking?>(null) }
    var reporting by remember { mutableStateOf<Reported?>(null) }

    suspend fun load() {
        try {
            detail = groups.detail(id)
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = account.words(error)
        }
    }

    fun act(thenGone: Boolean = false, work: suspend () -> Unit) {
        if (isWorking) return
        isWorking = true
        problem = null
        outcome = null
        scope.launch {
            val failed = groups.attempt(work)
            isWorking = false
            if (failed != null) problem = failed else if (thenGone) onGone()
        }
    }

    LaunchedEffect(id) { load() }

    val choosePicture = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { photo ->
        if (photo == null) return@rememberLauncherForActivityResult
        act {
            val prepared = withContext(Dispatchers.IO) { runCatching { PictureFile.prepare(context, photo) }.getOrNull() }
                ?: throw IllegalArgumentException(PictureFile.UNREADABLE)
            detail = groups.setPicture(id, prepared)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(detail?.group?.name ?: words.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3)
                .testTag("group-page"),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            outcome?.let { said ->
                Text(said, style = MaterialTheme.typography.bodyLarge, color = Venti.colours.good, modifier = Modifier.testTag("group-outcome"))
            }
            problem?.let { said ->
                Text(said, style = MaterialTheme.typography.bodyLarge, color = Venti.colours.danger, modifier = Modifier.testTag("group-problem"))
            }
            val shown = detail
            if (shown == null) {
                if (problem == null) GroupsLoading()
                return@Column
            }
            val group = shown.group
            val colours = Venti.colours
            // The head: picture, name, kind and summary, description and home.
            Row(
                Modifier.clearAndSetSemantics { contentDescription = "${group.name}. ${group.kindWords} · ${group.summary}" },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                GroupPicture(group, VentiSpace.s8)
                Spacer(Modifier.width(VentiSpace.s3))
                Column {
                    Text(group.name, style = MaterialTheme.typography.titleLarge)
                    Text("${group.kindWords} · ${group.summary}", style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                }
            }
            group.description?.let { said ->
                Text(said, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("group-description"))
            }
            val place = group.place
            val radius = group.radiusKm
            if (place != null && radius != null) {
                Text(
                    "${GroupWords.HOME}: ${GroupWords.home(radius, place)}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = colours.textSecondary,
                    modifier = Modifier.testTag("group-home"),
                )
            }

            GroupsHeader(GroupWords.MEMBERS)
            Group {
                for (member in shown.members) {
                    row {
                        val canAct = !member.isMe && canAct(member, group.myRole)
                        MemberRow(member, canAct) { acting = member }
                    }
                }
            }

            if (shown.canModerate) {
                Group(Modifier.padding(top = VentiSpace.s3)) {
                    row {
                        RowButton(GroupWords.INVITE_BY_HANDLE, R.drawable.ic_add, Modifier.testTag("group-invite")) {
                            handle = ""
                            isInviting = true
                        }
                    }
                    row { RowButton(GroupWords.EDIT, R.drawable.ic_edit_square, Modifier.testTag("group-edit")) { isEditing = true } }
                    row {
                        RowButton(
                            if (group.picturePath == null) GroupWords.GIVE_PICTURE else GroupWords.CHANGE_PICTURE,
                            R.drawable.ic_image,
                            Modifier.testTag("group-picture"),
                        ) { choosePicture.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }
                    }
                    if (group.picturePath != null) {
                        row {
                            RowButton(GroupWords.REMOVE_PICTURE, R.drawable.ic_close, Modifier.testTag("group-picture-remove"), danger = true) {
                                act { detail = groups.removePicture(id) }
                            }
                        }
                    }
                    if (group.myRole == CommunityRole.OWNER) {
                        row {
                            SwitchRow(words.public, group.isPublic, Modifier.testTag("group-public"), enabled = !isWorking) { isPublic ->
                                act { detail = groups.setPublic(id, isPublic) }
                            }
                        }
                    }
                }
                SectionFooter(if (group.isPublic) GroupWords.PUBLIC_NOTE else GroupWords.PRIVATE_NOTE)
            }

            if (shown.canPost) {
                Group(Modifier.padding(top = VentiSpace.s3)) {
                    row {
                        SwitchRow(words.mute, group.muted, Modifier.testTag("group-mute"), enabled = !isWorking) { muted ->
                            act {
                                val standing = groups.mute(id, muted)
                                detail = detail?.let { it.copy(group = it.group.copy(muted = standing.muted)) }
                            }
                        }
                    }
                }
                SectionFooter(if (group.isPublic) GroupWords.MUTE_PUBLIC else GroupWords.MUTE_PRIVATE)
                Group(Modifier.padding(top = VentiSpace.s3)) {
                    row { RowButton(words.leave, R.drawable.ic_cancel, Modifier.testTag("group-leave"), danger = true) { asking = Asking.Leave } }
                    if (group.myRole == CommunityRole.OWNER) {
                        row { RowButton(GroupWords.DELETE, R.drawable.ic_delete, Modifier.testTag("group-delete"), danger = true) { asking = Asking.Delete } }
                    }
                }
                if (group.myRole == CommunityRole.OWNER) SectionFooter(GroupWords.HAND_ON_FIRST)
            } else if (shown.canJoin) {
                Button(
                    onClick = { act { groups.enter(id); load() } },
                    enabled = !isWorking,
                    modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("group-page-join"),
                ) { Text(words.join) }
            }
            if (group.myRole != CommunityRole.OWNER) {
                Group(Modifier.padding(top = VentiSpace.s3)) {
                    row { RowButton(GroupWords.REPORT_GROUP, R.drawable.ic_flag, Modifier.testTag("group-report")) { reporting = Reported.group(id) } }
                }
            }
            Spacer(Modifier.heightIn(min = VentiSpace.s6))
        }
    }

    if (isInviting) {
        AlertDialog(
            onDismissRequest = { isInviting = false },
            title = { Text(GroupWords.INVITE_A_PILOT) },
            text = {
                Column {
                    Text(GroupWords.BY_THE_HANDLE, style = MaterialTheme.typography.bodyMedium)
                    OutlinedTextField(
                        value = handle,
                        onValueChange = { handle = it },
                        placeholder = { Text(GroupWords.HANDLE) },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
                        modifier = Modifier.fillMaxWidth().padding(top = VentiSpace.s3).testTag("group-invite-handle"),
                    )
                }
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        isInviting = false
                        val typed = handle
                        act { outcome = groups.invite(typed, id) }
                    },
                    modifier = Modifier.testTag("group-invite-send"),
                ) { Text(words.invite) }
            },
            dismissButton = { TextButton(onClick = { isInviting = false }) { Text(Common.words.cancel) } },
        )
    }

    acting?.let { member ->
        val isOwner = detail?.group?.myRole == CommunityRole.OWNER
        AlertDialog(
            onDismissRequest = { acting = null },
            title = { Text(member.name) },
            text = {
                Column {
                    if (isOwner) {
                        val admin = member.role == CommunityRole.ADMIN
                        TextButton(
                            onClick = {
                                acting = null
                                act { groups.setAdmin(member.handle, id, !admin); load() }
                            },
                            modifier = Modifier.fillMaxWidth().testTag("member-admin"),
                        ) { Text(if (admin) GroupWords.MAKE_MEMBER else GroupWords.MAKE_ADMIN) }
                        TextButton(
                            onClick = {
                                acting = null
                                act { detail = groups.handOver(id, member.handle) }
                            },
                            modifier = Modifier.fillMaxWidth().testTag("member-owner"),
                        ) { Text(GroupWords.owner(member.name)) }
                    }
                    if (isOwner || member.role == CommunityRole.MEMBER) {
                        TextButton(
                            onClick = {
                                acting = null
                                act { groups.remove(member.handle, id); load() }
                            },
                            modifier = Modifier.fillMaxWidth().testTag("member-remove"),
                        ) { Text(GroupWords.REMOVE_MEMBER, color = Venti.colours.danger) }
                    }
                }
            },
            confirmButton = {},
            dismissButton = { TextButton(onClick = { acting = null }) { Text(Common.words.cancel) } },
        )
    }

    asking?.let { question ->
        AlertDialog(
            onDismissRequest = { asking = null },
            title = { Text(if (question == Asking.Delete) GroupWords.DELETE_QUESTION else GroupWords.LEAVE_QUESTION) },
            text = { Text(if (question == Asking.Delete) GroupWords.DELETE_MEANS else GroupWords.LEAVE_MEANS) },
            confirmButton = {
                TextButton(
                    onClick = {
                        asking = null
                        act(thenGone = true) { if (question == Asking.Delete) groups.deleteGroup(id) else groups.leave(id) }
                    },
                    modifier = Modifier.testTag("group-confirm"),
                ) { Text(if (question == Asking.Delete) GroupWords.DELETE else words.leave, color = Venti.colours.danger) }
            },
            dismissButton = { TextButton(onClick = { asking = null }) { Text(Common.words.cancel) } },
        )
    }

    if (isEditing) {
        detail?.let { shown ->
            GroupFormSheet(shown, groups, requests, onDismiss = { isEditing = false }) { changed ->
                detail = changed
                isEditing = false
            }
        }
    }

    reporting?.let { reported ->
        ReportSheet(reported, account, account.account?.server, requests, onDismiss = { reporting = null })
    }
}

/** Whether the pilot may do something with a member: the owner with anyone, an admin with a member who is no admin. */
private fun canAct(member: CommunityMember, role: CommunityRole): Boolean = when (role) {
    CommunityRole.OWNER -> true
    CommunityRole.ADMIN -> member.role == CommunityRole.MEMBER
    else -> false
}

/** A member in a line: face, name and handle, the core's word for the role. One element for TalkBack. */
@Composable
private fun MemberRow(member: CommunityMember, canAct: Boolean, onClick: () -> Unit) {
    val colours = Venti.colours
    val name = if (member.isMe) GroupWords.you(member.name) else member.name
    Row(
        Modifier
            .fillMaxWidth()
            .testTag("group-member")
            .then(if (canAct) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2)
            .clearAndSetSemantics {
                contentDescription = name
                stateDescription = listOf("@${member.handle}", member.roleWords).filter { it.isNotEmpty() }.joinToString(", ")
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PilotFace(member.name, PilotPictures.of(member.handle), size = FaceSize.Small)
        Spacer(Modifier.width(VentiSpace.s3))
        Column(Modifier.weight(1f)) {
            Text(name, style = MaterialTheme.typography.bodyLarge)
            Text("@${member.handle}", style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
        }
        if (member.roleWords.isNotEmpty()) {
            Text(member.roleWords, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium, color = colours.textSecondary)
        }
    }
}

/** A row that does something, with its sign. */
@Composable
private fun RowButton(title: String, icon: Int, modifier: Modifier = Modifier, danger: Boolean = false, onClick: () -> Unit) {
    val colour = if (danger) Venti.colours.danger else Venti.colours.link
    Row(
        modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = colour, modifier = Modifier.size(VentiSize.icon))
        Spacer(Modifier.width(VentiSpace.s3))
        Text(title, style = MaterialTheme.typography.bodyLarge, color = colour)
    }
}

/** A row with a switch at its end; the whole row switches it. */
@Composable
private fun SwitchRow(title: String, isOn: Boolean, modifier: Modifier = Modifier, enabled: Boolean = true, onChange: (Boolean) -> Unit) {
    Row(
        modifier
            .fillMaxWidth()
            .toggleable(value = isOn, enabled = enabled, role = Role.Switch, onValueChange = onChange)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Switch(checked = isOn, onCheckedChange = null, enabled = enabled)
    }
}

// Making and changing a group

/** Where a group's home is. */
private enum class Home { Kept, Map, Position, None }

/**
 * The form for a group: a new one (name, public or private, description,
 * home, picture), or the name, description and home of one the pilot owns
 * or is an admin of. A sheet over the groups' section or a group's page.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GroupFormSheet(
    editing: CommunityDetail?,
    groups: GroupsModel,
    requests: CoroutineScope,
    onDismiss: () -> Unit,
    /** Called with the group as Venti has it after saving. */
    onSaved: (CommunityDetail) -> Unit,
) {
    val words = groups.words
    val context = LocalContext.current
    val position = remember { PilotPosition(context.applicationContext) }
    val shown = editing?.group
    var name by remember { mutableStateOf(shown?.name.orEmpty()) }
    var isPublic by remember { mutableStateOf(true) }
    var description by remember { mutableStateOf(shown?.description.orEmpty()) }
    var home by remember { mutableStateOf(if (shown == null) Home.Map else if (shown.center == null) Home.None else Home.Kept) }
    var place by remember { mutableStateOf(shown?.place.orEmpty()) }
    var radius by remember { mutableStateOf(shown?.radiusKm ?: DEFAULT_RADIUS) }
    var picture by remember { mutableStateOf<Pair<ByteArray, String>?>(null) }
    var isSaving by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    val homes = listOfNotNull(Home.Kept.takeIf { shown?.center != null }, Home.Map, Home.Position, Home.None)

    val askForPosition = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { granted ->
        position.isDenied = granted.values.none { it }
        position.readIfAllowed()
    }
    val choosePicture = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { photo ->
        if (photo == null) return@rememberLauncherForActivityResult
        requests.launch {
            val prepared = withContext(Dispatchers.IO) { runCatching { PictureFile.prepare(context, photo) }.getOrNull() }
            if (prepared == null) problem = PictureFile.UNREADABLE else picture = prepared
        }
    }
    LaunchedEffect(home) {
        if (home == Home.Position) {
            if (position.isAllowed) {
                position.readIfAllowed()
            } else {
                askForPosition.launch(arrayOf(Manifest.permission.ACCESS_COARSE_LOCATION, Manifest.permission.ACCESS_FINE_LOCATION))
            }
        }
    }

    val center: Coordinate? = when (home) {
        Home.Map -> OverviewMap.lastLookedAt
        Home.Position -> position.place
        Home.Kept -> shown?.center
        Home.None -> null
    }

    fun save() {
        if (isSaving) return
        problem = null
        if (home == Home.Position && center == null) {
            problem = GroupWords.POSITION_UNKNOWN
            return
        }
        val draft = CommunityDraft(name, description, center?.let { HomeDraft(it, radius, place) })
        isSaving = true
        requests.launch {
            try {
                val saved = if (editing != null) groups.change(editing.group.id, draft) else groups.create(draft, isPublic, picture)
                onSaved(saved)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Exception) {
                problem = groups.attempt { throw error }
            } finally {
                isSaving = false
            }
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Box(Modifier.fillMaxHeight().imePadding()) {
            Column {
                Row(
                    Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        if (editing == null) GroupWords.NEW_GROUP else GroupWords.CHANGE_GROUP,
                        style = MaterialTheme.typography.titleLarge,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = onDismiss) { Text(Common.words.cancel) }
                    TextButton(onClick = ::save, enabled = !isSaving, modifier = Modifier.testTag("group-form-save")) { Text(GroupWords.SAVE) }
                }
                Column(
                    Modifier
                        .fillMaxHeight()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = VentiSpace.s4)
                        .padding(bottom = VentiSpace.s6)
                        .testTag("group-form"),
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                ) {
                    problem?.let { said ->
                        Text(said, color = Venti.colours.danger, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("group-form-problem"))
                    }
                    OutlinedTextField(
                        value = name,
                        onValueChange = { name = it },
                        label = { Text(GroupWords.NAME) },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words),
                        modifier = Modifier.fillMaxWidth().testTag("group-form-name"),
                    )
                    OutlinedTextField(
                        value = description,
                        onValueChange = { description = it },
                        label = { Text(GroupWords.ABOUT_FIELD) },
                        minLines = 2,
                        maxLines = 6,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        modifier = Modifier.fillMaxWidth().testTag("group-form-description"),
                    )
                    if (editing == null) {
                        SegmentedPills(
                            listOf(words.public, words.private),
                            chosen = if (isPublic) 0 else 1,
                            modifier = Modifier.testTag("group-form-public"),
                        ) { isPublic = it == 0 }
                        SectionFooter(if (isPublic) GroupWords.FORM_PUBLIC_NOTE else GroupWords.FORM_PRIVATE_NOTE)
                    }
                    GroupsHeader(GroupWords.HOME)
                    SegmentedPills(
                        homes.map { choice ->
                            when (choice) {
                                Home.Kept -> GroupWords.HOME_AS_IT_IS
                                Home.Map -> GroupWords.HOME_MAP
                                Home.Position -> GroupWords.HOME_POSITION
                                Home.None -> GroupWords.HOME_NONE
                            }
                        },
                        chosen = homes.indexOf(home).coerceAtLeast(0),
                        fitToWords = true,
                        modifier = Modifier.testTag("group-form-home"),
                    ) { home = homes[it] }
                    if (home != Home.None) {
                        OutlinedTextField(
                            value = place,
                            onValueChange = { place = it },
                            label = { Text(GroupWords.PLACE_FIELD) },
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words),
                            modifier = Modifier.fillMaxWidth().testTag("group-form-place"),
                        )
                        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.testTag("group-form-radius")) {
                            Text(GroupWords.around(radius), style = MaterialTheme.typography.bodyLarge.numbers, modifier = Modifier.weight(1f))
                            IconButton(
                                onClick = { radius = (radius - GroupWords.RADIUS_STEP).coerceAtLeast(GroupWords.LEAST_RADIUS) },
                                enabled = radius > GroupWords.LEAST_RADIUS,
                                modifier = Modifier.testTag("group-form-radius-less"),
                            ) { Icon(painterResource(R.drawable.ic_remove), contentDescription = "Less") }
                            IconButton(
                                onClick = { radius = (radius + GroupWords.RADIUS_STEP).coerceAtMost(GroupWords.MOST_RADIUS) },
                                enabled = radius < GroupWords.MOST_RADIUS,
                                modifier = Modifier.testTag("group-form-radius-more"),
                            ) { Icon(painterResource(R.drawable.ic_add), contentDescription = "More") }
                        }
                    }
                    if (home == Home.Position && position.place == null) {
                        GroupsLine(if (position.isDenied) GroupWords.NO_POSITION else GroupWords.WAITING_FOR_POSITION)
                    }
                    SectionFooter(GroupWords.HOME_NOTE)
                    if (editing == null) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            OutlinedButton(
                                onClick = { choosePicture.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
                                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("group-form-picture"),
                            ) {
                                Icon(painterResource(R.drawable.ic_image), contentDescription = null)
                                Spacer(Modifier.width(VentiSpace.s2))
                                Text(if (picture == null) GroupWords.GIVE_PICTURE else GroupWords.ANOTHER_PICTURE)
                            }
                        }
                        SectionFooter(GroupWords.PICTURE_NOTE)
                    }
                }
            }
        }
    }
}

/** A new group's home reaches this far at first, kilometres. */
private const val DEFAULT_RADIUS = 30.0
