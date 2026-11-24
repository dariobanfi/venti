package fly.venti.app.groups

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import android.graphics.BitmapFactory
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.account.PictureFile
import fly.venti.app.community.FaceSize
import fly.venti.app.community.OpenPlanOpening
import fly.venti.app.community.OpenPlanSheet
import fly.venti.app.community.OpenPlansModel
import fly.venti.app.community.PilotFace
import fly.venti.app.community.PilotPictures
import fly.venti.app.community.PlanCard
import fly.venti.app.community.ReportSheet
import fly.venti.app.community.Reported
import fly.venti.app.design.ChipTick
import fly.venti.app.design.Common
import fly.venti.app.design.Format
import fly.venti.app.design.Venti
import fly.venti.app.logbook.TrackThumbnail
import fly.venti.app.waypoints.WaypointsModel
import fly.venti.app.wind.WindModel
import fly.venti.core.CardView
import fly.venti.core.ChatMessage
import fly.venti.core.CommunityDetail
import fly.venti.core.messageLinks
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.OffsetDateTime

/**
 * A group's chat, a page of the League tab: the messages, the newest at
 * the bottom, older pages as the pilot scrolls up, what is new every five
 * seconds while the page is open; the field to write in with up to four
 * photos; links to Venti's flights, sites, open plans and pilots shown as
 * cards that open them. A long press on a message removes it (its author,
 * the group's owner and admins) or reports it. As the iPhone's
 * `GroupChatScreen`.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(
    id: String,
    name: String,
    groups: GroupsModel,
    account: AccountModel,
    openPlans: OpenPlansModel,
    requests: CoroutineScope,
    waypoints: WaypointsModel?,
    wind: WindModel?,
    onBack: () -> Unit,
    /** The group's page. */
    onInfo: () -> Unit,
    /** Read again once the chat is left: the tab's count and the groups. */
    onLeft: () -> Unit,
    onOpenFlight: (String) -> Unit,
    /** A site by its identifier, and its name until it is read. */
    onOpenSite: (String, String) -> Unit,
    onOpenPilot: (String, String) -> Unit,
    onSignIn: () -> Unit,
) {
    val words = groups.words
    val chat = remember(id) { groups.chat(id) }
    var detail by remember { mutableStateOf<CommunityDetail?>(null) }
    var isJoining by remember { mutableStateOf(false) }
    var joinProblem by remember { mutableStateOf<String?>(null) }
    var actionProblem by remember { mutableStateOf<String?>(null) }
    var reporting by remember { mutableStateOf<Reported?>(null) }
    var isAddingPlan by remember { mutableStateOf(false) }
    var openPlan by remember { mutableStateOf<String?>(null) }
    var plan by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    suspend fun loadDetail() {
        detail = try {
            groups.detail(id)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            detail
        }
    }

    // The first page, then what is new every five seconds, until the page
    // goes: the effect's coroutine is cancelled with it.
    LaunchedEffect(id) {
        launch { loadDetail() }
        // The board, for the plan posts' "I'm in" and the plan form.
        launch { groups.load() }
        chat.run()
    }
    DisposableEffect(id) { onDispose { onLeft() } }

    val messages = chat.log.messages
    val newestFirst = remember(messages) { messages.asReversed() }
    val list = rememberLazyListState()
    // The newest stays in view as messages come.
    LaunchedEffect(messages.lastOrNull()?.id) {
        if (messages.isNotEmpty()) list.animateScrollToItem(0)
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(detail?.group?.name ?: name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
                actions = {
                    if (detail?.canPost == true) {
                        IconButton(onClick = { isAddingPlan = true }, modifier = Modifier.testTag("chat-new-plan")) {
                            Icon(painterResource(R.drawable.ic_calendar_month), contentDescription = GroupWords.NEW_PLAN)
                        }
                    }
                    IconButton(onClick = onInfo, modifier = Modifier.testTag("chat-info")) {
                        Icon(painterResource(R.drawable.ic_info), contentDescription = GroupWords.ABOUT)
                    }
                },
            )
        },
        bottomBar = {
            val shown = detail
            Box(Modifier.imePadding().navigationBarsPadding()) {
                when {
                    shown == null -> Unit
                    shown.canPost -> ChatComposer(chat, groups, account)
                    shown.canJoin -> JoinBar(words.join, joinProblem, isJoining) {
                        isJoining = true
                        joinProblem = null
                        scope.launch {
                            joinProblem = groups.attempt {
                                groups.enter(id)
                                loadDetail()
                                // Joining counts what was there as read.
                                chat.loadNewest()
                            }
                            isJoining = false
                        }
                    }
                }
            }
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding).background(Venti.colours.bg)) {
            LazyColumn(
                state = list,
                reverseLayout = true,
                modifier = Modifier.fillMaxSize().testTag("chat"),
                contentPadding = PaddingValues(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
            ) {
                (chat.problem ?: actionProblem)?.let { said ->
                    item(key = "problem") { GroupsLine(said, Modifier.testTag("chat-problem"), isProblem = true) }
                }
                items(newestFirst, key = { it.id }) { message ->
                    LaunchedEffect(message.id, detail?.canPost) {
                        if (detail?.canPost == true) chat.saw(message)
                    }
                    MessageRow(
                        message,
                        groupId = id,
                        canRemove = message.removedWords == null && (message.isMine || detail?.canModerate == true),
                        groups = groups,
                        account = account,
                        openPlans = openPlans,
                        onRemove = {
                            actionProblem = null
                            scope.launch { actionProblem = groups.attempt { chat.remove(message) } }
                        },
                        onReport = { reporting = it },
                        onOpenCard = { card ->
                            when (card.kind) {
                                "flight" -> onOpenFlight(card.id)
                                "site" -> onOpenSite(card.id, card.title)
                                "open_plan" -> openPlan = card.id
                                "pilot" -> onOpenPilot(card.id, card.title)
                            }
                        },
                        onOpenPlan = { plan = it },
                    )
                }
                if (chat.hasLoaded && messages.isEmpty()) {
                    item(key = "empty") {
                        Box(Modifier.fillMaxWidth().padding(vertical = VentiSpace.s6), contentAlignment = Alignment.Center) {
                            GroupsLine(words.emptyChat, Modifier.testTag("chat-empty"))
                        }
                    }
                }
                if (chat.log.older != null) {
                    item(key = "older") {
                        LaunchedEffect(chat.log.older) { chat.loadOlder() }
                        Box(Modifier.testTag("chat-older")) { GroupsLoading() }
                    }
                }
            }
            if (!chat.hasLoaded && chat.problem == null) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { GroupsLoading() }
            }
        }
    }

    if (isAddingPlan || plan != null) {
        PlansSheet(
            groups,
            account,
            requests,
            onDismiss = {
                isAddingPlan = false
                plan = null
            },
            waypoints = waypoints,
            wind = wind,
            startAt = plan,
            opensNewPlan = isAddingPlan,
            inGroup = id.takeIf { isAddingPlan },
        )
    }
    openPlan?.let { shown ->
        androidx.compose.runtime.key(shown) {
            OpenPlanSheet(
                openPlans,
                account,
                requests,
                OpenPlanOpening.Plan(shown),
                onDismiss = { openPlan = null },
                onSignIn = onSignIn,
                onOpenPilot = onOpenPilot,
                wind = wind,
            )
        }
    }
    reporting?.let { reported ->
        ReportSheet(reported, account, account.account?.server, requests, onDismiss = { reporting = null })
    }
}

