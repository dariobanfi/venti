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
import androidx.compose.foundation.selection.selectable
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
import fly.venti.core.SiteAdding
import fly.venti.core.SiteByPilots
import fly.venti.core.SiteConfirmFlight
import fly.venti.core.SiteConfirmations
import fly.venti.core.SiteField
import fly.venti.core.SiteFormField
import fly.venti.core.SiteHistory
import fly.venti.core.SiteOffers
import fly.venti.core.SiteRevisionView
import fly.venti.core.SiteView
import fly.venti.core.SiteWords as CoreSiteWords
import fly.venti.core.SiteWritten
import fly.venti.core.siteDraftCheck
import fly.venti.core.siteDraftWindInWords
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/**
 * What the screens for adding and correcting a site say that the core has
 * not worded: where the way to the account is named, and a heading. Every
 * word about the rule, a tier, the licence and a refusal is the core's.
 */
object SiteEditWords {
    const val NO_HISTORY = "The history could not be read"
    const val NO_CONFIRMATIONS = "The confirmations could not be read"

    /** How much of a line's room is used: "12 / 200". */
    fun room(used: Int, atMost: UInt): String = "$used / $atMost"

    /** A test's name for a field of the form. */
    fun tag(field: SiteField): String = field.name.lowercase().replace('_', '-')
}

/**
 * The entry for adding, in what a long press on the map opens: the core's
 * words, which lead to the form, or to the account where the core says
 * that the pilot has to sign in first. Shown where the core has words for
 * it and nowhere else.
 */
@Composable
fun AddSiteEntry(
    adding: SiteAdding,
    onAdd: () -> Unit,
    onSignIn: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Group(modifier.padding(horizontal = VentiSpace.s4)) {
        row {
            LinkRow(
                adding.entryWords,
                R.drawable.ic_chevron_right,
                Modifier.testTag(if (adding.leadsToSignIn) "site-add-sign-in" else "site-add-here"),
                leading = R.drawable.venti_site,
                onClick = if (adding.leadsToSignIn) onSignIn else onAdd,
            )
        }
    }
}

/**
 * Over the map while a site's place is chosen: the core's words, and the
 * button that takes the place under the mark in the middle of the map.
 */
