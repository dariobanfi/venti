package fly.venti.app.community

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.design.FloatElevation
import fly.venti.app.design.Group
import fly.venti.app.design.PillShape
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.Venti
import fly.venti.app.logbook.groupedShape
import fly.venti.core.CheerList
import fly.venti.core.cheerButton
import fly.venti.core.CommentList
import fly.venti.core.CommentRow
import fly.venti.core.FeedRow
import fly.venti.core.FollowList
import fly.venti.core.NotificationLine
import fly.venti.core.Notifications
import fly.venti.core.PublicFlight
import fly.venti.core.commentDraft
import fly.venti.core.commentReportForm
import fly.venti.core.commentsSwitch
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/**
 * On a pilot's page, under the handle: how many follow them and whom they
 * follow, and for a signed-in pilot the button that follows them. The
 * counts, the button's word and whether there is a button are the core's:
 * it gives none on the pilot's own page and none signed out.
 */
@Composable
fun FollowLine(handle: String, social: SocialModel, requests: CoroutineScope, modifier: Modifier = Modifier) {
    var list by remember(handle) { mutableStateOf<FollowList?>(null) }
    var problem by remember(handle) { mutableStateOf<String?>(null) }
    var isSending by remember(handle) { mutableStateOf(false) }
    LaunchedEffect(handle, social.isSignedIn) {
        when (val told = social.follows(handle)) {
            is SocialModel.Told.Read -> list = told.value
            // Without the counts the page is whole all the same.
            is SocialModel.Told.Refused -> list = null
        }
    }
    val shown = list ?: return
    Column(modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
            Text(
                "${shown.followersLine} · ${shown.followingLine}",
                style = MaterialTheme.typography.bodyMedium,
                color = Venti.colours.textSecondary,
                modifier = Modifier.weight(1f).testTag("pilot-follows"),
            )
            val following = shown.youFollow
            val button = shown.button
            if (following != null && button != null && social.isSignedIn) {
                val press = {
                    isSending = true
                    problem = null
                    requests.launch {
                        try {
                            when (val told = social.follow(handle, !following)) {
                                is SocialModel.Told.Read -> list = shown.copy(
                                    youFollow = told.value.following,
                                    button = told.value.button,
                                    followers = told.value.followers,
                                    followersLine = told.value.followersLine,
                                )
                                is SocialModel.Told.Refused -> problem = told.words
                            }
                        } finally {
                            isSending = false
                        }
                    }
                    Unit
                }
                val tagged = Modifier.heightIn(min = VentiSize.touch).testTag("pilot-follow")
                if (following) {
                    OutlinedButton(onClick = press, enabled = !isSending, modifier = tagged) { Text(button) }
                } else {
                    Button(onClick = press, enabled = !isSending, modifier = tagged) { Text(button) }
                }
            }
        }
        problem?.let { words -> Refusal(words, Modifier.testTag("pilot-follow-problem")) }
    }
}

/** What the server refused, in the core's words, where the pilot asked for it. */
@Composable
private fun Refusal(words: String, modifier: Modifier = Modifier) {
    Text(
        words,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.error,
        modifier = modifier.padding(top = VentiSpace.s2),
    )
}

/**
 * The feed: the public flights of the pilots one follows, the newest
 * first, each as the list of everyone's flights draws it, with the core's
 * line of cheers and comments under it. More is fetched as the list ends.
 */