/** "Join" in place of the field, for a pilot who reads a public group they are not in. */
@Composable
private fun JoinBar(join: String, problem: String?, isJoining: Boolean, onJoin: () -> Unit) {
    val colours = Venti.colours
    Surface(color = colours.surface) {
        Column {
            HorizontalDivider(color = colours.border)
            Column(Modifier.padding(VentiSpace.s3), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                problem?.let { GroupsLine(it, isProblem = true) }
                Button(
                    onClick = onJoin,
                    enabled = !isJoining,
                    modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("chat-join"),
                ) { Text(join) }
            }
        }
    }
}

// A message

/**
 * One message: who wrote it and when, its text, its pictures, its cards
 * and, for a plan of the group, the plan with "I'm in". The pilot's own
 * stand at the right on the primary ground.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun MessageRow(
    message: ChatMessage,
    groupId: String,
    canRemove: Boolean,
    groups: GroupsModel,
    account: AccountModel,
    openPlans: OpenPlansModel,
    onRemove: () -> Unit,
    onReport: (Reported) -> Unit,
    onOpenCard: (CardView) -> Unit,
    onOpenPlan: (String) -> Unit,
) {
    val colours = Venti.colours
    val mine = message.isMine
    var menu by remember { mutableStateOf(false) }
    var isConfirmingRemoval by remember { mutableStateOf(false) }
    val canReport = !mine && message.removedWords == null
    val when_ = ChatWords.posted(message.postedAt)
    Column(
        Modifier
            .fillMaxWidth()
            .padding(start = if (mine) VentiSpace.s6 else 0.dp, end = if (mine) 0.dp else VentiSpace.s6)
            .testTag("chat-message")
            .combinedClickable(
                enabled = canRemove || canReport,
                onClick = {},
                onLongClick = { menu = true },
                onLongClickLabel = Common.words.more,
            ),
        horizontalAlignment = if (mine) Alignment.End else Alignment.Start,
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
    ) {
        Text(
            if (mine) when_ else "${message.authorName} · $when_",
            style = MaterialTheme.typography.labelMedium,
            color = colours.textSecondary,
        )
        val removed = message.removedWords
        if (removed != null) {
            Text(
                removed,
                style = MaterialTheme.typography.bodyLarge,
                fontStyle = FontStyle.Italic,
                color = colours.textMuted,
                modifier = Modifier
                    .border(1.dp, colours.border, MaterialTheme.shapes.medium)
                    .padding(horizontal = VentiSpace.s3, vertical = VentiSpace.s2)
                    .testTag("chat-removed"),
            )
        } else {
            if (message.text.isNotEmpty()) {
                Text(
                    message.text,
                    style = MaterialTheme.typography.bodyLarge,
                    color = if (mine) colours.onPrimary else colours.text,
                    modifier = Modifier
                        .background(if (mine) colours.primary else colours.surface, MaterialTheme.shapes.medium)
                        .then(if (mine) Modifier else Modifier.border(1.dp, colours.border, MaterialTheme.shapes.medium))
                        .padding(horizontal = VentiSpace.s3, vertical = VentiSpace.s2)
                        .testTag("chat-text-shown"),
                )
            }
            if (message.pictures.isNotEmpty()) ChatPictures(message.pictures)
            for (card in message.cards) ChatCard(card, groups, openPlans, onOpen = { onOpenCard(card) })
            message.plan?.let { posted ->
                PlanCard(
                    posted,
                    day = PlanWords.title(posted.day),
                    myName = account.account?.displayName,
                    myPicture = null,
                    groups = groups,
                    onOpen = { onOpenPlan(posted.id) },
                )
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            if (canRemove) {
                DropdownMenuItem(
                    text = { Text(GroupWords.REMOVE, color = colours.danger) },
                    onClick = {
                        menu = false
                        isConfirmingRemoval = true
                    },
                    modifier = Modifier.testTag("message-remove"),
                )
            }
            if (canReport) {
                DropdownMenuItem(
                    text = { Text(GroupWords.REPORT_MESSAGE) },
                    onClick = {
                        menu = false
                        onReport(Reported.message(message.id, groupId))
                    },
                    modifier = Modifier.testTag("message-report"),
                )
                message.pictureIds.forEachIndexed { index, picture ->
                    DropdownMenuItem(
                        text = { Text(GroupWords.reportPicture(index, message.pictureIds.size)) },
                        onClick = {
                            menu = false
                            onReport(Reported.messagePicture(picture, groupId))
                        },
                        modifier = Modifier.testTag("message-report-picture"),
                    )
                }
            }
        }
    }
    if (isConfirmingRemoval) {
        AlertDialog(
            onDismissRequest = { isConfirmingRemoval = false },
            title = { Text(GroupWords.REMOVE_QUESTION) },
            text = { Text(GroupWords.REMOVE_MEANS) },
            confirmButton = {
                TextButton(
                    onClick = {
                        isConfirmingRemoval = false
                        onRemove()
                    },
                    modifier = Modifier.testTag("message-remove-confirm"),
                ) { Text(GroupWords.REMOVE, color = colours.danger) }
            },
            dismissButton = { TextButton(onClick = { isConfirmingRemoval = false }) { Text(Common.words.cancel) } },
        )
    }
}

/** What a chat says of a message's time: the day and the time, by the phone's zone. */
object ChatWords {
    fun posted(rfc3339: String): String =
        runCatching { Format.dayAndTime(OffsetDateTime.parse(rfc3339).toEpochSecond()) }.getOrDefault("")
}

