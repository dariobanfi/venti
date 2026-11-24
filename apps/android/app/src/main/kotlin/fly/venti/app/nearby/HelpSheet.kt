package fly.venti.app.nearby

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.community.ReportSheet
import fly.venti.app.community.Reported
import fly.venti.app.design.Common
import fly.venti.app.design.Format
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.today.PilotPosition
import fly.venti.core.Coordinate
import fly.venti.core.HelpView
import fly.venti.core.helpEmergencyWords
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** What the screens of help after landing say that is not the core's to say. */
object HelpWords {
    const val ENTRY = "Ask pilots nearby for help"
    const val TITLE = "Help after landing"
    const val NOTE = "What do you need?"
    const val NOTE_EXAMPLE = "By the river at Völs, anyone driving to Innsbruck?"
    const val ASK = "Ask pilots nearby"
    const val CALL = "Call 112"
    const val CLOSE = "I'm fine, close"
    const val CAN = "I can help"
    const val CANNOT = "I can't"
    const val CAN_HELP = "Can help"
    const val WHERE = "Where they are"
    const val DIRECTIONS = "Directions"
    const val ASKED = "Pilots nearby ask for help"
    const val ENDED = "This request has ended."
    const val NO_POSITION = "Venti needs to know where you are to ask pilots nearby."
    const val USE_POSITION = "Use my position"
    const val WAITING_FOR_POSITION = "Finding where you are"
    const val WHERE_YOU_ARE = "Where you are"
    const val SIGNED_OUT = "Sign in to ask pilots nearby for help, or to be asked."
    const val REPORT = "Report this request"
    const val NOTE_ROOM = 280

    /** The channel of the phone's notifications that pushes come on. */
    const val CHANNEL = "Pilots nearby"
    const val CHANNEL_ABOUT = "Calls for help after landing, their answers, and new open plans near you."

    /** Where a pilot is, in the numbers a phone's maps take. */
    fun place(at: Coordinate): String = Format.place(at.lat, at.lon)
}

/** Where the sheet of help after landing opens. */
sealed interface HelpOpening {
    /**
     * The pilot's own request, or the form to ask, with the requests the
     * pilot was asked under it. `landedAt` is where the cockpit saw the
     * glider land; without it the phone's position is asked.
     */
    data class Home(val landedAt: Coordinate? = null) : HelpOpening

    /** One request: from a notification, or from Community's list. */
    data class One(val id: String) : HelpOpening
}

/**
 * Help after landing (docs/design.md, "Help after landing"): first, and
 * in the warning's colours, the core's words that this is no emergency
 * service, with a button that dials 112; then the pilot's own call, its
 * line and who can help, with "I'm fine, close", or the form to ask; and
 * a request the pilot was asked, with "I can help" and, once said, where
 * the caller is. Every request and its line are the core's.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HelpSheet(
    model: NearbyModel,
    account: AccountModel,
    /** Where requests run, so that one carries on when the sheet is closed. */
    requests: CoroutineScope,
    opening: HelpOpening,
    onDismiss: () -> Unit,
    onSignIn: () -> Unit,
    /** Opens a pilot's page: the handle and the name. */
    onOpenPilot: (String, String) -> Unit,
) {
    var shown by remember { mutableStateOf(opening) }
    var reporting by remember { mutableStateOf<Reported?>(null) }
    val back: () -> Unit = { if (shown != opening && shown is HelpOpening.One) shown = opening else onDismiss() }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
        modifier = Modifier.testTag("help"),
    ) {
        BackHandler(enabled = shown != opening, onBack = back)
        Box(Modifier.fillMaxHeight()) {
            Column {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        HelpWords.TITLE,
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).semantics { heading() },
                    )
                    TextButton(onClick = onDismiss, modifier = Modifier.testTag("help-done")) { Text(Common.words.done) }
                }
                Column(
                    Modifier
                        .fillMaxHeight()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = VentiSpace.s4)
                        .navigationBarsPadding(),
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s4),
                ) {
                    Emergency()
                    when {
                        !model.isSignedIn -> SignedOut(onSignIn = {
                            onDismiss()
                            onSignIn()
                        })
                        else -> when (val page = shown) {
                            is HelpOpening.Home -> Home(
                                model,
                                requests,
                                page.landedAt,
                                onOpen = { shown = HelpOpening.One(it) },
                                onOpenPilot = { handle, name ->
                                    onDismiss()
                                    onOpenPilot(handle, name)
                                },
                            )
                            is HelpOpening.One -> OneRequest(
                                model,
                                requests,
                                page.id,
                                onReport = { reporting = Reported.helpRequest(page.id) },
                                onOpenPilot = { handle, name ->
                                    onDismiss()
                                    onOpenPilot(handle, name)
                                },
                            )
                        }
                    }
                    Spacer(Modifier.size(VentiSpace.s4))
                }
            }
        }
    }

    // A request reported to whoever runs Venti: its note, never where.
    reporting?.let { reported ->
        ReportSheet(reported, account, account.account?.server, requests, onDismiss = { reporting = null })
    }
}

