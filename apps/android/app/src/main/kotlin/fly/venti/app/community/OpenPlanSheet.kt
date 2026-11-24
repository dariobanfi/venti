package fly.venti.app.community

import fly.venti.app.groups.GroupWords
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Common
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.design.Venti
import fly.venti.app.design.text
import fly.venti.app.design.numbers
import fly.venti.app.forecast.ForecastPage
import fly.venti.app.forecast.ForecastPlace
import fly.venti.app.groups.PlanWords
import fly.venti.app.groups.Problem
import fly.venti.app.groups.Quiet
import fly.venti.app.groups.TextRow
import fly.venti.app.groups.Waiting
import fly.venti.app.map.OverviewMap
import fly.venti.app.wind.WindModel
import fly.venti.core.Coordinate
import fly.venti.core.OpenPlanDraft
import fly.venti.core.OpenPlanView
import fly.venti.core.TodayTakeoff
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneOffset

/** How an open plan's sheet opens from Community. */
sealed interface OpenPlanOpening {
    data class Plan(val id: String) : OpenPlanOpening

    data object NewPlan : OpenPlanOpening
}

/** A page of the sheet. */
private sealed interface Page {
    data class Plan(val id: String) : Page

    /** The form: a new plan, or the pilot's own to change. */
    data class Form(val editing: OpenPlanView?) : Page

    data class Forecast(val place: ForecastPlace) : Page
}

/**
 * An open plan's page and the form for one (docs/design.md, "Open
 * plans"), one sheet with pages of its own, each with the way back in its
 * corner. The page shows everything the plan says, who posted it and who
 * is in, and the pilot's word on it; its author changes and deletes it,
 * anyone else reports it. Everyone can read it, signed in or not.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OpenPlanSheet(
    model: OpenPlansModel,
    account: AccountModel,
    /** Where requests run, so that one carries on when the sheet is closed. */
    requests: CoroutineScope,
    opening: OpenPlanOpening,
    onDismiss: () -> Unit,
    /** Opens the account, for a pilot who is not signed in. */
    onSignIn: () -> Unit,
    /** Opens the author's page: the handle and the name. */
    onOpenPilot: (String, String) -> Unit,
    /** Where the forecast for the plan's place is asked for. */
    wind: WindModel? = null,
) {
    var path by remember {
        mutableStateOf(
            listOf(
                when (opening) {
                    is OpenPlanOpening.Plan -> Page.Plan(opening.id)
                    OpenPlanOpening.NewPlan -> Page.Form(editing = null)
                },
            ),
        )
    }
    var reporting by remember { mutableStateOf<Reported?>(null) }
    val page = path.last()
    val open: (Page) -> Unit = { next -> path = path + next }
    val back: () -> Unit = { if (path.size > 1) path = path.dropLast(1) else onDismiss() }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        BackHandler(enabled = path.size > 1, onBack = back)
        Box(Modifier.fillMaxHeight()) {
            if (page is Page.Forecast && wind != null) {
                ForecastPage(page.place, wind, onBack = back, onDone = onDismiss)
                return@Box
            }
            Column {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = if (path.size > 1) VentiSpace.s1 else VentiSpace.s5, end = VentiSpace.s2),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (path.size > 1) {
                        IconButton(onClick = back) {
                            Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                        }
                    }
                    Text(
                        when (page) {
                            is Page.Plan -> OpenPlanWords.PLAN
                            is Page.Form -> if (page.editing == null) OpenPlanWords.NEW else OpenPlanWords.CHANGE
                            is Page.Forecast -> page.place.name
                        },
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = onDismiss, modifier = Modifier.testTag("open-plan-done")) { Text(Common.words.done) }
                }
                when (page) {
                    is Page.Plan -> OpenPlanPage(
                        page.id,
                        model,
                        requests,
                        onEdit = { open(Page.Form(editing = it)) },
                        onGone = onDismiss,
                        onForecast = wind?.let { { place -> open(Page.Forecast(place)) } },
                        onReport = { reporting = it },
                        onSignIn = {
                            onDismiss()
                            onSignIn()
                        },
                        onOpenPilot = { handle, name ->
                            onDismiss()
                            onOpenPilot(handle, name)
                        },
                    )
                    is Page.Form -> OpenPlanForm(
                        page.editing,
                        model,
                        requests,
                        onSaved = { saved ->
                            // A new plan's page comes in place of the form; a changed plan's page is the one before.
                            path = if (page.editing == null) path.dropLast(1) + Page.Plan(saved.id) else path.dropLast(1)
                        },
                    )
                    is Page.Forecast -> Unit
                }
            }
        }
    }

    // An open plan reported to whoever runs the server it is on: Venti.
    reporting?.let { reported ->
        ReportSheet(reported, account, account.account?.server, requests, onDismiss = { reporting = null })
    }
}