/** A message's pictures, two side by side; a tap shows one alone. */
@Composable
private fun ChatPictures(paths: List<String>) {
    var shown by remember { mutableStateOf<String?>(null) }
    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s2), modifier = Modifier.fillMaxWidth()) {
        for (pair in paths.chunked(if (paths.size == 1) 1 else 2)) {
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2), modifier = Modifier.fillMaxWidth()) {
                for (path in pair) {
                    ChatPicture(
                        path,
                        Modifier
                            .weight(1f)
                            .height(VentiSpace.s8 * 2)
                            .clickable(role = Role.Image) { shown = path }
                            .semantics { contentDescription = GroupWords.PICTURE }
                            .testTag("chat-picture"),
                    )
                }
                if (pair.size == 1 && paths.size > 1) Spacer(Modifier.weight(1f))
            }
        }
    }
    shown?.let { path -> ChatPictureSheet(path, onDismiss = { shown = null }) }
}

/** One picture of a chat, fetched from Venti by its path; the raised surface until it has come. */
@Composable
private fun ChatPicture(path: String, modifier: Modifier, contentScale: ContentScale = ContentScale.Crop) {
    var picture by remember(path) { mutableStateOf(ChatPictureStore.kept(path)) }
    LaunchedEffect(path) { if (picture == null) picture = ChatPictureStore.fetch(path) }
    Box(modifier.clip(MaterialTheme.shapes.medium).background(Venti.colours.surfaceRaised)) {
        picture?.let { Image(it, contentDescription = null, contentScale = contentScale, modifier = Modifier.fillMaxSize()) }
    }
}