/** The core's words first, in the warning's colours, with the button that dials 112. */
@Composable
private fun Emergency() {
    val colours = Venti.colours
    val context = LocalContext.current
    val words = remember { helpEmergencyWords() }
    Surface(
        shape = MaterialTheme.shapes.medium,
        color = colours.dangerSoft,
        contentColor = colours.danger,
        modifier = Modifier.fillMaxWidth().testTag("help-emergency"),
    ) {
        Column(Modifier.padding(VentiSpace.s4), verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
            Row(verticalAlignment = Alignment.Top) {
                Icon(painterResource(R.drawable.venti_danger), contentDescription = null, modifier = Modifier.size(VentiSize.icon))
                Spacer(Modifier.width(VentiSpace.s3))
                Text(words, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
            }
            OutlinedButton(
                onClick = {
                    // The phone's dialler with the number in it: the pilot presses call.
                    try {
                        context.startActivity(Intent(Intent.ACTION_DIAL, "tel:112".toUri()))
                    } catch (_: ActivityNotFoundException) {
                        // A tablet without a phone: the words above still say it.
                    }
                },
                border = BorderStroke(1.dp, colours.danger),
                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-call"),
            ) {
                Text(HelpWords.CALL, color = colours.danger)
            }
        }
    }
}

@Composable
private fun SignedOut(onSignIn: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
        Text(HelpWords.SIGNED_OUT, style = MaterialTheme.typography.bodyLarge)
        Button(onClick = onSignIn, modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-sign-in")) { Text(Common.words.signIn) }
    }
}

/** The pilot's own call that stands, or the form to ask; then the calls the pilot was asked. */
@Composable
private fun Home(
    model: NearbyModel,
    requests: CoroutineScope,
    landedAt: Coordinate?,
    onOpen: (String) -> Unit,
    onOpenPilot: (String, String) -> Unit,
) {
    LaunchedEffect(model) { model.loadHelp() }
    val lists = model.help
    when {
        lists == null && model.helpProblem != null -> Problem(model.helpProblem.orEmpty(), Modifier.testTag("help-problem"))
        lists == null -> Waiting()
        else -> {
            val mine = lists.mine.firstOrNull { it.stands }
            if (mine != null) {
                Mine(mine, model, requests, onOpenPilot)
            } else {
                AskForm(model, requests, landedAt)
            }
            val asked = lists.asked.filter { it.stands }
            if (asked.isNotEmpty()) {
                Text(HelpWords.ASKED, style = MaterialTheme.typography.titleMedium, modifier = Modifier.semantics { heading() })
                for (request in asked) {
                    AskedCard(request, onOpen = { onOpen(request.id) })
                }
            }
        }
    }
}

/** The form: a note, and "Ask pilots nearby", sent with where the pilot landed or where the phone is. */
@Composable
private fun AskForm(model: NearbyModel, requests: CoroutineScope, landedAt: Coordinate?) {
    val context = LocalContext.current
    val position = remember { PilotPosition(context.applicationContext) }
    var asked by remember { mutableIntStateOf(0) }
    val allow = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { asked += 1 }
    var isLooking by remember { mutableStateOf(landedAt == null) }
    LaunchedEffect(asked) {
        if (landedAt != null) return@LaunchedEffect
        isLooking = true
        position.readIfAllowed()
        if (position.isAllowed) withTimeoutOrNull(POSITION_WAIT_MILLISECONDS) { snapshotFlow { position.place }.filterNotNull().first() }
        isLooking = false
    }
    val at = landedAt ?: position.place
    val note = model.draftNote
    var isSending by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }

    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
        OutlinedTextField(
            value = note,
            onValueChange = { model.draftNote = it },
            label = { Text(HelpWords.NOTE) },
            placeholder = { Text(HelpWords.NOTE_EXAMPLE) },
            supportingText = {
                val characters = note.codePointCount(0, note.length)
                if (characters > HelpWords.NOTE_ROOM * 4 / 5) Text("${Format.count(characters.toLong())} / ${HelpWords.NOTE_ROOM}")
            },
            minLines = 2,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            modifier = Modifier.fillMaxWidth().testTag("help-note"),
        )
        when {
            // Where the call goes from, in the numbers a phone's maps take.
            at != null -> Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("help-place")
                    .semantics(mergeDescendants = true) {},
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(HelpWords.WHERE_YOU_ARE, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                Text(
                    HelpWords.place(at),
                    style = MaterialTheme.typography.bodyLarge.numbers,
                    color = Venti.colours.textSecondary,
                )
            }
            isLooking -> Text(HelpWords.WAITING_FOR_POSITION, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
            else -> Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                Text(HelpWords.NO_POSITION, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.testTag("help-no-position"))
                OutlinedButton(
                    onClick = { allow.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION)) },
                    modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-use-position"),
                ) { Text(HelpWords.USE_POSITION) }
            }
        }
        problem?.let { Problem(it, Modifier.testTag("help-ask-problem")) }
        Button(
            onClick = {
                val from = at ?: return@Button
                isSending = true
                problem = null
                // The request goes on when the sheet is closed; the sheet shows what came of it.
                requests.launch {
                    try {
                        problem = model.attempt { model.ask(from, note.trim()) }
                    } finally {
                        isSending = false
                    }
                }
            },
            enabled = at != null && !isSending,
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("help-ask"),
        ) {
            if (isSending) {
                CircularProgressIndicator(Modifier.size(VentiSpace.s5), strokeWidth = VentiSpace.s1 / 2)
            } else {
                Text(HelpWords.ASK)
            }
        }
    }
}

