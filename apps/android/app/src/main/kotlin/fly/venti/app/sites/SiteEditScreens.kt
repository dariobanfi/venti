package fly.venti.app.sites

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.community.Reported
import fly.venti.app.design.ActionRow
import fly.venti.app.design.FloatElevation
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.ValueRow
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.core.SiteByPilots
import fly.venti.core.SiteField
import fly.venti.core.SiteFormField
import fly.venti.core.SiteHistory
import fly.venti.core.SiteRevisionView
import fly.venti.core.SiteWritten
import fly.venti.core.siteDraftCheck
import fly.venti.core.siteDraftWindInWords
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * What the screens for adding and correcting a site say that the core has
 * not worded: the names of buttons. Every word about the rule, the licence
 * and a refusal is the core's.
 */
object SiteEditWords {
    const val ADD_HERE = "Add a site here"

    /** Why the entry leads to the account, signed out. */
    const val NEEDS_ACCOUNT = "Adding and correcting a site needs an account. Sign in first."
    const val SIGN_IN = "Sign in"
    const val MOVE = "Move"
    const val USE_PLACE = "Use this place"
    const val HISTORY = "History"
    const val REMOVE = "Remove this site"
    const val REMOVE_ASK = "Remove this site?"
    const val PUT_BACK = "Put back"
    const val PUT_BACK_ASK = "Put the site back to this?"
    const val REPORT = "Report"

    /** Over the core's lines of what another pilot changed while the form was open. */
    const val MEANWHILE = "Changed meanwhile, and in the form now. What you entered is still there:"
    const val NO_HISTORY = "The history could not be read"

    /** A test's name for a field of the form. */
    fun tag(field: SiteField): String = field.name.lowercase().replace('_', '-')
}

/**
 * "Add a site here", in what a long press on the map opens: the form for a
 * signed-in pilot, and the way to the account with a line why for one who
 * is not.
 */
@Composable
fun AddSiteEntry(isSignedIn: Boolean, onAdd: () -> Unit, onSignIn: () -> Unit, modifier: Modifier = Modifier) {
    Column(modifier.padding(horizontal = VentiSpace.s4)) {
        Group {
            row {
                LinkRow(
                    SiteEditWords.ADD_HERE,
                    R.drawable.ic_chevron_right,
                    Modifier.testTag("site-add-here"),
                    leading = R.drawable.venti_site,
                    onClick = if (isSignedIn) onAdd else onSignIn,
                )
            }
        }
        if (!isSignedIn) {
            SectionFooter(SiteEditWords.NEEDS_ACCOUNT, Modifier.testTag("site-add-needs-account"))
        }
    }
}

/**
 * Over the map while a site's place is chosen: the core's words, and the
 * button that takes the place under the mark in the middle of the map.
 */
@Composable
fun SitePlaceCard(words: String, onUse: () -> Unit, onCancel: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth().testTag("site-place-card"),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surface,
        shadowElevation = FloatElevation,
    ) {
        Column(Modifier.padding(VentiSpace.s4), verticalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
            Text(words, style = MaterialTheme.typography.bodyLarge)
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3)) {
                OutlinedButton(
                    onClick = onCancel,
                    modifier = Modifier.weight(1f).heightIn(min = VentiSize.touch),
                ) { Text("Cancel") }
                Button(
                    onClick = onUse,
                    modifier = Modifier.weight(1f).heightIn(min = VentiSize.touch).testTag("site-place-use"),
                ) { Text(SiteEditWords.USE_PLACE) }
            }
        }
    }
}