@Composable
fun FollowingFeed(social: SocialModel, community: CommunityModel, onOpen: (PublicFlight) -> Unit) {
    var rows by remember { mutableStateOf<List<FeedRow>>(emptyList()) }
    var next by remember { mutableStateOf<String?>(null) }
    var emptyLine by remember { mutableStateOf<String?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    var isLoading by remember { mutableStateOf(true) }
    // Counted up to ask for the page after the last row.
    var wanted by remember { mutableIntStateOf(0) }
    LaunchedEffect(wanted) {
        if (wanted > 0 && next == null) return@LaunchedEffect
        isLoading = true
        when (val told = social.feed(if (wanted == 0) null else next)) {
            is SocialModel.Told.Read -> {
                val page = told.value
                community.know(page.rows.map { it.flight })
                rows = if (wanted == 0) page.rows else rows + page.rows.filter { row -> rows.none { it.flight.id == row.flight.id } }
                next = page.next
                emptyLine = page.emptyLine
                problem = null
            }
            is SocialModel.Told.Refused -> problem = told.words
        }
        isLoading = false
    }
    LazyColumn(
        modifier = Modifier.fillMaxSize().testTag("feed"),
        contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s3, bottom = VentiSpace.s5),
    ) {
        itemsIndexed(rows, key = { _, row -> row.flight.id }) { index, row ->
            PublicFlightRow(
                row.flight,
                shape = groupedShape(index, rows.size),
                tag = "feed-row",
                onOpen = { onOpen(row.flight) },
                under = listOf(row.cheersLine, row.commentsLine).filter { it.isNotEmpty() }.joinToString(" · "),
            )
            if (index < rows.lastIndex) {
                Spacer(Modifier.size(VentiSpace.s1 / 2))
            } else {
                // The last row has come into view: fetch what follows it.
                LaunchedEffect(row.flight.id) { if (next != null) wanted += 1 }
            }
        }
        // The core's sentence where the feed could not be loaded, the error's own words under it.
        val said = problem?.let { words -> listOf(social.words.feedFailed, words).filter { it.isNotEmpty() }.joinToString("\n") }
            ?: emptyLine?.takeIf { rows.isEmpty() }
        if (said != null && !isLoading) {
            item(key = "said") {
                Text(
                    said,
                    style = MaterialTheme.typography.bodyLarge,
                    color = Venti.colours.textSecondary,
                    textAlign = TextAlign.Center,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s7)
                        .testTag(if (problem != null) "feed-problem" else "feed-empty"),
                )
            }
        }
        if (isLoading) {
            item(key = "loading") { Waiting() }
        }
    }
}

@Composable
private fun Waiting() {
    Box(Modifier.fillMaxWidth().padding(vertical = VentiSpace.s5), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(Modifier.size(VentiSpace.s6))
    }
}

/**
 * Under the head of a public flight's page, floating over its map: the
 * cheer button for a signed-in pilot on another pilot's flight, the core's
 * line of who cheered, and the entry that opens the comments. Nothing of
 * it shows until the server has said how the flight stands.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FlightSocialBar(
    flight: PublicFlight,
    social: SocialModel,
    account: AccountModel,
    server: String?,
    requests: CoroutineScope,
    modifier: Modifier = Modifier,
) {
    var cheers by remember(flight.id) { mutableStateOf<CheerList?>(null) }
    var comments by remember(flight.id) { mutableStateOf<CommentList?>(null) }
    var problem by remember(flight.id) { mutableStateOf<String?>(null) }
    var isSending by remember(flight.id) { mutableStateOf(false) }
    var showsComments by rememberSaveable(flight.id) { mutableStateOf(false) }
    // Counted up to read the comments again.
    var asked by remember(flight.id) { mutableIntStateOf(0) }
    LaunchedEffect(flight.id, social.isSignedIn) {
        (social.cheers(flight.id) as? SocialModel.Told.Read)?.let { cheers = it.value }
    }
    LaunchedEffect(flight.id, asked, social.isSignedIn) {
        (social.comments(flight.id, flight.handle) as? SocialModel.Told.Read)?.let { comments = it.value }
    }
    val isOwn = social.ownHandle == flight.handle
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2, Alignment.CenterHorizontally),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
        ) {
            cheers?.let { shown ->
                val cheered = shown.cheered
                val showsButton = cheered != null && social.isSignedIn && !isOwn
                if (showsButton && cheered != null) {
                    // A button in the pills' manner, filled while cheered, as on the iPhone.
                    Pill(
                        cheerButton(cheered),
                        Modifier
                            .testTag("flight-cheer")
                            .semantics { selected = cheered }
                            .clickable(enabled = !isSending, role = Role.Button) {
                                val wanted = !cheered
                                isSending = true
                                problem = null
                                requests.launch {
                                    try {
                                        when (val told = social.cheer(flight.id, wanted)) {
                                            is SocialModel.Told.Read -> cheers = shown.copy(
                                                cheered = told.value.cheered,
                                                button = told.value.button,
                                                count = told.value.cheers,
                                                line = told.value.line,
                                            )
                                            is SocialModel.Told.Refused -> problem = told.words
                                        }
                                    } finally {
                                        isSending = false
                                    }
                                }
                            },
                        isOn = cheered,
                    )
                }
                // Beside the button, a line of the pilot's own cheer alone
                // ("You cheered") would only say again what "Cheered" says.
                val repeatsButton = showsButton && shown.cheered == true && shown.count == 1u
                if (shown.line.isNotEmpty() && !repeatsButton) {
                    Pill(shown.line, Modifier.testTag("flight-cheers-line"), quiet = true)
                }
            }
            comments?.let { shown ->
                Pill(
                    shown.countLine.ifEmpty { social.words.commentsTitle },
                    Modifier.testTag("flight-comments").clickable(role = Role.Button) { showsComments = true },
                )
            }
        }
        problem?.let { words ->
            Pill(words, Modifier.padding(top = VentiSpace.s1).testTag("flight-cheer-problem"), quiet = true)
        }
    }
    val shown = comments
    if (showsComments && shown != null) {
        CommentsSheet(
            flight,
            shown,
            social,
            account,
            server,
            requests,
            onChanged = { comments = it },
            onAgain = { asked += 1 },
            onDismiss = { showsComments = false },
        )
    }
}

/**
 * A pill floating over a map: the chip's look, on the primary ground while
 * `isOn`. A `quiet` one only says something. 36 high as the pack draws a
 * chip, 48 to the finger.
 */