/**
 * The chats' pictures, kept while the app runs: the server's copies are up
 * to 1600 pixels, too large for the faces' store, which keeps 256-pixel
 * squares. A path names one picture for good.
 */
private object ChatPictureStore {
    private val fetched = android.util.LruCache<String, ImageBitmap>(KEPT)

    fun kept(path: String): ImageBitmap? = fetched.get(path)

    suspend fun fetch(path: String): ImageBitmap? {
        fetched.get(path)?.let { return it }
        val server = PilotPictures.server() ?: return null
        return try {
            val answer = fly.venti.app.Http.get(server + path, accept = "image/webp,image/*")
            if (answer.status != 200) return null
            val picture = withContext(Dispatchers.Default) {
                BitmapFactory.decodeByteArray(answer.body, 0, answer.body.size)?.asImageBitmap()
            } ?: return null
            fetched.put(path, picture)
            picture
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            null
        }
    }

    private const val KEPT = 24
}

/** A picture of a chat on its own, as large as the screen allows. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChatPictureSheet(path: String, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(Modifier.fillMaxWidth().testTag("chat-picture-shown")) {
            Row(Modifier.fillMaxWidth().padding(horizontal = VentiSpace.s2), horizontalArrangement = Arrangement.End) {
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("chat-picture-done")) { Text(Common.words.done) }
            }
            ChatPicture(path, Modifier.fillMaxWidth().aspectRatio(1f).padding(VentiSpace.s4), contentScale = ContentScale.Fit)
        }
    }
}

// Cards

/**
 * A link of a message as a card that opens what it names: a flight with
 * its pilot, take-off, distance, day and track drawn small; a site; an open
 * plan with "I'm in"; a pilot. What is gone says so.
 */