/**
 * The form for a site, a new one or one to correct, as a sheet at full
 * height: the core's fields in the core's order with its titles and
 * helps, its choices, its faults at their fields once the pilot has tried
 * to send, and what giving a site means over the button that sends it.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class, ExperimentalFoundationApi::class)
@Composable
fun SiteFormSheet(
    entry: SiteFormDraft,
    edit: SiteEditModel,
    /** Where the request runs, so that it carries on when the sheet is closed. */
    requests: CoroutineScope,
    /** The pilot wants to move the place on the map. */
    onMove: () -> Unit,
    /** The server took the site. */
    onWritten: (SiteWritten) -> Unit,
    /** Opens the site that stands in the way, by its identifier. */
    onOpen: (String) -> Unit,
    /** Someone else changed the site meanwhile: the page under the form shows it as it stands. */
    onStands: (fly.venti.core.SiteView) -> Unit,
    onDismiss: () -> Unit,
) {
    val form = edit.form
    val colours = Venti.colours
    val links = LocalUriHandler.current
    val showing = rememberCoroutineScope()
    var isSending by remember { mutableStateOf(false) }
    val draft = entry.draft
    // The core's check, asked again with every change once the pilot has tried.
    val faults = remember(draft, entry.comment, entry.hasTried) {
        if (entry.hasTried) siteDraftCheck(draft, entry.comment) else emptyList()
    }
    val spots = remember { SiteField.entries.associateWith { BringIntoViewRequester() } }
    fun fault(field: SiteField): String? = faults.firstOrNull { it.field == field }?.words

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Box(Modifier.fillMaxHeight()) {
            Column {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        if (entry.isNew) form.titleNew else form.titleCorrect,
                        style = MaterialTheme.typography.titleLarge,
                        modifier = Modifier.weight(1f).semantics { heading() },
                    )
                    TextButton(onClick = onDismiss, modifier = Modifier.testTag("site-form-cancel")) { Text("Cancel") }
                }
                Column(
                    modifier = Modifier
                        .testTag("site-form")
                        .verticalScroll(rememberScrollState())
                        .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
                    verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                ) {
                    for (field in form.fields) {
                        val wrong = fault(field.field)
                        val tag = SiteEditWords.tag(field.field)
                        Column(Modifier.bringIntoViewRequester(checkNotNull(spots[field.field]))) {
                            when (field.field) {
                                SiteField.NAME -> Entry(field, draft.name, wrong, !isSending, tag, words = true) {
                                    entry.draft = entry.draft.copy(name = it)
                                }
                                SiteField.PLACE -> Chosen(field, wrong) {
                                    Group {
                                        row {
                                            Row(
                                                modifier = Modifier
                                                    .fillMaxWidth()
                                                    .heightIn(min = VentiSize.touch)
                                                    .padding(start = VentiSpace.s4, end = VentiSpace.s1),
                                                verticalAlignment = Alignment.CenterVertically,
                                            ) {
                                                Text(
                                                    Format.place(draft.position.lat, draft.position.lon),
                                                    style = MaterialTheme.typography.bodyLarge.numbers,
                                                    modifier = Modifier.weight(1f).testTag("site-field-place"),
                                                )
                                                TextButton(
                                                    onClick = onMove,
                                                    enabled = !isSending,
                                                    modifier = Modifier.testTag("site-move"),
                                                ) { Text(SiteEditWords.MOVE) }
                                            }
                                        }
                                    }
                                }
                                SiteField.USES -> Chosen(field, wrong) {
                                    FlowRow(
                                        modifier = Modifier.testTag("site-field-uses"),
                                        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                                    ) {
                                        for (use in form.uses) {
                                            Choice(
                                                use.words,
                                                isOn = use.kind in draft.uses,
                                                tag = "site-use-${use.kind.name.lowercase()}",
                                                ticks = true,
                                            ) {
                                                val uses = entry.draft.uses
                                                // In the form's order, whatever the order of ticking.
                                                val ticked = if (use.kind in uses) uses - use.kind else uses + use.kind
                                                entry.draft = entry.draft.copy(uses = form.uses.map { it.kind }.filter { it in ticked })
                                            }
                                        }
                                    }
                                }
                                SiteField.HEIGHT -> Entry(
                                    field,
                                    entry.height,
                                    wrong,
                                    !isSending,
                                    tag,
                                    keyboard = KeyboardOptions(keyboardType = KeyboardType.Number),
                                    onChange = entry::typeHeight,
                                )
                                SiteField.WIND -> Chosen(field, wrong) {
                                    FlowRow(
                                        modifier = Modifier.testTag("site-field-wind"),
                                        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                                    ) {
                                        for (wind in form.winds) {
                                            Choice(
                                                wind.words,
                                                isOn = wind.fromDeg in draft.windFromDeg,
                                                tag = "site-wind-${wind.words}",
                                                ticks = true,
                                            ) {
                                                val ticked = entry.draft.windFromDeg
                                                entry.draft = entry.draft.copy(
                                                    windFromDeg = if (wind.fromDeg in ticked) {
                                                        ticked - wind.fromDeg
                                                    } else {
                                                        (ticked + wind.fromDeg).sorted()
                                                    },
                                                )
                                            }
                                        }
                                    }
                                    // The ticked points as the core sends them: "SW to NW".
                                    val inWords = remember(draft.windFromDeg) { siteDraftWindInWords(draft.windFromDeg) }
                                    if (inWords.isNotEmpty()) {
                                        Text(
                                            inWords,
                                            style = MaterialTheme.typography.bodyMedium,
                                            color = colours.text,
                                            modifier = Modifier.testTag("site-wind-words"),
                                        )
                                    }
                                }
                                SiteField.DESCRIPTION -> Entry(field, draft.description, wrong, !isSending, tag, lines = 3) {
                                    entry.draft = entry.draft.copy(description = it)
                                }
                                SiteField.FEE -> Entry(field, draft.fee, wrong, !isSending, tag) {
                                    entry.draft = entry.draft.copy(fee = it)
                                }
                                SiteField.GUESTS -> Chosen(field, wrong) {
                                    Group(Modifier.testTag("site-field-guests")) {
                                        for (answer in form.guests) {
                                            row {
                                                Row(
                                                    modifier = Modifier
                                                        .fillMaxWidth()
                                                        .clickable(enabled = !isSending, role = Role.RadioButton) {
                                                            entry.draft = entry.draft.copy(guestGuidelines = answer.value)
                                                        }
                                                        .heightIn(min = VentiSize.touch)
                                                        .padding(horizontal = VentiSpace.s4),
                                                    verticalAlignment = Alignment.CenterVertically,
                                                ) {
                                                    Text(answer.words, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                                                    if (draft.guestGuidelines == answer.value) {
                                                        Icon(
                                                            painterResource(R.drawable.ic_check),
                                                            contentDescription = "Chosen",
                                                            tint = MaterialTheme.colorScheme.primary,
                                                        )
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                SiteField.NO_FLIGHT_TIME -> Entry(field, draft.noFlightTime, wrong, !isSending, tag) {
                                    entry.draft = entry.draft.copy(noFlightTime = it)
                                }
                                SiteField.WEBSITE -> Entry(
                                    field,
                                    draft.website,
                                    wrong,
                                    !isSending,
                                    tag,
                                    keyboard = KeyboardOptions(keyboardType = KeyboardType.Uri, autoCorrectEnabled = false),
                                ) {
                                    entry.draft = entry.draft.copy(website = it)
                                }
                                SiteField.COMMENT -> Entry(field, entry.comment, wrong, !isSending, tag) { entry.comment = it }
                            }
                        }
                    }

                    // What giving a site means, read before it is sent, with
                    // the licence to open; and what such a site is worth.
                    Group(Modifier.testTag("site-giving-card")) {
                        row {
                            Column(
                                Modifier.padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
                                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
                            ) {
                                Text(form.giving, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.testTag("site-giving"))
                                Text(form.note, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                            }
                        }
                        row {
                            LinkRow(form.licence, R.drawable.ic_open_in_new, Modifier.testTag("site-licence")) {
                                // No app that opens web pages: nothing to do.
                                runCatching { links.openUri(form.licenceUrl) }
                            }
                        }
                    }
                    if (entry.meanwhile.isNotEmpty()) {
                        Column(Modifier.testTag("site-meanwhile")) {
                            Text(SiteEditWords.MEANWHILE, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                            for (line in entry.meanwhile) {
                                Text(line, style = MaterialTheme.typography.bodyMedium.numbers)
                            }
                        }
                    }
                    entry.problem?.let { words ->
                        Text(
                            words,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.testTag("site-problem"),
                        )
                    }
                    entry.inTheWay?.let { other ->
                        OutlinedButton(
                            onClick = { onOpen(other.id) },
                            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("site-in-the-way"),
                        ) { Text(other.words) }
                    }
                    Button(
                        onClick = {
                            entry.hasTried = true
                            val found = siteDraftCheck(entry.draft, entry.comment)
                            if (found.isNotEmpty()) {
                                // The first thing that is wrong, in view.
                                showing.launch { spots[found.first().field]?.bringIntoView() }
                                return@Button
                            }
                            isSending = true
                            entry.problem = null
                            entry.inTheWay = null
                            requests.launch {
                                try {
                                    when (val sent = edit.send(entry.draft, entry.comment)) {
                                        is SiteEditModel.Sent.Written -> onWritten(sent.written)
                                        is SiteEditModel.Sent.Refused -> {
                                            entry.problem = sent.words
                                            entry.inTheWay = sent.inTheWay
                                            sent.now?.let { now ->
                                                entry.carryOnto(now)
                                                onStands(now)
                                            }
                                        }
                                    }
                                } finally {
                                    isSending = false
                                }
                            }
                        },
                        // Tappable also with something missing: the pilot learns what.
                        enabled = !isSending,
                        modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("site-send"),
                    ) {
                        Text(if (entry.isNew) form.sendNew else form.sendCorrection)
                    }
                }
            }
        }
    }
}

/** One field of the form that is text: the pack's field under the core's title, with its help or its fault under it. */
@Composable
private fun Entry(
    field: SiteFormField,
    value: String,
    fault: String?,
    enabled: Boolean,
    tag: String,
    words: Boolean = false,
    lines: Int = 1,
    keyboard: KeyboardOptions = KeyboardOptions(
        capitalization = if (words) KeyboardCapitalization.Words else KeyboardCapitalization.Sentences,
    ),
    onChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(field.title) },
        enabled = enabled,
        isError = fault != null,
        supportingText = {
            Text(fault ?: field.help, modifier = if (fault != null) Modifier.testTag("site-fault-$tag") else Modifier)
        },
        singleLine = lines == 1,
        minLines = lines,
        maxLines = if (lines == 1) 1 else 8,
        keyboardOptions = keyboard,
        modifier = Modifier.fillMaxWidth().testTag("site-field-$tag"),
    )
}

/** One field of the form that is chosen and not typed: the core's title, the choice, and the help or the fault. */
@Composable
private fun Chosen(field: SiteFormField, fault: String?, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
        Text(
            field.title,
            style = MaterialTheme.typography.labelLarge,
            color = Venti.colours.textSecondary,
            modifier = Modifier.semantics { heading() },
        )
        content()
        if (fault != null) {
            Text(
                fault,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.testTag("site-fault-${SiteEditWords.tag(field.field)}"),
            )
        } else {
            Text(field.help, style = MaterialTheme.typography.bodySmall, color = Venti.colours.textSecondary)
        }
    }
}

/** What a site's page does about a site that pilots made. */
class SitePageEdits(
    /** Whether a pilot can correct the site here, as the server's own answer about it says. */
    val canBeCorrected: Boolean,
    val isSignedIn: Boolean,
    /** "Correct this site", the core's title of the form. */
    val correct: String,
    /** The core's line about what was just done to the site. */
    val said: String?,
    /** Why the site could not be asked for, in the core's words. */
    val problem: String?,
    val onCorrect: () -> Unit,
    val onRemove: () -> Unit,
    val onHistory: () -> Unit,
    val onReport: () -> Unit,
    val onSignIn: () -> Unit,
)

/** The core's line about what was just done to a site, at the head of its page or its history. */
@Composable
fun SiteSaid(words: String, modifier: Modifier = Modifier) {
    Group(modifier) {
        row {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(painterResource(R.drawable.ic_check), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                Text(words, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("site-said"))
            }
        }
    }
}

/**
 * On the page of a site that pilots made: who added it and who changed it
 * last, each with its day, and under them the core's notice that nobody
 * has verified it.
 */
@Composable
fun SiteByPilotsRows(by: SiteByPilots) {
    Column(Modifier.testTag("site-by-pilots")) {
        Group {
            row { ValueRow(by.addedInWords, Format.day(by.addedUnixS)) }
            if (by.changedInWords.isNotEmpty()) {
                row { ValueRow(by.changedInWords, Format.day(by.changedUnixS)) }
            }
        }
        SectionFooter(by.note, Modifier.testTag("site-by-pilots-note"))
    }
}

/** What a pilot does about a site that pilots made: correct it, read its history, remove it, report it. */
@Composable
fun SiteEditRows(edits: SitePageEdits) {
    val primary = MaterialTheme.colorScheme.primary
    Column(Modifier.testTag("site-edits")) {
        Group {
            if (edits.canBeCorrected && edits.isSignedIn) {
                row { ActionRow(edits.correct, primary, Modifier.testTag("site-correct"), onClick = edits.onCorrect) }
            }
            row {
                LinkRow(SiteEditWords.HISTORY, R.drawable.ic_chevron_right, Modifier.testTag("site-history-open"), onClick = edits.onHistory)
            }
            if (edits.canBeCorrected && edits.isSignedIn) {
                row {
                    ActionRow(SiteEditWords.REMOVE, MaterialTheme.colorScheme.error, Modifier.testTag("site-remove"), onClick = edits.onRemove)
                }
            }
            row { ActionRow(Reported.site("").title, primary, Modifier.testTag("site-report"), onClick = edits.onReport) }
            if (edits.canBeCorrected && !edits.isSignedIn) {
                row { ActionRow(SiteEditWords.SIGN_IN, primary, Modifier.testTag("site-sign-in"), onClick = edits.onSignIn) }
            }
        }
        if (edits.canBeCorrected && !edits.isSignedIn) {
            SectionFooter(SiteEditWords.NEEDS_ACCOUNT)
        }
        edits.problem?.let { words ->
            Text(
                words,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.testTag("site-edit-problem").padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s2),
            )
        }
    }
}

/**
 * Asks for the line about a change before it is sent: removing a site,
 * putting a revision back, undoing the last change. The field is the
 * form's own "Why", with the core's title and help. It stays open while
 * the request runs and says what the server refused.
 */
@Composable
fun SiteWhyDialog(
    title: String,
    /** The words of the button that sends. */
    send: String,
    why: SiteFormField?,
    isRemoval: Boolean,
    requests: CoroutineScope,
    /** Sends with the line; returns what went wrong in words, or `null` when nothing did. */
    work: suspend (String) -> String?,
    onDismiss: () -> Unit,
) {
    var comment by remember { mutableStateOf("") }
    var isWorking by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = { if (!isWorking) onDismiss() },
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                OutlinedTextField(
                    value = comment,
                    onValueChange = { comment = it },
                    label = why?.let { field -> { Text(field.title) } },
                    supportingText = why?.let { field -> { Text(field.help) } },
                    enabled = !isWorking,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    modifier = Modifier.fillMaxWidth().testTag("site-why"),
                )
                problem?.let { words ->
                    Text(
                        words,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.testTag("site-why-problem"),
                    )
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = {
                    isWorking = true
                    problem = null
                    requests.launch {
                        try {
                            problem = work(comment)
                        } finally {
                            isWorking = false
                        }
                    }
                },
                enabled = !isWorking,
                modifier = Modifier.testTag("site-why-send"),
            ) {
                Text(send, color = if (isRemoval) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
            }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !isWorking) { Text("Cancel") } },
    )
}