@Composable
private fun Pill(words: String, modifier: Modifier = Modifier, isOn: Boolean = false, quiet: Boolean = false) {
    val colours = Venti.colours
    // As high as the touch target, a quiet one too, so that the pills stand in one line.
    Box(Modifier.heightIn(min = VentiSize.touch), contentAlignment = Alignment.Center) {
        Surface(
            shape = PillShape,
            color = if (isOn) colours.primary else colours.surface,
            contentColor = when {
                isOn -> colours.onPrimary
                quiet -> colours.textSecondary
                else -> colours.text
            },
            shadowElevation = FloatElevation,
            modifier = modifier,
        ) {
            Text(
                words,
                style = MaterialTheme.typography.labelLarge,
                modifier = Modifier
                    .heightIn(min = VentiSpace.s6 + VentiSpace.s1)
                    .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
            )
        }
    }
}

/**
 * The comments under a public flight, the oldest first, and for a
 * signed-in pilot the field that writes one. A comment is shown as the
 * text it is: no link and no markup is made of it. Who may remove one, the
 * counter, what is wrong with a draft and every refusal are the core's.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CommentsSheet(
    flight: PublicFlight,
    list: CommentList,
    social: SocialModel,
    account: AccountModel,
    server: String?,
    requests: CoroutineScope,
    onChanged: (CommentList) -> Unit,
    onAgain: () -> Unit,
    onDismiss: () -> Unit,
) {
    val words = social.words
    var text by rememberSaveable(flight.id) { mutableStateOf("") }
    var isSending by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var reporting by remember { mutableStateOf<CommentRow?>(null) }
    // The comment the pilot asked to remove, until they say yes or no.
    var removing by remember { mutableStateOf<CommentRow?>(null) }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                list.countLine.ifEmpty { words.commentsTitle },
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.weight(1f).testTag("comments-count"),
            )
            TextButton(onClick = onDismiss, modifier = Modifier.testTag("comments-close")) { Text(Common.words.done) }
        }
        Column(
            Modifier
                .testTag("comments")
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            if (list.comments.isNotEmpty()) {
                Group {
                    for (comment in list.comments) {
                        row {
                            Comment(
                                comment,
                                remove = words.commentRemove.takeIf { comment.canRemove && social.isSignedIn },
                                report = words.commentReport.takeIf { social.isSignedIn && comment.handle != social.ownHandle },
                                enabled = !isSending,
                                onRemove = { removing = comment },
                                onReport = { reporting = comment },
                            )
                        }
                    }
                }
            }
            val closed = list.closedLine
            when {
                closed != null -> SectionFooter(closed, Modifier.testTag("comments-closed"))
                social.isSignedIn -> {
                    val draft = remember(text) { commentDraft(text) }
                    OutlinedTextField(
                        value = text,
                        onValueChange = { text = it },
                        placeholder = { Text(words.commentPlaceholder) },
                        enabled = !isSending,
                        isError = draft.fault != null,
                        supportingText = { Text(draft.fault ?: draft.counter) },
                        minLines = 2,
                        maxLines = 6,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        modifier = Modifier.fillMaxWidth().testTag("comment-text"),
                    )
                    problem?.let { said -> Refusal(said, Modifier.testTag("comment-problem")) }
                    Button(
                        onClick = {
                            isSending = true
                            problem = null
                            requests.launch {
                                try {
                                    when (val told = social.post(flight.id, text)) {
                                        is SocialModel.Told.Read -> {
                                            text = ""
                                            onChanged(
                                                list.copy(
                                                    count = told.value.count,
                                                    countLine = told.value.countLine,
                                                    comments = list.comments + told.value.comment,
                                                ),
                                            )
                                        }
                                        is SocialModel.Told.Refused -> problem = told.words
                                    }
                                } finally {
                                    isSending = false
                                }
                            }
                        },
                        enabled = draft.canSend && !isSending,
                        modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("comment-send"),
                    ) {
                        Text(words.commentSend)
                    }
                    SectionFooter(draft.notice, Modifier.testTag("comment-notice"))
                }
                else -> problem?.let { said -> Refusal(said) }
            }
        }
    }
    removing?.let { comment ->
        AlertDialog(
            onDismissRequest = { removing = null },
            text = { Text(words.commentRemoveQuestion) },
            confirmButton = {
                TextButton(
                    onClick = {
                        removing = null
                        isSending = true
                        problem = null
                        requests.launch {
                            try {
                                when (val told = social.remove(flight.id, comment.id)) {
                                    is SocialModel.Told.Read -> onAgain()
                                    is SocialModel.Told.Refused -> problem = told.words
                                }
                            } finally {
                                isSending = false
                            }
                        }
                    },
                    modifier = Modifier.testTag("comment-remove-confirm"),
                ) { Text(words.commentRemove) }
            },
            dismissButton = { TextButton(onClick = { removing = null }) { Text(Common.words.cancel) } },
        )
    }
    reporting?.let { comment ->
        ReportSheet(
            title = words.commentReportTitle,
            form = remember { commentReportForm() },
            account = account,
            server = server,
            requests = requests,
            send = { reason, said -> social.reportComment(flight.id, comment.id, reason, said) },
            onDismiss = { reporting = null },
        )
    }
}

/** One comment: who and when, what they wrote, and what the pilot may do about it. */
@Composable
private fun Comment(
    comment: CommentRow,
    /** The word of the action that removes it; `null` where the pilot may not. */
    remove: String?,
    /** The word of the action that reports it; `null` where there is none. */
    report: String?,
    enabled: Boolean,
    onRemove: () -> Unit,
    onReport: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().testTag("comment").padding(start = VentiSpace.s4, end = VentiSpace.s1, top = VentiSpace.s2, bottom = VentiSpace.s3)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                comment.byline,
                style = MaterialTheme.typography.bodyMedium,
                color = Venti.colours.textSecondary,
                modifier = Modifier.weight(1f).heightIn(min = VentiSpace.s5),
            )
            if (remove != null) {
                TextButton(onClick = onRemove, enabled = enabled, modifier = Modifier.testTag("comment-remove")) { Text(remove) }
            }
            if (report != null) {
                TextButton(onClick = onReport, enabled = enabled, modifier = Modifier.testTag("comment-report")) { Text(report) }
            }
        }
        // Plain text, as it was written.
        Text(comment.text, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.padding(end = VentiSpace.s3))
    }
}

