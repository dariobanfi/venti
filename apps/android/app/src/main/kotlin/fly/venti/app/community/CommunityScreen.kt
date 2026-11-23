package fly.venti.app.community

import androidx.compose.foundation.BorderStroke
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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.design.Common
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.Venti
import fly.venti.app.design.text
import fly.venti.app.groups.GroupsModel
import fly.venti.app.groups.PlanWords
import fly.venti.app.logbook.TrackThumbnail
import fly.venti.app.map.OverviewMap
import fly.venti.app.today.PilotPosition
import fly.venti.core.FeedRow
import fly.venti.core.Notifications
import fly.venti.core.PlanView
import fly.venti.core.PublicFlight
import fly.venti.core.pilotsWords
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** What the Community page says that is not the core's to say. */
object CommunityPageWords {
    const val PLANS = "Plans"
    const val NEW_PLAN = "New plan"
    const val GROUPS = "Groups"
    const val MAKE_OR_JOIN = "Make or join a group"
    const val ALL = "All"
    const val FIND = "Find a pilot"
    const val I_AM_IN = "I am in"
    const val SIGNED_OUT =
        "With an account you plan flying days with your groups, follow pilots and see what they fly, " +
            "and hear who cheered your flights."
    const val SIGN_IN = "Sign in"

    /** Where the pilots near were looked for. */
    fun around(fromPhone: Boolean): String = if (fromPhone) "Around your position" else "Around the middle of the map"

    /** How many more are in than the faces show: "+3". */
    fun more(count: Int): String = "+${Format.count(count.toLong())}"

    /** How many lines of the feed and of the notifications the page shows; the rest are a page of their own. */
    const val FEED_CARDS = 5
    const val TOLD_LINES = 3

    /** How many faces a plan's card shows before "+3". */
    const val FACES = 5

    /** How long the phone's position is waited for before the middle of the map is taken, milliseconds. */
    const val POSITION_WAIT_MILLISECONDS = 3_000L
}