@Composable
private fun ChatCard(card: CardView, groups: GroupsModel, openPlans: OpenPlansModel, onOpen: () -> Unit) {
    val colours = Venti.colours
    when (card.kind) {
        "flight" -> CardLines(card, "chat-card-flight", onOpen) { TrackThumbnail(card.track) }
        "site" -> CardLines(card, "chat-card-site", onOpen) { Sign(R.drawable.venti_site) }
        "open_plan" -> Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
            CardLines(card, "chat-card-open-plan", onOpen) { Sign(R.drawable.venti_takeoff) }
            val scope = rememberCoroutineScope()
            // "I'm in" as the pilot last said it here, until the chat reads the card anew.
            var amIn by remember(card) { mutableStateOf(card.amIn) }
            var isSending by remember { mutableStateOf(false) }
            fly.venti.app.design.OnChip(
                selected = amIn,
                onClick = {
                    if (isSending) return@OnChip
                    isSending = true
                    scope.launch {
                        try {
                            amIn = openPlans.setIn(card.id, !amIn).amIn
                        } catch (error: CancellationException) {
                            throw error
                        } catch (_: Exception) {
                            // The card says what Venti said last.
                        } finally {
                            isSending = false
                        }
                    }
                },
                enabled = !isSending,
                label = groups.words.imIn,
                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("chat-card-in"),
            )
        }
        "pilot" -> CardLines(card, "chat-card-pilot", onOpen) { PilotFace(card.title, card.picturePath, size = FaceSize.Medium) }
        else -> Text(
            card.title,
            style = MaterialTheme.typography.bodyMedium,
            color = colours.textSecondary,
            modifier = Modifier
                .border(1.dp, colours.border, MaterialTheme.shapes.medium)
                .padding(VentiSpace.s3)
                .testTag("chat-card-gone"),
        )
    }
}

/** A pack sign on the raised surface, the size of a face. */
@Composable
private fun Sign(icon: Int) {
    val colours = Venti.colours
    Box(
        Modifier.size(VentiSpace.s7).background(colours.surfaceRaised, MaterialTheme.shapes.small),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = colours.textMuted, modifier = Modifier.size(VentiSize.icon))
    }
}

/** A card's two lines beside what leads it, the whole a link. One element for TalkBack. */
@Composable
private fun CardLines(card: CardView, tag: String, onOpen: () -> Unit, leading: @Composable () -> Unit) {
    val colours = Venti.colours
    Surface(
        shape = MaterialTheme.shapes.medium,
        color = colours.surface,
        border = BorderStroke(1.dp, colours.border),
        modifier = Modifier.fillMaxWidth().testTag(tag),
    ) {
        Row(
            Modifier
                .clickable(role = Role.Button, onClick = onOpen)
                .heightIn(min = VentiSize.touch)
                .padding(VentiSpace.s3)
                .clearAndSetSemantics {
                    contentDescription = card.title
                    stateDescription = card.subtitle
                },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            leading()
            Spacer(Modifier.width(VentiSpace.s3))
            Column(Modifier.weight(1f)) {
                Text(card.title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold, color = colours.text)
                Text(card.subtitle, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
            }
            Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, tint = colours.textMuted)
        }
    }
}

// Writing

/**
 * The field a message is written in, the photos chosen for it (four at
 * most, sent one by one before the message) and "Send".
 */