/**
 * On the pilot's own public flight, in the sheet that says who may see
 * it: whether other pilots can write under it. The switch's name and the
 * line under it are the core's; how it stands is read from the flight's
 * comments when the sheet shows.
 */
@Composable
fun CommentsSwitchRow(flight: String, social: SocialModel, requests: CoroutineScope) {
    var open by remember(flight) { mutableStateOf<Boolean?>(null) }
    var problem by remember(flight) { mutableStateOf<String?>(null) }
    var isSending by remember(flight) { mutableStateOf(false) }
    LaunchedEffect(flight) {
        (social.comments(flight, social.ownHandle) as? SocialModel.Told.Read)?.let { open = it.value.open }
    }
    val stands = open ?: return
    val switch = remember(stands) { commentsSwitch(stands) }
    Group(Modifier.padding(top = VentiSpace.s4)) {
        row {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("share-comments")
                    .toggleable(value = stands, enabled = !isSending, role = Role.Switch) { wanted ->
                        isSending = true
                        problem = null
                        requests.launch {
                            try {
                                when (val told = social.commentsOpen(flight, wanted)) {
                                    is SocialModel.Told.Read -> open = told.value.open
                                    is SocialModel.Told.Refused -> problem = told.words
                                }
                            } finally {
                                isSending = false
                            }
                        }
                    }
                    .heightIn(min = VentiSize.touch)
                    .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(switch.title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                androidx.compose.material3.Switch(checked = stands, onCheckedChange = null)
            }
        }
    }
    SectionFooter(problem ?: switch.line, Modifier.testTag("share-comments-line"))
}