/** One open plan: everything it says, who posted it, who is in, and the pilot's own word on it. */
@Composable
private fun OpenPlanPage(
    id: String,
    model: OpenPlansModel,
    requests: CoroutineScope,
    onEdit: (OpenPlanView) -> Unit,
    onGone: () -> Unit,
    onForecast: ((ForecastPlace) -> Unit)?,
    onReport: (Reported) -> Unit,
    onSignIn: () -> Unit,
    onOpenPilot: (String, String) -> Unit,
) {
    // The plan as the list had it, until the server has said it again.
    var plan by remember { mutableStateOf(model.listed(id)) }
    var missing by remember { mutableStateOf<String?>(null) }
    var isWorking by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var isConfirmingDelete by remember { mutableStateOf(false) }
    val signedIn = model.isSignedIn

    // Read when the page opens, again after every change and when the
    // pilot signs in or out: whose it is and who is "You" follow the account.
    LaunchedEffect(id, model.revision, signedIn) {
        try {
            plan = model.plan(id)
            missing = null
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            plan = null
            missing = model.words(error)
        }
    }

    fun act(thenLeave: Boolean = false, work: suspend () -> Unit) {
        if (isWorking) return
        isWorking = true
        problem = null
        requests.launch {
            try {
                problem = model.attempt(work)
                if (problem == null && thenLeave) onGone()
            } finally {
                isWorking = false
            }
        }
    }

    val colours = Venti.colours
    Column(
        Modifier
            .testTag("open-plan")
            .verticalScroll(rememberScrollState())
            .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s7),
    ) {
        val shown = plan
        when {
            shown == null && missing != null -> Quiet(missing.orEmpty(), Modifier.testTag("open-plan-missing"))
            shown == null -> Waiting()
            else -> {
                // The day and time large, then the place: as the card has them.
                Column(
                    Modifier
                        .fillMaxWidth()
                        .padding(top = VentiSpace.s3)
                        .testTag("open-plan-head")
                        .clearAndSetSemantics {
                            contentDescription = shown.place
                            stateDescription = listOfNotNull(PlanWords.title(shown.day, model.today()), shown.meetAt?.let { "At $it" }).joinToString(". ")
                        },
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            PlanWords.title(shown.day, model.today()),
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.SemiBold,
                            color = colours.link,
                            modifier = Modifier.weight(1f),
                        )
                        shown.meetAt?.let { time -> Text(time, style = VentiType.instrumentM.text, color = colours.text, maxLines = 1) }
                    }
                    Text(shown.place, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold, color = colours.text)
                }
                Group(Modifier.padding(top = VentiSpace.s4)) {
                    row {
                        // Who posted it, with their face; another pilot's leads to their page.
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .then(if (shown.isMine) Modifier else Modifier.clickable(role = Role.Button) { onOpenPilot(shown.authorHandle, shown.author) })
                                .heightIn(min = VentiSize.touch)
                                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s2)
                                .testTag("open-plan-author"),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            PilotFace(shown.author, if (shown.isMine) null else PilotPictures.of(shown.authorHandle), size = FaceSize.Medium)
                            Spacer(Modifier.width(VentiSpace.s3))
                            Column(Modifier.weight(1f)) {
                                Text(OpenPlanWords.POSTED_BY, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                                Text(if (shown.isMine) OpenPlanWords.YOU else shown.author, style = MaterialTheme.typography.bodyLarge)
                            }
                            if (!shown.isMine) Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, tint = colours.textSecondary)
                        }
                    }
                    row { ValueRow(OpenPlanWords.OFFER, shown.offerWords, Modifier.testTag("open-plan-offer")) }
                    row { ValueRow(OpenPlanWords.ON_THE_MAP, Format.place(shown.position.lat, shown.position.lon)) }
                }
                if (onForecast != null) {
                    Group(Modifier.padding(top = VentiSpace.s4)) {
                        row {
                            LinkRow(
                                OpenPlanWords.FORECAST,
                                R.drawable.ic_chevron_right,
                                Modifier.testTag("open-plan-forecast"),
                                leading = R.drawable.venti_forecast,
                            ) { onForecast(ForecastPlace(shown.place, shown.position)) }
                        }
                    }
                }
                shown.note?.takeIf { it.isNotBlank() }?.let { note ->
                    SectionHeader(OpenPlanWords.NOTE)
                    Group { row { TextRow(note, Modifier.testTag("open-plan-note-text")) } }
                }
                SectionHeader(OpenPlanWords.WHO_IS_IN)
                Group {
                    if (shown.pilotsIn.isEmpty()) {
                        row { TextRow(OpenPlanWords.NOBODY, isQuiet = true) }
                    }
                    for (name in shown.pilotsIn) {
                        row { TextRow(name, Modifier.testTag("open-plan-pilot")) }
                    }
                    row {
                        if (signedIn) {
                            ActionRow(
                                if (shown.amIn) GroupWords.core.imOut else GroupWords.core.imIn,
                                MaterialTheme.colorScheme.primary,
                                Modifier.testTag("open-plan-in"),
                                enabled = !isWorking,
                            ) {
                                act { plan = model.setIn(shown.id, !shown.amIn) }
                            }
                        } else {
                            ActionRow(OpenPlanWords.SIGN_IN_TO_JOIN, MaterialTheme.colorScheme.primary, Modifier.testTag("open-plan-sign-in"), onClick = onSignIn)
                        }
                    }
                }
                // Said before the pilot says "I'm in": everyone reads it, with their name.
                model.near?.publicWords?.takeIf { it.isNotEmpty() }?.let { SectionFooter(it, Modifier.testTag("open-plan-public")) }
                if (shown.isMine) {
                    Group(Modifier.padding(top = VentiSpace.s4)) {
                        row {
                            ActionRow(OpenPlanWords.CHANGE, MaterialTheme.colorScheme.primary, Modifier.testTag("open-plan-change"), enabled = !isWorking) {
                                onEdit(shown)
                            }
                        }
                        row {
                            ActionRow(OpenPlanWords.DELETE, MaterialTheme.colorScheme.error, Modifier.testTag("open-plan-delete"), enabled = !isWorking) {
                                isConfirmingDelete = true
                            }
                        }
                    }
                } else {
                    // Another pilot's plan that should not be there, to whoever runs the server.
                    val report = Reported.openPlan(shown.id)
                    Group(Modifier.padding(top = VentiSpace.s4)) {
                        row {
                            ActionRow(report.title, MaterialTheme.colorScheme.primary, Modifier.testTag("open-plan-report"), enabled = !isWorking) {
                                onReport(report)
                            }
                        }
                    }
                }
                Problem(problem)
            }
        }
    }

    val shown = plan
    if (isConfirmingDelete && shown != null) {
        AlertDialog(
            onDismissRequest = { isConfirmingDelete = false },
            title = { Text(OpenPlanWords.DELETE_QUESTION) },
            text = { Text(OpenPlanWords.DELETE_MEANS) },
            confirmButton = {
                TextButton(
                    onClick = {
                        isConfirmingDelete = false
                        act(thenLeave = true) { model.delete(shown.id) }
                    },
                    modifier = Modifier.testTag("open-plan-delete-confirm"),
                ) {
                    Text(OpenPlanWords.DELETE, color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { isConfirmingDelete = false }) { Text(Common.words.cancel) } },
        )
    }
}