/** The pilot's own call: its line, who can help, and "I'm fine, close". */
@Composable
private fun Mine(request: HelpView, model: NearbyModel, requests: CoroutineScope, onOpenPilot: (String, String) -> Unit) {
    var isClosing by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s3), modifier = Modifier.testTag("help-mine")) {
        Text(
            request.line,
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.testTag("help-mine-line"),
        )
        request.note?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.bodyLarge) }
        if (request.helpers.isNotEmpty()) {
            Text(HelpWords.CAN_HELP, style = MaterialTheme.typography.labelLarge, color = Venti.colours.textSecondary, modifier = Modifier.semantics { heading() })
            for (helper in request.helpers) {
                TextButton(
                    onClick = { onOpenPilot(helper.handle, helper.name) },
                    modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-helper"),
                ) { Text(helper.name) }
            }
        }
        problem?.let { Problem(it) }
        OutlinedButton(
            onClick = {
                isClosing = true
                problem = null
                requests.launch {
                    try {
                        problem = model.attempt { model.close(request.id) }
                    } finally {
                        isClosing = false
                    }
                }
            },
            enabled = !isClosing,
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("help-close"),
        ) { Text(HelpWords.CLOSE) }
    }
}

/** A call the pilot was asked, as a card in a list: who, the note, the line. */
@Composable
private fun AskedCard(request: HelpView, onOpen: () -> Unit) {
    Surface(
        onClick = onOpen,
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        border = BorderStroke(1.dp, Venti.colours.border),
        modifier = Modifier
            .fillMaxWidth()
            .testTag("help-asked")
            .clearAndSetSemantics {
                contentDescription = request.pilot.name
                stateDescription = listOfNotNull(request.note, request.line).joinToString(". ")
            },
    ) {
        Column(Modifier.padding(VentiSpace.s4), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
            Text(request.pilot.name, style = MaterialTheme.typography.titleMedium)
            request.note?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.bodyLarge) }
            Text(request.line, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary)
        }
    }
}