@Composable
fun SitePlaceCard(
    words: String,
    says: CoreSiteWords,
    onUse: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
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
                ) { Text(says.cancel) }
                Button(
                    onClick = onUse,
                    modifier = Modifier.weight(1f).heightIn(min = VentiSize.touch).testTag("site-place-use"),
                ) { Text(says.usePlace) }
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
    /** The pilot wants to move the place on the map; `null` where no map is at hand to move it on. */
    onMove: (() -> Unit)?,
    /** The server took the site. */
    onWritten: (SiteWritten) -> Unit,
    /** Opens the site that stands in the way, by its identifier. */
    onOpen: (String) -> Unit,
    /** Someone else changed the site meanwhile: the page under the form shows it as it stands. */
    onStands: (SiteView) -> Unit,
    onDismiss: () -> Unit,
) {
    val form = edit.form
    val says = edit.says
    val colours = Venti.colours
    val links = LocalUriHandler.current
    // The words of the tier, the core's: a new site's as it said them with
    // the entry, a correction's as it says them of the site corrected.
    val title = entry.words.title
    val giving = entry.words.giving
    val note = entry.words.note
    val send = entry.words.send
    val showing = rememberCoroutineScope()
    var isSending by remember { mutableStateOf(false) }
    val draft = entry.draft
    // The core's check, asked again with every change once the pilot has tried.
    val faults = remember(draft, entry.comment, entry.hasTried) {
        if (entry.hasTried) siteDraftCheck(draft, entry.comment) else emptyList()
    }
    val spots = remember { SiteField.entries.associateWith { BringIntoViewRequester() } }
    // What is wrong at a field; and where nothing is, what the core says
    // of a field that someone else changed too while the form was open.
    fun fault(field: SiteField): String? =
        (faults.firstOrNull { it.field == field } ?: entry.clashes.firstOrNull { it.field == field })?.words

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
                        title,
                        style = MaterialTheme.typography.titleLarge,
                        modifier = Modifier.weight(1f).semantics { heading() }.testTag("site-form-title"),
                    )
                    TextButton(onClick = onDismiss, modifier = Modifier.testTag("site-form-cancel")) { Text(says.cancel) }
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
                                                if (onMove != null) {
                                                    TextButton(
                                                        onClick = onMove,
                                                        enabled = !isSending,
                                                        modifier = Modifier.testTag("site-move"),
                                                    ) { Text(says.movePlace) }
                                                }
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
                                Text(giving, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.testTag("site-giving"))
                                Text(
                                    note,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = colours.textSecondary,
                                    modifier = Modifier.testTag("site-giving-note"),
                                )
                            }
                        }
                        row {
                            LinkRow(form.licence, R.drawable.ic_open_in_new, Modifier.testTag("site-licence")) {
                                // No app that opens web pages: nothing to do.
                                runCatching { links.openUri(form.licenceUrl) }
                            }
                        }
                    }
                    // Someone else changed the site meanwhile: what became of
                    // the draft and what they changed, in the core's words.
                    entry.meanwhileWords?.let { words ->
                        Column(Modifier.testTag("site-meanwhile"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                            Text(words, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
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
                                            entry.inTheWay = sent.inTheWay
                                            val now = sent.now
                                            // Carried onto the site as it stands, the core's
                                            // words about that stand for the refusal's.
                                            entry.problem = if (now != null && entry.rebase(now)) null else sent.words
                                            now?.let(onStands)
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
                        Text(send)
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

/**
 * What a site's page does about a site: what the core offers for it to
 * whoever is signed in on this phone, each with the core's words, and
 * where each leads. The page shows what is offered and nothing else; the
 * core's note says why something is missing.
 */
class SitePageEdits(
    val offers: SiteOffers,
    /** Who says they took off here, once the server has answered. */
    val confirmations: SiteConfirmations?,
    /** The core's line about what was just done to the site. */
    val said: String?,
    /** Why the site could not be asked for, in the core's words. */
    val problem: String?,
    /** Whether the way to the account stands with the entries: the phone holds no session. */
    val showsSignIn: Boolean,
    val onCorrect: () -> Unit,
    val onRemove: () -> Unit,
    val onHistory: () -> Unit,
    val onReport: () -> Unit,
    val onSignIn: () -> Unit,
    /** Opens the form that says "I took off here and it was good", or says it of another day. */
    val onConfirm: () -> Unit,
    /** Asks whether the pilot's confirmation is to be taken back. */
    val onWithdraw: () -> Unit,
    /** Opens the list of who took off here. */
    val onConfirmations: () -> Unit,
) {
    /** The count of confirmations as the core's line: the list's once it is read, the site's own until then. */
    fun confirmedLine(by: SiteByPilots): String =
        confirmations?.let { it.words.ifEmpty { it.nobodyWords } } ?: by.confirmedInWords

    /** Whether the page has anything to show about confirmations. */
    fun showsConfirmed(by: SiteByPilots): Boolean =
        confirmedLine(by).isNotEmpty() || offers.confirm.isOffered || offers.hasConfirmed

    /** Whether the page has an entry or a note to show at its foot. */
    val showsEntries: Boolean
        get() = offers.correct.isOffered || offers.history.isOffered || offers.remove.isOffered ||
            offers.report.isOffered || showsSignIn || offers.note.isNotEmpty() || problem != null
}

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
 * On the page of a site of Venti's own layers: who added it and who
 * changed it last, each with its day, and under them the core's notice of
 * what a site of its tier is worth.
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

/**
 * Who took off here, on a site's page: the count as the core's line, which
 * opens the list; the sentence "I took off here and it was good" as a
 * button where the core offers it; and once the pilot has said it, the
 * core's line about that in the button's place, with the ways to change
 * the day and to take it back.
 */
@Composable
fun SiteConfirmedRows(by: SiteByPilots, edits: SitePageEdits) {
    val offers = edits.offers
    val line = edits.confirmedLine(by)
    Column(Modifier.testTag("site-confirmed"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
        if (line.isNotEmpty() || offers.hasConfirmed) {
            Group {
                if (line.isNotEmpty()) {
                    row {
                        LinkRow(line, R.drawable.ic_chevron_right, Modifier.testTag("site-confirmations-open"), onClick = edits.onConfirmations)
                    }
                }
                if (offers.hasConfirmed) {
                    row {
                        Text(
                            offers.confirmedWords,
                            style = MaterialTheme.typography.bodyLarge,
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3)
                                .testTag("site-confirmed-words"),
                        )
                    }
                    if (offers.confirm.isOffered) {
                        row {
                            ActionRow(
                                offers.confirm.words,
                                MaterialTheme.colorScheme.primary,
                                Modifier.testTag("site-confirm-again"),
                                onClick = edits.onConfirm,
                            )
                        }
                    }
                    if (offers.withdraw.isOffered) {
                        row {
                            ActionRow(
                                offers.withdraw.words,
                                MaterialTheme.colorScheme.error,
                                Modifier.testTag("site-withdraw"),
                                onClick = edits.onWithdraw,
                            )
                        }
                    }
                }
            }
        }
        if (offers.confirm.isOffered && !offers.hasConfirmed) {
            OutlinedButton(
                onClick = edits.onConfirm,
                modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("site-confirm"),
            ) { Text(offers.confirm.words) }
        }
    }
}

/**
 * What the core offers for a site, at the foot of its page: correct it,
 * read its history, remove it, report it, each under the core's words and
 * only where offered; and the core's one line about what is missing.
 */
@Composable
fun SiteEditRows(edits: SitePageEdits) {
    val offers = edits.offers
    val primary = MaterialTheme.colorScheme.primary
    Column(Modifier.testTag("site-edits")) {
        if (offers.correct.isOffered || offers.history.isOffered || offers.remove.isOffered || offers.report.isOffered || edits.showsSignIn) {
            Group {
                if (offers.correct.isOffered) {
                    row { ActionRow(offers.correct.words, primary, Modifier.testTag("site-correct"), onClick = edits.onCorrect) }
                }
                if (offers.history.isOffered) {
                    row {
                        LinkRow(offers.history.words, R.drawable.ic_chevron_right, Modifier.testTag("site-history-open"), onClick = edits.onHistory)
                    }
                }
                if (offers.remove.isOffered) {
                    row {
                        ActionRow(offers.remove.words, MaterialTheme.colorScheme.error, Modifier.testTag("site-remove"), onClick = edits.onRemove)
                    }
                }
                if (offers.report.isOffered) {
                    row { ActionRow(offers.report.words, primary, Modifier.testTag("site-report"), onClick = edits.onReport) }
                }
                if (edits.showsSignIn) {
                    row { ActionRow(Common.words.signIn, primary, Modifier.testTag("site-sign-in"), onClick = edits.onSignIn) }
                }
            }
        }
        if (offers.note.isNotEmpty()) {
            SectionFooter(offers.note, Modifier.testTag("site-offers-note"))
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
 * Asks before something is done to a site: removing it, putting a
 * revision back, bringing a removed site back, undoing the last change,
 * taking a confirmation back. The question and what it means are the
 * core's; with `asksWhy` the form's own "Why" field takes the line about
 * the change. It stays open while the request runs and says what the
 * server refused.
 */
@Composable
fun SiteWhyDialog(
    title: String,
    /** What the step does, under the question; `null` for none. */
    about: String?,
    /** The words of the button that sends. */
    send: String,
    cancel: String,
    why: SiteFormField?,
    isRemoval: Boolean,
    requests: CoroutineScope,
    /** Sends with the line; returns what went wrong in words, or `null` when nothing did. */
    work: suspend (String) -> String?,
    onDismiss: () -> Unit,
    asksWhy: Boolean = true,
) {
    var comment by remember { mutableStateOf("") }
    var isWorking by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = { if (!isWorking) onDismiss() },
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                if (about != null) {
                    Text(about, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.testTag("site-why-about"))
                }
                if (asksWhy) {
                    OutlinedTextField(
                        value = comment,
                        onValueChange = { comment = it },
                        label = why?.let { field -> { Text(field.title) } },
                        supportingText = why?.let { field -> { Text(field.help) } },
                        enabled = !isWorking,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        modifier = Modifier.fillMaxWidth().testTag("site-why"),
                    )
                }
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
        dismissButton = { TextButton(onClick = onDismiss, enabled = !isWorking) { Text(cancel) } },
    )
}

/** The head of a page laid over a site's: the way back, a title with the site's name under it, and "Done". */
@Composable
private fun OverASiteHead(title: String, name: String, onBack: (() -> Unit)?, onDone: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = if (onBack != null) VentiSpace.s1 else VentiSpace.s5, end = VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) {
            IconButton(onClick = onBack) {
                Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
            }
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
            Text(
                name,
                style = MaterialTheme.typography.bodyMedium,
                color = Venti.colours.textSecondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        TextButton(onClick = onDone) { Text(Common.words.done) }
    }
}

/** What the pilot is asked before a revision is put back: the revision, and the core's words around the step. */
private class PuttingBack(val revision: UInt, val question: String, val about: String?, val send: String)

/**
 * A site's history: its revisions, the newest first, each in the core's
 * line with what it changed under it, and the ways back, each under the
 * core's words: "Bring this site back" on a removed site, "Undo the last
 * change" where the core names a revision for it, and the core's offer on
 * each revision that can be put back. Which of them stand for whoever is
 * signed in here is the core's one answer for the history, with its line
 * about what is missing; the page needs the site's identifier and no more,
 * so a removed site's history is read like any other. Behind a removed
 * site's history there is no page: its way back closes the sheet.
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
    val says = edit.says
    var history by remember(id) { mutableStateOf<SiteHistory?>(null) }
    var problem by remember(id) { mutableStateOf<String?>(null) }
    var attempt by remember(id) { mutableIntStateOf(0) }
    var asking by remember { mutableStateOf<PuttingBack?>(null) }
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
    // What the core offers in this history to whoever is signed in here.
    val offers = shown?.let { edit.historyOffers(it) }

    Column {
        OverASiteHead(says.history, name, if (shown?.isRemoved == true) onDone else onBack, onDone)
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
                    TextButton(onClick = { attempt += 1 }) { Text(Common.words.tryAgain) }
                }
                else -> {
                    // Why something is not offered here, in the core's line.
                    offers?.note?.takeIf { it.isNotEmpty() }?.let { words ->
                        SectionFooter(words, Modifier.testTag("site-history-note"))
                    }
                    // A removed site, as it stood before it was removed.
                    val bringBack = shown.bringBackRevision
                    if (bringBack != null && offers?.bringBack?.isOffered == true) {
                        Button(
                            onClick = { asking = PuttingBack(bringBack, says.bringBackQuestion, says.bringBackAbout, says.bringBack) },
                            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("site-bring-back"),
                        ) { Text(offers.bringBack.words) }
                    }
                    val undo = shown.undoRevision
                    if (undo != null && offers?.undo?.isOffered == true) {
                        val words = offers.undo.words
                        OutlinedButton(
                            onClick = { asking = PuttingBack(undo, "$words?", null, words) },
                            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("site-undo"),
                        ) { Text(words) }
                    }
                    for (revision in shown.revisions.asReversed()) {
                        Revision(
                            revision,
                            report = offers?.reportRevision?.words.orEmpty(),
                            onPutBack = if (revision.canBePutBack && offers?.putBack?.isOffered == true) {
                                { asking = PuttingBack(revision.number, says.putBackQuestion, says.putBackAbout, says.putBack) }
                            } else {
                                null
                            },
                            onReport = if (offers?.reportRevision?.isOffered == true) {
                                { onReport(Reported.siteRevision(id, revision.number)) }
                            } else {
                                null
                            },
                        )
                    }
                    shown.credit?.let { SectionFooter(it.text) }
                }
            }
        }
    }

    asking?.let { asked ->
        SiteWhyDialog(
            title = asked.question,
            about = asked.about,
            send = asked.send,
            cancel = says.cancel,
            why = edit.form.fields.firstOrNull { it.field == SiteField.COMMENT },
            isRemoval = false,
            requests = requests,
            work = { comment ->
                try {
                    val written = edit.restore(id, asked.revision, comment)
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

/** One revision: the core's line, what it changed a line a field, and what can be done about it, in the core's words. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Revision(revision: SiteRevisionView, report: String, onPutBack: (() -> Unit)?, onReport: (() -> Unit)?) {
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
                // Side by side where there is room, one under the other where there is not.
                FlowRow(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    if (onReport != null) {
                        TextButton(onClick = onReport, modifier = Modifier.testTag("site-revision-report")) { Text(report) }
                    }
                    if (onPutBack != null) {
                        TextButton(onClick = onPutBack, modifier = Modifier.testTag("site-put-back")) {
                            Text(revision.putBackWords)
                        }
                    }
                }
            }
        }
    }
}

/**
 * "Pilots who took off here": the count as the core's line and each
 * confirmation in the core's line, the newest day first, and under them
 * the core's sentence where the list does not hold them all. Anyone reads
 * it.
 */
@Composable
fun SiteConfirmationsPage(
    name: String,
    says: CoreSiteWords,
    confirmations: SiteConfirmations?,
    /** Why the list could not be read, in the core's words. */
    problem: String?,
    onTryAgain: () -> Unit,
    onBack: (() -> Unit)?,
    onDone: () -> Unit,
) {
    Column {
        OverASiteHead(says.confirmationsTitle, name, onBack, onDone)
        Column(
            modifier = Modifier
                .testTag("site-confirmations")
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            when {
                confirmations == null && problem == null -> Box(
                    Modifier.fillMaxWidth().padding(vertical = VentiSpace.s6),
                    contentAlignment = Alignment.Center,
                ) { CircularProgressIndicator() }
                confirmations == null -> {
                    Text(SiteEditWords.NO_CONFIRMATIONS, style = MaterialTheme.typography.titleMedium)
                    Text(problem.orEmpty(), color = Venti.colours.textSecondary, modifier = Modifier.testTag("site-confirmations-problem"))
                    TextButton(onClick = onTryAgain) { Text(Common.words.tryAgain) }
                }
                else -> {
                    Text(
                        confirmations.words.ifEmpty { confirmations.nobodyWords },
                        style = MaterialTheme.typography.bodyLarge,
                        modifier = Modifier.testTag("site-confirmations-count"),
                    )
                    if (confirmations.lines.isNotEmpty()) {
                        Group {
                            for (line in confirmations.lines) {
                                row {
                                    Text(
                                        line.words,
                                        style = MaterialTheme.typography.bodyLarge.numbers,
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3)
                                            .testTag("site-confirmation"),
                                    )
                                }
                            }
                        }
                    }
                    if (confirmations.notWholeWords.isNotEmpty()) {
                        SectionFooter(confirmations.notWholeWords, Modifier.testTag("site-confirmations-not-whole"))
                    }
                }
            }
        }
    }
}

/**
 * The small form that says of a site "I took off here and it was good":
 * the day from a menu, today chosen; a line for other pilots, if the pilot
 * likes, with the room it has; one of the pilot's own flights of that day
 * that began here, only where the logbook has one; and the core's button.
 * Every word and every refusal is the core's.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SiteConfirmSheet(
    site: SiteView,
    edit: SiteEditModel,
    /** Where the request runs, so that it carries on when the sheet is closed. */
    requests: CoroutineScope,
    /** The server took it: the site with it counted, and the core's line. */
    onConfirmed: (SiteWritten) -> Unit,
    onDismiss: () -> Unit,
) {
    val colours = Venti.colours
    val form = remember { runCatching { edit.confirmForm() }.getOrNull() }
    val today = form?.days?.firstOrNull()
    if (form == null || today == null) {
        // The core could not make the form of this phone's date: nothing to show.
        LaunchedEffect(Unit) { onDismiss() }
        return
    }
    var day by remember { mutableStateOf(today) }
    var isChoosingDay by remember { mutableStateOf(false) }
    var note by remember { mutableStateOf("") }
    var flights by remember { mutableStateOf(emptyList<SiteConfirmFlight>()) }
    var flight by remember { mutableStateOf<String?>(null) }
    var isSending by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    // The pilot's own flights of the day chosen that began here: mostly none.
    LaunchedEffect(site.id, day.day) {
        flight = null
        flights = edit.confirmFlights(site, day.day)
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                .testTag("site-confirm-form")
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s6),
            verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    form.title,
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = onDismiss, modifier = Modifier.testTag("site-confirm-cancel")) { Text(edit.says.cancel) }
            }
            Text(site.name, style = MaterialTheme.typography.bodyLarge)
            Text(
                form.about,
                style = MaterialTheme.typography.bodyMedium,
                color = colours.textSecondary,
                modifier = Modifier.testTag("site-confirm-about"),
            )

            Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                Text(
                    form.dayTitle,
                    style = MaterialTheme.typography.labelLarge,
                    color = colours.textSecondary,
                    modifier = Modifier.semantics { heading() },
                )
                Box {
                    Group {
                        row {
                            LinkRow(
                                day.words,
                                R.drawable.ic_arrow_drop_down,
                                Modifier.testTag("site-confirm-day"),
                                onClick = { if (!isSending) isChoosingDay = true },
                            )
                        }
                    }
                    DropdownMenu(expanded = isChoosingDay, onDismissRequest = { isChoosingDay = false }) {
                        for (one in form.days) {
                            DropdownMenuItem(
                                text = { Text(one.words) },
                                onClick = {
                                    day = one
                                    isChoosingDay = false
                                },
                                modifier = Modifier.testTag("site-confirm-day-${one.day}"),
                            )
                        }
                    }
                }
            }

            OutlinedTextField(
                value = note,
                // No more than the core says the line has room for.
                onValueChange = { note = it.take(form.noteAtMost.toInt()) },
                label = { Text(form.noteTitle) },
                supportingText = {
                    Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2)) {
                        Text(form.noteHelp, modifier = Modifier.weight(1f))
                        Text(SiteEditWords.room(note.length, form.noteAtMost), style = MaterialTheme.typography.bodySmall.numbers)
                    }
                },
                enabled = !isSending,
                minLines = 2,
                maxLines = 6,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                modifier = Modifier.fillMaxWidth().testTag("site-confirm-note"),
            )

            if (flights.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
                    Text(
                        form.flightTitle,
                        style = MaterialTheme.typography.labelLarge,
                        color = colours.textSecondary,
                        modifier = Modifier.semantics { heading() },
                    )
                    Group(Modifier.testTag("site-confirm-flights")) {
                        row { Picked(form.noFlight, isOn = flight == null, enabled = !isSending) { flight = null } }
                        for (one in flights) {
                            row { Picked(one.words, isOn = flight == one.id, enabled = !isSending) { flight = one.id } }
                        }
                    }
                    Text(form.flightHelp, style = MaterialTheme.typography.bodySmall, color = colours.textSecondary)
                }
            }

            problem?.let { words ->
                Text(
                    words,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.testTag("site-confirm-problem"),
                )
            }
            Button(
                onClick = {
                    isSending = true
                    problem = null
                    requests.launch {
                        try {
                            onConfirmed(edit.confirm(site.id, day.day, note, flight))
                        } catch (error: CancellationException) {
                            throw error
                        } catch (error: Exception) {
                            problem = edit.words(error)
                        } finally {
                            isSending = false
                        }
                    }
                },
                enabled = !isSending,
                modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("site-confirm-send"),
            ) { Text(form.send) }
        }
    }
}

/** One of several to pick one from, in a group's row: its words, and a tick at the one that is picked. */
@Composable
private fun Picked(words: String, isOn: Boolean, enabled: Boolean, onPick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .selectable(selected = isOn, enabled = enabled, role = Role.RadioButton, onClick = onPick)
            .heightIn(min = VentiSize.touch)
            .padding(horizontal = VentiSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(words, style = MaterialTheme.typography.bodyLarge.numbers, modifier = Modifier.weight(1f))
        if (isOn) {
            Icon(painterResource(R.drawable.ic_check), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
        }
    }
}