@Composable
private fun ChatComposer(chat: ChatModel, groups: GroupsModel, account: AccountModel) {
    val colours = Venti.colours
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var text by remember { mutableStateOf("") }
    // The chosen photos, each made a JPEG of at most 1,024 pixels.
    var photos by remember { mutableStateOf<List<Pair<ByteArray, String>>>(emptyList()) }
    var isSending by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    val canSend = !isSending && (text.isNotBlank() || photos.isNotEmpty())
    val linksLine = GroupWords.links(remember(text) { messageLinks(text).size })
    val room = GroupWords.MOST_PICTURES - photos.size
    val choose = rememberLauncherForActivityResult(
        ActivityResultContracts.PickMultipleVisualMedia(GroupWords.MOST_PICTURES),
    ) { chosen ->
        if (chosen.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            for (photo in chosen.take(room)) {
                val prepared = withContext(Dispatchers.IO) { runCatching { PictureFile.prepare(context, photo) }.getOrNull() }
                if (prepared == null) problem = PictureFile.UNREADABLE else photos = photos + prepared
            }
        }
    }

    fun send() {
        if (!canSend) return
        isSending = true
        problem = null
        val said = text
        val chosen = photos
        scope.launch {
            problem = groups.attempt {
                val pictures = chosen.map { chat.sendPicture(it).id }
                chat.post(said, pictures)
                text = ""
                photos = emptyList()
            }
            isSending = false
        }
    }

    Surface(color = colours.surface) {
        Column {
            HorizontalDivider(color = colours.border)
            Column(
                Modifier.padding(horizontal = VentiSpace.s3, vertical = VentiSpace.s2),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                problem?.let { GroupsLine(it, Modifier.testTag("chat-send-problem"), isProblem = true) }
                if (photos.isNotEmpty()) {
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                        photos.forEachIndexed { index, photo ->
                            val bitmap = remember(photo) {
                                BitmapFactory.decodeByteArray(photo.first, 0, photo.first.size)?.asImageBitmap()
                            }
                            Box(Modifier.size(VentiSpace.s8).testTag("chat-photo")) {
                                bitmap?.let {
                                    Image(
                                        it,
                                        contentDescription = null,
                                        contentScale = ContentScale.Crop,
                                        modifier = Modifier.fillMaxSize().clip(MaterialTheme.shapes.small),
                                    )
                                }
                                IconButton(
                                    onClick = { photos = photos.filterIndexed { at, _ -> at != index } },
                                    modifier = Modifier.align(Alignment.TopEnd).size(VentiSize.touch),
                                ) {
                                    Box(
                                        Modifier.size(VentiSize.icon).background(colours.primary, CircleShape),
                                        contentAlignment = Alignment.Center,
                                    ) {
                                        Icon(
                                            painterResource(R.drawable.ic_close),
                                            contentDescription = GroupWords.TAKE_PHOTO_OUT,
                                            tint = colours.onPrimary,
                                            modifier = Modifier.size(VentiSpace.s4),
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
                linksLine?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = colours.textSecondary) }
                Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                    IconButton(
                        onClick = { choose.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
                        enabled = room > 0 && !isSending,
                        modifier = Modifier.size(VentiSize.touch).testTag("chat-photos"),
                    ) {
                        Icon(painterResource(R.drawable.ic_image), contentDescription = GroupWords.ADD_PHOTOS, tint = colours.link)
                    }
                    OutlinedTextField(
                        value = text,
                        onValueChange = { text = it },
                        placeholder = { Text(groups.words.messagePlaceholder) },
                        maxLines = 5,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        modifier = Modifier.weight(1f).heightIn(min = VentiSize.touch).testTag("chat-text"),
                    )
                    FilledIconButton(
                        onClick = ::send,
                        enabled = canSend,
                        modifier = Modifier.size(VentiSize.touch).testTag("chat-send"),
                    ) {
                        Icon(painterResource(R.drawable.ic_arrow_upward), contentDescription = groups.words.send)
                    }
                }
            }
        }
    }
}