/**
 * A site's history: its revisions, the newest first, each in the core's
 * line with what it changed under it, and the ways back: the core's
 * "Undo the last change" where it names a revision for it, and "Put back"
 * on each revision the core says can be. That is also how a removed site
 * comes back.
 */
@Composable
fun SiteHistoryPage(
    id: String,
    name: String,
    edit: SiteEditModel,
    requests: CoroutineScope,
    /** The core's line about what was just done to the site. */
    said: String?,
    onBack: (() -> Unit)?,
    onDone: () -> Unit,
    /** A revision was put back: the site as it stands now. */
    onWritten: (SiteWritten) -> Unit,
    onReport: (Reported) -> Unit,
) {
    var history by remember(id) { mutableStateOf<SiteHistory?>(null) }
    var problem by remember(id) { mutableStateOf<String?>(null) }
    var attempt by remember(id) { mutableIntStateOf(0) }
    // The revision to put back, with the words of the button that asked.
    var asking by remember { mutableStateOf<Pair<UInt, String>?>(null) }
    LaunchedEffect(id, attempt) {
        try {
            history = edit.history(id)
            problem = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            problem = edit.words(error)
        }
    }
    val shown = history
    val mayWrite = edit.takesSites() && edit.isSignedIn

    Column {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = if (onBack != null) VentiSpace.s1 else VentiSpace.s5, end = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                IconButton(onClick = onBack) {
                    Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                }
            }
            Column(Modifier.weight(1f)) {
                Text(SiteEditWords.HISTORY, style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
                Text(
                    name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = Venti.colours.textSecondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            TextButton(onClick = onDone) { Text("Done") }
        }
        Column(
            modifier = Modifier
                .testTag("site-history")
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            said?.let { SiteSaid(it) }
            when {
                shown == null && problem == null -> Box(
                    Modifier.fillMaxWidth().padding(vertical = VentiSpace.s6),
                    contentAlignment = Alignment.Center,
                ) { CircularProgressIndicator() }
                shown == null -> {
                    Text(SiteEditWords.NO_HISTORY, style = MaterialTheme.typography.titleMedium)
                    Text(problem.orEmpty(), color = Venti.colours.textSecondary, modifier = Modifier.testTag("site-history-problem"))
                    TextButton(onClick = { attempt += 1 }) { Text("Try again") }
                }
                else -> {
                    val undo = shown.undoRevision
                    if (undo != null && mayWrite) {
                        OutlinedButton(
                            onClick = { asking = undo to shown.undoWords },
                            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("site-undo"),
                        ) { Text(shown.undoWords) }
                    }
                    for (revision in shown.revisions.asReversed()) {
                        Revision(
                            revision,
                            onPutBack = if (revision.canBePutBack && mayWrite) {
                                { asking = revision.number to SiteEditWords.PUT_BACK }
                            } else {
                                null
                            },
                            onReport = { onReport(Reported.siteRevision(id, revision.number)) },
                        )
                    }
                    shown.credit?.let { SectionFooter(it.text) }
                }
            }
        }
    }

    asking?.let { (revision, words) ->
        SiteWhyDialog(
            title = if (words == SiteEditWords.PUT_BACK) SiteEditWords.PUT_BACK_ASK else "$words?",
            send = words,
            why = edit.form.fields.firstOrNull { it.field == SiteField.COMMENT },
            isRemoval = false,
            requests = requests,
            work = { comment ->
                try {
                    val written = edit.restore(id, revision, comment)
                    asking = null
                    onWritten(written)
                    null
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Exception) {
                    edit.words(error)
                }
            },
            onDismiss = { asking = null },
        )
    }
}

/** One revision: the core's line, what it changed a line a field, and what can be done about it. */
@Composable
private fun Revision(revision: SiteRevisionView, onPutBack: (() -> Unit)?, onReport: () -> Unit) {
    Group(Modifier.testTag("site-revision")) {
        row {
            Column(
                Modifier.fillMaxWidth().padding(start = VentiSpace.s4, end = VentiSpace.s2, top = VentiSpace.s3),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                Text(revision.words, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("site-revision-words"))
                for (change in revision.changes) {
                    Text(
                        change,
                        style = MaterialTheme.typography.bodySmall.numbers,
                        color = Venti.colours.textSecondary,
                        modifier = Modifier.testTag("site-revision-change"),
                    )
                }
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(onClick = onReport, modifier = Modifier.testTag("site-revision-report")) {
                        Text(SiteEditWords.REPORT)
                    }
                    if (onPutBack != null) {
                        TextButton(onClick = onPutBack, modifier = Modifier.testTag("site-put-back")) {
                            Text(SiteEditWords.PUT_BACK)
                        }
                    }
                }
            }
        }
    }
}