/**
 * "Notifications", a page of the account: what the pilot was told, the
 * newest on top. A line about a flight opens that flight, one about a
 * follower that pilot's page. It is read when the page shows, and what it
 * shows is then seen: off the Community tab's badge on every device.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NotificationsScreen(
    social: SocialModel,
    /** Opens a public flight by its address on the server. */
    onOpenFlight: (String) -> Unit,
    /** Opens a pilot's page by their handle. */
    onOpenPilot: (String) -> Unit,
    onBack: () -> Unit,
) {
    var told by remember { mutableStateOf<SocialModel.Told<Notifications>?>(null) }
    // Counted up to ask again.
    var asked by remember { mutableIntStateOf(0) }
    LaunchedEffect(asked) {
        told = null
        val read = social.notifications()
        told = read
        if (read is SocialModel.Told.Read) social.unread?.sawNotifications(upTo = read.value.cursor)
    }
    BackHandler(onBack = onBack)
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(social.words.notificationsTitle) },
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
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s5)
                .testTag("notifications"),
        ) {
            when (val shown = told) {
                null -> Waiting()
                is SocialModel.Told.Refused -> {
                    // The core's sentence, and the error's own words under it.
                    Text(
                        listOf(social.words.notificationsFailed, shown.words).filter { it.isNotEmpty() }.joinToString("\n"),
                        style = MaterialTheme.typography.bodyLarge,
                        color = Venti.colours.textSecondary,
                        modifier = Modifier.padding(vertical = VentiSpace.s4).testTag("notifications-problem"),
                    )
                    OutlinedButton(onClick = { asked += 1 }, modifier = Modifier.heightIn(min = VentiSize.touch)) {
                        Text(Common.words.tryAgain)
                    }
                }
                is SocialModel.Told.Read -> {
                    val lines = shown.value.lines.asReversed()
                    if (lines.isEmpty()) {
                        Text(
                            shown.value.emptyLine,
                            style = MaterialTheme.typography.bodyLarge,
                            color = Venti.colours.textSecondary,
                            textAlign = TextAlign.Center,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s7)
                                .testTag("notifications-empty"),
                        )
                    } else {
                        Group {
                            for (line in lines) {
                                row { Notification(line) { line.flightId?.let(onOpenFlight) ?: onOpenPilot(line.handle) } }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** One thing the pilot was told, and how long ago. */
@Composable
internal fun Notification(line: NotificationLine, onOpen: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("notification")
            .clickable(role = Role.Button, onClick = onOpen)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(line.line, style = MaterialTheme.typography.bodyLarge)
            Text(line.age, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
        }
        Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, tint = Venti.colours.textSecondary)
    }
}

/** What the feed's rows say of a flight for someone who cannot see it. Used by [PublicFlightRow]. */
internal fun spokenWith(under: String?, spoken: String): String =
    if (under.isNullOrEmpty()) spoken else "$spoken, $under"