/**
 * Community, the first list of the League tab: pilots among each other in
 * one place, as the web has it (docs/plan.md, item 7). In this order: the
 * plans of the pilot's groups for the coming days, each a card with its
 * day and time large, its place, its group, the faces of who is in and
 * "I am in"; the flights of the pilots they follow, each a card with the
 * track's shape; the pilots who fly near the phone's position, or near the
 * middle of the map where the position is not allowed; finding a pilot by
 * name; and what the pilot was told.
 *
 * Signed out, the pilots near and the search are there, with a sign-in for
 * the rest. Every part asks for itself when the page shows: no request is
 * the page's own, and nothing is kept on the phone.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CommunityPage(
    community: CommunityModel,
    social: SocialModel,
    groups: GroupsModel,
    account: AccountModel,
    onOpenFlight: (PublicFlight) -> Unit,
    onOpenPilot: (String, String) -> Unit,
    /** Opens the plans' sheet: at a plan, at the form for a new one, or at the groups. */
    onOpenPlans: (PlansOpening) -> Unit,
    /** The whole feed, a page of its own. */
    onOpenFeed: () -> Unit,
    /** Everything the pilot was told, a page of its own. */
    onOpenNotifications: () -> Unit,
    /** Opens a public flight a notification is about, by its address on the server. */
    onOpenFlightById: (String) -> Unit,
    onSignIn: () -> Unit,
) {
    val signedIn = social.isSignedIn
    val own = account.account?.takeIf { signedIn }
    // The pilot's own picture, for their face among who is in.
    LaunchedEffect(own?.handle, own?.picturePath) { own?.let { PilotPictures.know(it.handle, it.picturePath) } }

    // The plans: asked when the page shows and after every change.
    LaunchedEffect(signedIn) { if (signedIn) groups.load() }

    // The feed's first page, asked when the page shows.
    var feed by remember { mutableStateOf<SocialModel.Told<fly.venti.core.Feed>?>(null) }
    var feedAsked by remember { mutableIntStateOf(0) }
    LaunchedEffect(signedIn, feedAsked) {
        if (!signedIn) return@LaunchedEffect
        feed = null
        val told = social.feed(null)
        if (told is SocialModel.Told.Read) community.know(told.value.rows.map { it.flight })
        feed = told
    }

    // What the pilot was told, the newest first.
    var told by remember { mutableStateOf<SocialModel.Told<Notifications>?>(null) }
    var toldAsked by remember { mutableIntStateOf(0) }
    LaunchedEffect(signedIn, toldAsked) {
        if (!signedIn) return@LaunchedEffect
        told = null
        told = social.notifications()
    }

    // The pilots near: the phone's position where it is allowed, else the map's middle.
    val context = LocalContext.current
    val position = remember { PilotPosition(context.applicationContext) }
    var nearAsked by remember { mutableIntStateOf(0) }
    LaunchedEffect(nearAsked) {
        position.readIfAllowed()
        val phone = if (position.isAllowed) {
            withTimeoutOrNull(CommunityPageWords.POSITION_WAIT_MILLISECONDS) { snapshotFlow { position.place }.filterNotNull().first() }
        } else {
            null
        }
        community.showNear(phone ?: OverviewMap.lastLookedAt, fromPhone = phone != null, again = nearAsked > 0)
    }

    // Finding a pilot, as everyone's flights have it.
    var wanted by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(community, wanted) { community.findPilots(wanted) }

    LazyColumn(
        modifier = Modifier.fillMaxSize().testTag("community"),
        contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s6),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
    ) {
        if (signedIn) {
            // The plans of the pilot's groups.
            item(key = "plans-head") {
                PartHead(CommunityPageWords.PLANS, Modifier.testTag("community-plans")) {
                    val board = groups.board
                    if (board != null && board.groups.isNotEmpty()) {
                        TextButton(onClick = { onOpenPlans(PlansOpening.Groups) }, modifier = Modifier.testTag("community-groups")) {
                            Text(CommunityPageWords.GROUPS)
                        }
                        TextButton(onClick = { onOpenPlans(PlansOpening.NewPlan) }, modifier = Modifier.testTag("community-new-plan")) {
                            Icon(painterResource(R.drawable.ic_add), contentDescription = null, modifier = Modifier.size(VentiSpace.s5))
                            Spacer(Modifier.width(VentiSpace.s1))
                            Text(CommunityPageWords.NEW_PLAN)
                        }
                    }
                }
            }
            val board = groups.board
            when {
                board == null && groups.problem != null -> item(key = "plans-problem") {
                    Quietly(groups.problem.orEmpty(), Modifier.testTag("community-plans-problem"))
                }
                board == null -> item(key = "plans-loading") { Waiting() }
                board.groups.isEmpty() -> item(key = "plans-no-group") {
                    Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainerLow, border = BorderStroke(1.dp, Venti.colours.border)) {
                        Column(Modifier.fillMaxWidth().padding(VentiSpace.s4), verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                            Text(PlanWords.NO_GROUP, style = MaterialTheme.typography.bodyLarge)
                            OutlinedButton(
                                onClick = { onOpenPlans(PlansOpening.Groups) },
                                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("community-make-group"),
                            ) { Text(CommunityPageWords.MAKE_OR_JOIN) }
                        }
                    }
                }
                else -> {
                    val plans = board.days.flatMap { day -> day.plans.map { day.day to it } }
                    if (plans.isEmpty()) {
                        item(key = "plans-none") { Quietly(PlanWords.NO_PLANS, Modifier.testTag("community-plans-none")) }
                    }
                    items(plans, key = { (_, plan) -> "plan-${plan.id}" }) { (day, plan) ->
                        PlanCard(
                            plan,
                            day = PlanWords.title(day),
                            myName = own?.displayName,
                            myPicture = own?.picturePath,
                            groups = groups,
                            onOpen = { onOpenPlans(PlansOpening.Plan(plan.id)) },
                        )
                    }
                    if (board.isCutShort) {
                        item(key = "plans-cut") { SectionFooter(PlanWords.CUT_SHORT) }
                    }
                    item(key = "plans-who") { SectionFooter(PlanWords.WHO_SEES) }
                }
            }

            // The flights of the pilots followed: the newest of them.
            item(key = "feed-head") {
                PartHead(social.words.feedFollowing, Modifier.testTag("community-feed")) {
                    val rows = (feed as? SocialModel.Told.Read)?.value?.rows.orEmpty()
                    if (rows.isNotEmpty()) {
                        TextButton(onClick = onOpenFeed, modifier = Modifier.testTag("community-feed-all")) { Text(CommunityPageWords.ALL) }
                    }
                }
            }
            when (val shown = feed) {
                null -> item(key = "feed-loading") { Waiting() }
                is SocialModel.Told.Refused -> item(key = "feed-problem") {
                    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                        Quietly(listOf(social.words.feedFailed, shown.words).filter { it.isNotEmpty() }.joinToString("\n"))
                        OutlinedButton(onClick = { feedAsked += 1 }, modifier = Modifier.heightIn(min = VentiSize.touch)) { Text(Common.words.tryAgain) }
                    }
                }
                is SocialModel.Told.Read -> {
                    val rows = shown.value.rows.take(CommunityPageWords.FEED_CARDS)
                    if (rows.isEmpty()) {
                        item(key = "feed-empty") { Quietly(shown.value.emptyLine, Modifier.testTag("community-feed-empty")) }
                    }
                    items(rows, key = { "feed-${it.flight.id}" }) { row -> FeedCard(row, onOpen = { onOpenFlight(row.flight) }) }
                }
            }
        } else {
            // Signed out: what needs an account, and the way to one.
            item(key = "signed-out") {
                Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainerLow, border = BorderStroke(1.dp, Venti.colours.border)) {
                    Column(Modifier.fillMaxWidth().padding(VentiSpace.s4), verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                        Text(CommunityPageWords.SIGNED_OUT, style = MaterialTheme.typography.bodyLarge)
                        Button(onClick = onSignIn, modifier = Modifier.heightIn(min = VentiSize.touch).testTag("community-sign-in")) {
                            Text(CommunityPageWords.SIGN_IN)
                        }
                    }
                }
            }
        }

        // The pilots who fly near the phone, or near the map's middle.
        item(key = "near-head") {
            PartHead(PilotPictures.words.nearTitle, Modifier.testTag("community-near")) {
                IconButton(onClick = { nearAsked += 1 }, enabled = !community.near.isLoading) {
                    Icon(painterResource(R.drawable.ic_refresh), contentDescription = "Ask again")
                }
            }
        }
        val near = community.near
        val nearPilots = near.pilots
        item(key = "near-where") {
            val said = listOfNotNull(
                near.place?.let { CommunityPageWords.around(near.fromPhone) },
                nearPilots?.words?.takeIf { it.isNotEmpty() },
            ).joinToString(" · ")
            if (said.isNotEmpty()) {
                Text(said, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary, modifier = Modifier.testTag("pilots-near-words"))
            }
        }
        when {
            nearPilots != null && nearPilots.pilots.isNotEmpty() -> item(key = "near") {
                Group(Modifier.testTag("pilots-near")) {
                    for (pilot in nearPilots.pilots) {
                        row {
                            PilotLine(
                                pilot.name,
                                pilot.picturePath,
                                title = pilot.name,
                                line = pilot.words,
                                initials = pilot.initials,
                                modifier = Modifier.testTag("near-pilot"),
                                onClick = { onOpenPilot(pilot.handle, pilot.name) },
                            )
                        }
                    }
                }
            }
            nearPilots != null -> item(key = "near-empty") { Quietly(nearPilots.empty, Modifier.testTag("pilots-near-empty")) }
            near.problem != null -> item(key = "near-problem") { Quietly(near.problem, Modifier.testTag("pilots-near-problem")) }
            else -> item(key = "near-loading") { Waiting() }
        }

        // Finding a pilot by name.
        item(key = "find-head") { PartHead(CommunityPageWords.FIND) {} }
        item(key = "find") {
            val words = remember { pilotsWords() }
            OutlinedTextField(
                value = wanted,
                onValueChange = { wanted = it },
                placeholder = { Text(words.searchPlaceholder) },
                leadingIcon = { Icon(painterResource(R.drawable.ic_search), contentDescription = null) },
                trailingIcon = {
                    if (wanted.isNotEmpty()) {
                        IconButton(onClick = { wanted = "" }) {
                            Icon(painterResource(R.drawable.ic_close), contentDescription = words.searchClear)
                        }
                    }
                },
                singleLine = true,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words, autoCorrectEnabled = false),
                shape = MaterialTheme.shapes.extraLarge,
                modifier = Modifier.fillMaxWidth().testTag("community-find"),
            )
        }
        val search = community.search.takeIf { it.text == wanted && wanted.isNotBlank() }
        if (search != null) {
            if (search.pilots.isNotEmpty()) {
                item(key = "found") {
                    Group {
                        for (line in search.pilots) {
                            row {
                                PilotLine(
                                    line.name,
                                    line.picturePath,
                                    title = line.words,
                                    modifier = Modifier.testTag("pilot-found"),
                                    onClick = { onOpenPilot(line.handle, line.name) },
                                )
                            }
                        }
                    }
                }
            }
            if (search.words.isNotEmpty()) {
                item(key = "found-words") { SectionFooter(search.words, Modifier.testTag("pilots-found-words")) }
            } else if (search.isLoading) {
                item(key = "found-loading") { Waiting() }
            }
        }

        // What the pilot was told: the newest lines, the rest on their own page.
        if (signedIn) {
            item(key = "told-head") {
                PartHead(social.words.notificationsTitle, Modifier.testTag("community-notifications")) {
                    val lines = (told as? SocialModel.Told.Read)?.value?.lines.orEmpty()
                    if (lines.size > CommunityPageWords.TOLD_LINES) {
                        TextButton(onClick = onOpenNotifications, modifier = Modifier.testTag("community-notifications-all")) {
                            Text(CommunityPageWords.ALL)
                        }
                    }
                }
            }
            when (val shown = told) {
                null -> item(key = "told-loading") { Waiting() }
                is SocialModel.Told.Refused -> item(key = "told-problem") {
                    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                        Quietly(listOf(social.words.notificationsFailed, shown.words).filter { it.isNotEmpty() }.joinToString("\n"))
                        OutlinedButton(onClick = { toldAsked += 1 }, modifier = Modifier.heightIn(min = VentiSize.touch)) { Text(Common.words.tryAgain) }
                    }
                }
                is SocialModel.Told.Read -> {
                    val lines = shown.value.lines.asReversed().take(CommunityPageWords.TOLD_LINES)
                    if (lines.isEmpty()) {
                        item(key = "told-empty") { Quietly(shown.value.emptyLine) }
                    } else {
                        item(key = "told") {
                            Group {
                                for (line in lines) {
                                    row {
                                        Notification(line) {
                                            line.flightId?.let(onOpenFlightById) ?: onOpenPilot(line.handle, "@${line.handle}")
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** How the plans' sheet opens from Community. */
sealed interface PlansOpening {
    data class Plan(val id: String) : PlansOpening

    data object NewPlan : PlansOpening

    data object Groups : PlansOpening
}

/** The heading of a part of the page, with what it offers at its end. */
@Composable
private fun PartHead(title: String, modifier: Modifier = Modifier, actions: @Composable () -> Unit) {
    Row(
        modifier.fillMaxWidth().padding(top = VentiSpace.s4).heightIn(min = VentiSize.touch),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            title,
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.weight(1f).semantics { heading() },
        )
        actions()
    }
}

/** A sentence in place of what is not there. */
@Composable
private fun Quietly(text: String, modifier: Modifier = Modifier) {
    Text(text, style = MaterialTheme.typography.bodyLarge, color = Venti.colours.textSecondary, modifier = modifier.fillMaxWidth())
}

@Composable
private fun Waiting() {
    Box(Modifier.fillMaxWidth().padding(vertical = VentiSpace.s4), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(Modifier.size(VentiSpace.s6))
    }
}

/**
 * One plan as a card: its day and its time large at the top, the place
 * with the pack's take-off sign, its group, the faces of who is in with
 * the core's sentence beside them, and the pilot's own word on it, "I am
 * in", a switch that stays on while the pilot is in. The card opens the
 * plan, where everything else is.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PlanCard(
    plan: PlanView,
    day: String,
    myName: String?,
    myPicture: String?,
    groups: GroupsModel,
    onOpen: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var isWorking by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    val colours = Venti.colours
    Surface(
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        border = BorderStroke(1.dp, colours.border),
        modifier = Modifier.fillMaxWidth().testTag("plan-card"),
    ) {
        Column(Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s3, bottom = VentiSpace.s4)) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .clickable(role = Role.Button, onClick = onOpen)
                    .semantics(mergeDescendants = true) {
                        contentDescription = plan.place
                        stateDescription = listOf(day, PlanWords.spoken(plan)).filter { it.isNotEmpty() }.joinToString(". ")
                    },
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                // The day leads, as a calendar's page does; the time at its right.
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        day,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = colours.link,
                        modifier = Modifier.weight(1f),
                    )
                    plan.meetAt?.let { time -> Text(time, style = VentiType.instrumentM.text, color = colours.text, maxLines = 1) }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(painterResource(R.drawable.venti_takeoff), contentDescription = null, tint = colours.textSecondary)
                    Spacer(Modifier.width(VentiSpace.s2))
                    Text(plan.place, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold, color = colours.text)
                }
                if (plan.groupName.isNotEmpty()) {
                    Text(plan.groupName, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                }
            }
            Spacer(Modifier.size(VentiSpace.s3))
            FlowRow(
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                itemVerticalAlignment = Alignment.CenterVertically,
            ) {
                Faces(plan, myName, myPicture)
                if (plan.inWords.isNotEmpty()) {
                    Text(plan.inWords, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary, modifier = Modifier.weight(1f, fill = false))
                }
            }
            Spacer(Modifier.size(VentiSpace.s3))
            FilterChip(
                selected = plan.amIn,
                onClick = {
                    if (isWorking) return@FilterChip
                    isWorking = true
                    problem = null
                    scope.launch {
                        try {
                            problem = groups.attempt { groups.setIn(plan, !plan.amIn) }
                        } finally {
                            isWorking = false
                        }
                    }
                },
                enabled = !isWorking,
                label = { Text(CommunityPageWords.I_AM_IN) },
                leadingIcon = if (plan.amIn) {
                    { Icon(painterResource(R.drawable.ic_check), contentDescription = null, modifier = Modifier.size(VentiSpace.s4)) }
                } else {
                    null
                },
                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("plan-card-in"),
            )
            problem?.let { said ->
                Text(said, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("plan-card-problem"))
            }
        }
    }
}

/** Who is in, as a row of faces laid a little over each other: the pilot of the phone first, as the core names them. */
@Composable
private fun Faces(plan: PlanView, myName: String?, myPicture: String?) {
    val names = plan.pilotsIn
    if (names.isEmpty()) return
    val room = CommunityPageWords.FACES
    val shown = if (names.size > room) names.take(room - 1) else names
    val more = names.size - shown.size
    // Laid a little over each other, as a row of faces is.
    Row(
        Modifier.clearAndSetSemantics {},
        horizontalArrangement = Arrangement.spacedBy(-VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        shown.forEachIndexed { index, name ->
            val isMe = plan.amIn && index == 0
            PilotFace(if (isMe && myName != null) myName else name, if (isMe) myPicture else null, size = FaceSize.Small)
        }
        if (more > 0) {
            Surface(
                shape = CircleShape,
                color = Venti.colours.surfaceRaised,
                border = BorderStroke(1.dp, Venti.colours.border),
                modifier = Modifier.size(FaceSize.Small.side),
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Text(CommunityPageWords.more(more), style = MaterialTheme.typography.labelMedium, color = Venti.colours.textSecondary)
                }
            }
        }
    }
}

/**
 * One flight of the feed as a card, to be told apart at a glance as a row
 * of the logbook is: the track's shape on a little map of its own, the
 * pilot's face and name, where they took off and on which day, the airtime
 * and the distance large, and the core's lines of cheers and comments. The
 * card opens the flight.
 */
@Composable
private fun FeedCard(row: FeedRow, onOpen: () -> Unit) {
    val flight = row.flight
    val colours = Venti.colours
    val triangle = CommunityWords.triangle(flight)
    val said = listOf(row.cheersLine, row.commentsLine).filter { it.isNotEmpty() }.joinToString(" · ")
    Surface(
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        border = BorderStroke(1.dp, colours.border),
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .testTag("feed-card")
                .clickable(role = Role.Button, onClick = onOpen)
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s3, bottom = VentiSpace.s4)
                .clearAndSetSemantics {
                    contentDescription = flight.pilot
                    stateDescription = spokenWith(said, CommunityWords.spoken(flight))
                },
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TrackThumbnail(flight.outline, large = true)
                Spacer(Modifier.width(VentiSpace.s3))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        PilotFace(flight.pilot, flight.pilotPicture, size = FaceSize.Small)
                        Spacer(Modifier.width(VentiSpace.s2))
                        Text(
                            flight.pilot,
                            style = MaterialTheme.typography.titleMedium,
                            color = colours.link,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        if (flight.pilotCountry != null) {
                            Spacer(Modifier.width(VentiSpace.s1))
                            Flag(flight.pilotCountry, style = MaterialTheme.typography.titleMedium)
                        }
                    }
                    // Where it took off: the site's name, else where it is; nothing where the track shows no take-off.
                    val takeoff = flight.takeoff
                    val where = CommunityWords.place(flight) ?: takeoff?.let { Format.place(it.lat, it.lon) }
                    if (where != null) Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(painterResource(R.drawable.venti_takeoff), contentDescription = null, tint = colours.textMuted, modifier = Modifier.size(VentiSpace.s4))
                        Spacer(Modifier.width(VentiSpace.s1))
                        Text(
                            where,
                            style = MaterialTheme.typography.bodyLarge,
                            color = colours.text,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    Text(Format.day(flight.startUnixS), style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                }
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s5), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                Large(Format.hoursMinutes(flight.airtimeS), "h")
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (triangle != null) {
                        Icon(painterResource(R.drawable.venti_triangle), contentDescription = null, tint = colours.textSecondary, modifier = Modifier.size(VentiSpace.s5))
                        Spacer(Modifier.width(VentiSpace.s1))
                    }
                    Large(Format.kilometres(flight.score?.distanceM ?: flight.trackLengthM), "km")
                }
            }
            CommunityWords.glider(flight)?.let { glider ->
                Text(glider, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
            }
            if (said.isNotEmpty()) {
                Text(said, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
            }
        }
    }
}

/** A number large with its unit smaller after it, as the logbook's rows have them. */
@Composable
private fun Large(number: String, unit: String) {
    Text(
        buildAnnotatedString {
            append(number)
            withStyle(SpanStyle(fontSize = VentiType.label.text.fontSize, fontWeight = FontWeight.Medium, color = Venti.colours.textSecondary)) {
                append(" $unit")
            }
        },
        style = VentiType.instrumentM.text,
        color = Venti.colours.text,
        maxLines = 1,
        textAlign = TextAlign.Start,
    )
}