/**
 * The form for an open plan: a new one, or the pilot's own to change. It
 * is the group plan's form with three differences: the place is where on
 * the map it is, from a take-off near or from where the map is; the seats
 * free in the pilot's car, or "Looking for company"; and a short note.
 * Above "Post" the core's words say that everyone can read it, with the
 * pilot's handle and name. What a plan may hold is the core's to say: a
 * refusal stands over the button in its words.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun OpenPlanForm(
    editing: OpenPlanView?,
    model: OpenPlansModel,
    requests: CoroutineScope,
    onSaved: (OpenPlanView) -> Unit,
) {
    // Where the form opens: the plan's place, else Community's, else where the map is.
    val start = remember { editing?.position ?: model.place ?: OverviewMap.lastLookedAt }
    var position by remember { mutableStateOf(start) }
    var place by remember { mutableStateOf(editing?.place.orEmpty()) }
    var day by remember { mutableStateOf(editing?.day?.let(PlanWords::date) ?: model.today()) }
    var time by remember { mutableStateOf(editing?.meetAt?.let(PlanWords::time)) }
    // The seats offered; `null` while the pilot is looking for company.
    var seats by remember { mutableStateOf(editing?.seats?.toInt()) }
    var note by remember { mutableStateOf(editing?.note.orEmpty()) }
    var isSaving by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var isPickingDay by remember { mutableStateOf(false) }
    var isPickingTime by remember { mutableStateOf(false) }
    var isPickingTakeoff by remember { mutableStateOf(false) }
    var takeoffs by remember { mutableStateOf<List<TodayTakeoff>>(emptyList()) }
    LaunchedEffect(start) {
        takeoffs = try {
            model.takeoffsNear(start)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Exception) {
            emptyList()
        }
    }
    // A change takes the refusal away: it was about what stood before.
    val changed = { problem = null }

    Column(
        Modifier
            .testTag("open-plan-form")
            .verticalScroll(rememberScrollState())
            .padding(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s7),
    ) {
        OutlinedTextField(
            value = place,
            onValueChange = {
                place = it
                changed()
            },
            label = { Text(OpenPlanWords.WHERE) },
            placeholder = { Text(OpenPlanWords.PLACE_HINT) },
            singleLine = true,
            enabled = !isSaving,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words),
            modifier = Modifier.fillMaxWidth().padding(top = VentiSpace.s4).testTag("open-plan-place"),
        )
        Group(Modifier.padding(top = VentiSpace.s4)) {
            if (takeoffs.isNotEmpty()) {
                row {
                    Box {
                        LinkRow(
                            OpenPlanWords.TAKEOFF_NEAR,
                            R.drawable.ic_arrow_drop_down,
                            Modifier.testTag("open-plan-takeoff"),
                            leading = R.drawable.venti_takeoff,
                        ) { isPickingTakeoff = true }
                        DropdownMenu(expanded = isPickingTakeoff, onDismissRequest = { isPickingTakeoff = false }) {
                            for (takeoff in takeoffs.take(OpenPlanWords.TAKEOFFS)) {
                                DropdownMenuItem(
                                    text = { Text(takeoff.site.name) },
                                    onClick = {
                                        isPickingTakeoff = false
                                        place = takeoff.site.name
                                        position = takeoff.site.position
                                        changed()
                                    },
                                    modifier = Modifier.testTag("open-plan-takeoff-choice"),
                                )
                            }
                        }
                    }
                }
            }
            row {
                LinkRow(
                    OpenPlanWords.MAP_MIDDLE,
                    R.drawable.ic_my_location,
                    Modifier.testTag("open-plan-map-middle"),
                    leading = R.drawable.ic_pin_drop,
                ) {
                    position = OverviewMap.lastLookedAt
                    changed()
                }
            }
            row {
                ValueRow(OpenPlanWords.ON_THE_MAP, Format.place(position.lat, position.lon), Modifier.testTag("open-plan-position"))
            }
        }
        SectionFooter(OpenPlanWords.MAP_MIDDLE_NOTE)
        Group(Modifier.padding(top = VentiSpace.s4)) {
            row {
                ValueRow("Day", PlanWords.title(PlanWords.day(day), model.today()), Modifier.testTag("open-plan-day"), onClick = { isPickingDay = true })
            }
            row {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("open-plan-has-time")
                        .toggleable(
                            value = time != null,
                            enabled = !isSaving,
                            role = Role.Switch,
                            onValueChange = { wanted ->
                                time = if (wanted) LocalTime.now().let { it.hour to it.minute } else null
                                changed()
                            },
                        )
                        .heightIn(min = VentiSize.touch)
                        .padding(horizontal = VentiSpace.s4),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Set a time", style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                    Switch(checked = time != null, onCheckedChange = null)
                }
            }
            time?.let { (hour, minute) ->
                row { ValueRow("Time", PlanWords.timeOfDay(hour, minute), onClick = { isPickingTime = true }) }
            }
        }
        SectionHeader(OpenPlanWords.OFFER)
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
            SegmentedButton(
                selected = seats != null,
                onClick = {
                    if (seats == null) seats = OpenPlanWords.SEAT_COUNTS.first
                    changed()
                },
                shape = SegmentedButtonDefaults.itemShape(0, 2),
                enabled = !isSaving,
                modifier = Modifier.testTag("open-plan-offers-seats"),
            ) { Text(OpenPlanWords.SEATS_FREE, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            SegmentedButton(
                selected = seats == null,
                onClick = {
                    seats = null
                    changed()
                },
                shape = SegmentedButtonDefaults.itemShape(1, 2),
                enabled = !isSaving,
                modifier = Modifier.testTag("open-plan-company"),
            ) { Text(OpenPlanWords.COMPANY, maxLines = 1, overflow = TextOverflow.Ellipsis) }
        }
        seats?.let { count ->
            Group(Modifier.padding(top = VentiSpace.s3)) {
                row {
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).padding(start = VentiSpace.s4, end = VentiSpace.s1),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(OpenPlanWords.SEATS, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                        IconButton(
                            onClick = {
                                seats = count - 1
                                changed()
                            },
                            enabled = !isSaving && count > OpenPlanWords.SEAT_COUNTS.first,
                            modifier = Modifier.testTag("open-plan-seats-fewer"),
                        ) { Icon(painterResource(R.drawable.ic_remove), contentDescription = OpenPlanWords.FEWER) }
                        Text(
                            Format.count(count.toLong()),
                            style = MaterialTheme.typography.titleMedium.numbers,
                            modifier = Modifier.testTag("open-plan-seats"),
                        )
                        IconButton(
                            onClick = {
                                seats = count + 1
                                changed()
                            },
                            enabled = !isSaving && count < OpenPlanWords.SEAT_COUNTS.last,
                            modifier = Modifier.testTag("open-plan-seats-more"),
                        ) { Icon(painterResource(R.drawable.ic_add), contentDescription = OpenPlanWords.MORE) }
                    }
                }
            }
        }
        SectionHeader(OpenPlanWords.NOTE)
        OutlinedTextField(
            value = note,
            onValueChange = {
                note = it
                changed()
            },
            placeholder = { Text(OpenPlanWords.NOTE_HINT) },
            enabled = !isSaving,
            minLines = 3,
            maxLines = 8,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            modifier = Modifier.fillMaxWidth().testTag("open-plan-note"),
        )
        // The core's: everyone reads it, with the pilot's handle and name.
        model.near?.publicWords?.takeIf { it.isNotEmpty() }?.let { SectionFooter(it, Modifier.testTag("open-plan-public")) }
        problem?.let { said ->
            Text(
                said,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(top = VentiSpace.s3).testTag("open-plan-problem"),
            )
        }
        Button(
            onClick = {
                isSaving = true
                problem = null
                val draft = OpenPlanDraft(
                    day = PlanWords.day(day),
                    place = place,
                    position = Coordinate(position.lat, position.lon),
                    meetAt = time?.let { (hour, minute) -> PlanWords.timeOfDay(hour, minute) },
                    seats = seats?.toUByte(),
                    note = note,
                )
                requests.launch {
                    try {
                        var saved: OpenPlanView? = null
                        problem = model.attempt {
                            saved = if (editing != null) model.change(editing.id, draft) else model.post(draft)
                        }
                        saved?.let(onSaved)
                    } finally {
                        isSaving = false
                    }
                }
            },
            enabled = !isSaving && place.isNotBlank(),
            modifier = Modifier.fillMaxWidth().padding(top = VentiSpace.s5).heightIn(min = VentiSize.touch).testTag("open-plan-save"),
        ) {
            Text(if (editing == null) OpenPlanWords.POST else OpenPlanWords.SAVE)
        }
    }

    if (isPickingDay) {
        val today = remember { model.today() }
        val picked = rememberDatePickerState(
            initialSelectedDateMillis = day.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli(),
            // An open plan is for today or a later day.
            selectableDates = object : SelectableDates {
                override fun isSelectableDate(utcTimeMillis: Long): Boolean =
                    !Instant.ofEpochMilli(utcTimeMillis).atZone(ZoneOffset.UTC).toLocalDate().isBefore(today)

                override fun isSelectableYear(year: Int): Boolean = year >= today.year
            },
        )
        DatePickerDialog(
            onDismissRequest = { isPickingDay = false },
            confirmButton = {
                TextButton(
                    onClick = {
                        picked.selectedDateMillis?.let { millis -> day = Instant.ofEpochMilli(millis).atZone(ZoneOffset.UTC).toLocalDate() }
                        isPickingDay = false
                        changed()
                    },
                ) { Text(Common.words.ok) }
            },
            dismissButton = { TextButton(onClick = { isPickingDay = false }) { Text(Common.words.cancel) } },
        ) {
            DatePicker(picked)
        }
    }

    if (isPickingTime) {
        val picked = rememberTimePickerState(initialHour = time?.first ?: 9, initialMinute = time?.second ?: 0, is24Hour = Format.uses24HourClock)
        AlertDialog(
            onDismissRequest = { isPickingTime = false },
            confirmButton = {
                TextButton(
                    onClick = {
                        time = picked.hour to picked.minute
                        isPickingTime = false
                        changed()
                    },
                ) { Text(Common.words.ok) }
            },
            dismissButton = { TextButton(onClick = { isPickingTime = false }) { Text(Common.words.cancel) } },
            text = { TimePicker(picked) },
        )
    }
}