/** One request: the pilot's own, or one they were asked, with their answer and, once they can help, where. */
@Composable
private fun OneRequest(
    model: NearbyModel,
    requests: CoroutineScope,
    id: String,
    onReport: () -> Unit,
    onOpenPilot: (String, String) -> Unit,
) {
    var request by remember { mutableStateOf<HelpView?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(id, model.revision) {
        problem = model.attempt { request = model.one(id) }
    }
    val shown = request
    when {
        shown == null && problem != null -> Problem(problem.orEmpty(), Modifier.testTag("help-problem"))
        shown == null -> Waiting()
        shown.isMine && shown.stands -> Mine(shown, model, requests, onOpenPilot)
        else -> Asked(shown, model, requests, onReport, onAnswered = { request = it })
    }
}

/** A request the pilot was asked: who, the note, how far; "I can help" and "I can't"; once they can, where. */
@Composable
private fun Asked(
    request: HelpView,
    model: NearbyModel,
    requests: CoroutineScope,
    onReport: () -> Unit,
    onAnswered: (HelpView) -> Unit,
) {
    val context = LocalContext.current
    var isAnswering by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    val say: (Boolean) -> Unit = { canHelp ->
        isAnswering = true
        problem = null
        requests.launch {
            try {
                problem = model.attempt { onAnswered(model.answer(request.id, canHelp)) }
            } finally {
                isAnswering = false
            }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s3), modifier = Modifier.testTag("help-one")) {
        Text(request.pilot.name, style = MaterialTheme.typography.titleLarge)
        request.note?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("help-note-shown")) }
        Text(request.line, style = MaterialTheme.typography.bodyMedium, color = Venti.colours.textSecondary, modifier = Modifier.testTag("help-line"))
        if (!request.stands) {
            Text(HelpWords.ENDED, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("help-ended"))
        } else if (!request.isMine) {
            problem?.let { Problem(it) }
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                if (request.canHelp == true) {
                    OutlinedButton(
                        onClick = { say(false) },
                        enabled = !isAnswering,
                        modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-cannot"),
                    ) { Text(HelpWords.CANNOT) }
                } else {
                    Button(
                        onClick = { say(true) },
                        enabled = !isAnswering,
                        modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-can"),
                    ) { Text(HelpWords.CAN) }
                    OutlinedButton(
                        onClick = { say(false) },
                        enabled = !isAnswering,
                        modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-cannot"),
                    ) { Text(HelpWords.CANNOT) }
                }
            }
            request.position?.let { at ->
                Text(HelpWords.WHERE, style = MaterialTheme.typography.labelLarge, color = Venti.colours.textSecondary, modifier = Modifier.semantics { heading() })
                Text(HelpWords.place(at), style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("help-where"))
                OutlinedButton(
                    onClick = {
                        // The phone's own maps, for the way there.
                        val geo = "geo:${at.lat},${at.lon}?q=${at.lat},${at.lon}(${Uri.encode(request.pilot.name)})".toUri()
                        try {
                            context.startActivity(Intent(Intent.ACTION_VIEW, geo))
                        } catch (_: ActivityNotFoundException) {
                            problem = HelpWords.place(at)
                        }
                    },
                    modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-directions"),
                ) { Text(HelpWords.DIRECTIONS) }
            }
        }
        if (!request.isMine) {
            TextButton(onClick = onReport, modifier = Modifier.heightIn(min = VentiSize.touch).testTag("help-report")) {
                Text(HelpWords.REPORT, color = MaterialTheme.colorScheme.error)
            }
        }
    }
}

@Composable
private fun Problem(words: String, modifier: Modifier = Modifier) {
    Text(words, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error, modifier = modifier)
}

@Composable
private fun Waiting() {
    Box(Modifier.fillMaxWidth().padding(VentiSpace.s4), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
}

/** How long the phone's position is waited for, milliseconds. */
private const val POSITION_WAIT_MILLISECONDS = 5_000L
