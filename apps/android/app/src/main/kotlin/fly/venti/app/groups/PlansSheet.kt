package fly.venti.app.groups

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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SelectableDates
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
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.design.numbers
import fly.venti.core.PlanDraft
import fly.venti.core.PlanView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneOffset

/** A page of the sheet: the board, or something opened from it. */
private sealed interface Page {
    data object Board : Page

    data class Plan(val id: String) : Page

    /** The form for a plan: a new one, or the pilot's own to change. */
    data class Form(val editing: String?) : Page

    data object Groups : Page

    data class Group(val id: String, val name: String) : Page
}

/**
 * Who flies where on which day: the plans of every group the pilot is in,
 * day by day from today on, and from there a plan, the form for one, and
 * the groups. Opened from the map.
 *
 * One sheet with pages of its own, each with the way back in its corner: a
 * plan is seen by the members of its group and by nobody else, and the
 * server removes it two days after its day.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PlansSheet(
    groups: GroupsModel,
    account: AccountModel,
    /** Where requests run, so that one carries on when the sheet is closed. */
    requests: CoroutineScope,
    onDismiss: () -> Unit,
) {
    var path by remember { mutableStateOf(listOf<Page>(Page.Board)) }
    val page = path.last()
    val open: (Page) -> Unit = { next -> path = path + next }
    val back: () -> Unit = { if (path.size > 1) path = path.dropLast(1) }

    // Asked for when it is opened; and again by its button, and after
    // every change.
    LaunchedEffect(Unit) {
        groups.load()
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        BackHandler(enabled = path.size > 1, onBack = back)
        // As tall as the sheet can be, whatever the page holds.
        Box(Modifier.fillMaxHeight()) {
            Column {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = if (path.size > 1) 4.dp else 20.dp, end = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (path.size > 1) {
                        IconButton(onClick = back) {
                            Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = "Back")
                        }
                    }
                    Text(
                        when (page) {
                            Page.Board -> "Flying plans"
                            is Page.Plan -> "Plan"
                            is Page.Form -> if (page.editing == null) "New plan" else "Change the plan"
                            Page.Groups -> "Groups"
                            is Page.Group -> page.name
                        },
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    if (page == Page.Board && account.isSignedIn) {
                        // Pulling a sheet down closes it, so the board is
                        // asked for again with a button.
                        IconButton(onClick = { requests.launch { groups.load() } }, enabled = !groups.isLoading) {
                            Icon(painterResource(R.drawable.ic_refresh), contentDescription = "Ask again")
                        }
                        if (groups.groups.isNotEmpty()) {
                            IconButton(onClick = { open(Page.Form(editing = null)) }) {
                                Icon(painterResource(R.drawable.ic_add), contentDescription = "New plan")
                            }
                        }
                    }
                    TextButton(onClick = onDismiss) { Text("Done") }
                }
                when (page) {
                    Page.Board -> Board(groups, account, onPlan = { open(Page.Plan(it)) }, onGroups = { open(Page.Groups) })
                    is Page.Plan -> PlanPage(
                        page.id,
                        groups,
                        requests,
                        onEdit = { open(Page.Form(editing = page.id)) },
                        onGone = back,
                    )
                    is Page.Form -> PlanForm(page.editing?.let(groups::plan), groups, requests, onSaved = back)
                    Page.Groups -> GroupsPage(groups, requests, onGroup = { open(Page.Group(it.id, it.name)) })
                    is Page.Group -> GroupPage(
                        page.id,
                        groups,
                        requests,
                        onRenamed = { name -> path = path.dropLast(1) + Page.Group(page.id, name) },
                        onGone = back,
                    )
                }
            }
        }
    }
}

/** A sentence in place of what is not there. */
@Composable
internal fun Quiet(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = MaterialTheme.typography.bodyLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = modifier.padding(horizontal = 4.dp, vertical = 16.dp),
    )
}

/** What went wrong, where the pilot asked for it. */
@Composable
internal fun Problem(text: String?) {
    if (text != null) {
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.error,
            modifier = Modifier.testTag("plans-problem").padding(start = 4.dp, end = 4.dp, top = 12.dp),
        )
    }
}

@Composable
internal fun Waiting() {
    Box(Modifier.fillMaxWidth().padding(vertical = 32.dp), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(Modifier.size(28.dp))
    }
}

/** The plans day by day, and the way to the groups under them. */
@Composable
private fun Board(groups: GroupsModel, account: AccountModel, onPlan: (String) -> Unit, onGroups: () -> Unit) {
    Column(
        Modifier
            .testTag("plans")
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
    ) {
        val board = groups.board
        val problem = groups.problem
        when {
            !account.isSignedIn -> Quiet(PlanWords.SIGNED_OUT, Modifier.testTag("plans-signed-out"))
            board != null -> {
                if (board.groups.isEmpty()) {
                    Quiet(PlanWords.NO_GROUP)
                } else if (board.days.isEmpty()) {
                    Quiet(PlanWords.NO_PLANS, Modifier.testTag("plans-none"))
                }
                for (day in board.days) {
                    SectionHeader(PlanWords.title(day.day))
                    Group {
                        for (plan in day.plans) {
                            row { PlanRow(plan, onClick = { onPlan(plan.id) }) }
                        }
                    }
                }
                if (board.isCutShort) {
                    Quiet(PlanWords.CUT_SHORT)
                }
                Group(Modifier.padding(top = 24.dp)) {
                    row {
                        LinkRow(
                            if (board.groups.isEmpty()) "Make or join a group" else "Groups",
                            R.drawable.ic_chevron_right,
                            Modifier.testTag("plans-groups"),
                            leading = R.drawable.ic_group,
                            onClick = onGroups,
                        )
                    }
                }
                SectionFooter(PlanWords.WHO_SEES)
            }
            problem != null -> Quiet(problem)
            else -> Waiting()
        }
    }
}

/** One plan in a line: where, when, in which group, and who is in. */
@Composable
private fun PlanRow(plan: PlanView, onClick: () -> Unit) {
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("plan-row")
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 60.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            // One element for a screen reader: the place, then the rest.
            .clearAndSetSemantics {
                contentDescription = plan.place
                stateDescription = PlanWords.spoken(plan)
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(plan.place, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
            if (plan.inWords.isNotEmpty()) {
                Text(plan.inWords, style = MaterialTheme.typography.bodyMedium, color = quiet)
            }
            if (plan.groupName.isNotEmpty()) {
                Text(plan.groupName, style = MaterialTheme.typography.bodyMedium, color = quiet)
            }
        }
        Spacer(Modifier.width(8.dp))
        Column(horizontalAlignment = Alignment.End) {
            plan.meetAt?.let { time ->
                Text(time, style = MaterialTheme.typography.bodyLarge.numbers, fontWeight = FontWeight.SemiBold, maxLines = 1)
            }
            if (plan.amIn) {
                Icon(painterResource(R.drawable.ic_check), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
            }
        }
    }
}

/** A line of text in a group of rows. */
@Composable
internal fun TextRow(text: String, modifier: Modifier = Modifier, isQuiet: Boolean = false) {
    Text(
        text,
        style = MaterialTheme.typography.bodyLarge,
        color = if (isQuiet) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
        modifier = modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp),
    )
}

/** One plan: everything it says, who is in, and the pilot's own word on it. */
@Composable
private fun PlanPage(id: String, groups: GroupsModel, requests: CoroutineScope, onEdit: () -> Unit, onGone: () -> Unit) {
    val plan = groups.plan(id)
    var isWorking by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var isConfirmingDelete by remember { mutableStateOf(false) }

    // Does one thing on the server and says what went wrong, if anything.
    fun act(thenLeave: Boolean = false, work: suspend () -> Unit) {
        if (isWorking) return
        isWorking = true
        problem = null
        requests.launch {
            try {
                problem = groups.attempt(work)
                if (problem == null && thenLeave) {
                    onGone()
                }
            } finally {
                isWorking = false
            }
        }
    }

    Column(
        Modifier
            .testTag("plan")
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
    ) {
        if (plan == null) {
            Quiet(PlanWords.GONE)
        } else {
            Group(Modifier.padding(top = 16.dp)) {
                row { ValueRow("Where", plan.place) }
                row { ValueRow("Day", PlanWords.title(plan.day)) }
                plan.meetAt?.let { time -> row { ValueRow("Time", time) } }
                if (plan.groupName.isNotEmpty()) {
                    row { ValueRow("Group", plan.groupName) }
                }
                row { ValueRow("Posted by", if (plan.isMine) "You" else plan.author) }
            }
            plan.note?.let { note ->
                SectionHeader("Note")
                Group { row { TextRow(note) } }
            }
            SectionHeader("Who is in")
            Group {
                if (plan.pilotsIn.isEmpty()) {
                    row { TextRow("Nobody yet", isQuiet = true) }
                }
                for (name in plan.pilotsIn) {
                    row { TextRow(name, Modifier.testTag("plan-pilot")) }
                }
                row {
                    ActionRow(
                        if (plan.amIn) "I am out" else "I am in",
                        MaterialTheme.colorScheme.primary,
                        Modifier.testTag("plan-in"),
                        enabled = !isWorking,
                    ) {
                        act { groups.setIn(plan, !plan.amIn) }
                    }
                }
            }
            if (plan.isMine || plan.canDelete) {
                Group(Modifier.padding(top = 16.dp)) {
                    if (plan.isMine) {
                        row { ActionRow("Change the plan", MaterialTheme.colorScheme.primary, enabled = !isWorking, onClick = onEdit) }
                    }
                    if (plan.canDelete) {
                        row {
                            ActionRow("Delete the plan", MaterialTheme.colorScheme.error, enabled = !isWorking) {
                                isConfirmingDelete = true
                            }
                        }
                    }
                }
            }
            Problem(problem)
        }
    }

    if (isConfirmingDelete && plan != null) {
        AlertDialog(
            onDismissRequest = { isConfirmingDelete = false },
            title = { Text("Delete this plan?") },
            text = { Text("It goes for everyone in the group, with who was in.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        isConfirmingDelete = false
                        act(thenLeave = true) { groups.delete(plan) }
                    },
                ) {
                    Text("Delete the plan", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { isConfirmingDelete = false }) { Text("Cancel") } },
        )
    }
}

/** The form for a plan: a new one, or the pilot's own to change. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PlanForm(editing: PlanView?, groups: GroupsModel, requests: CoroutineScope, onSaved: () -> Unit) {
    // Starts from the plan being changed, or from today in the first group.
    var group by remember { mutableStateOf(editing?.groupId ?: groups.groups.firstOrNull()?.id.orEmpty()) }
    var day by remember { mutableStateOf(editing?.day?.let(PlanWords::date) ?: LocalDate.now()) }
    var place by remember { mutableStateOf(editing?.place.orEmpty()) }
    var time by remember { mutableStateOf(editing?.meetAt?.let(PlanWords::time)) }
    var note by remember { mutableStateOf(editing?.note.orEmpty()) }
    var isSaving by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var isPickingDay by remember { mutableStateOf(false) }
    var isPickingTime by remember { mutableStateOf(false) }

    Column(
        Modifier
            .testTag("plan-form")
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
    ) {
        if (editing == null && groups.groups.size > 1) {
            SectionHeader("Group")
            Group {
                for (choice in groups.groups) {
                    row {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .testTag("plan-group")
                                .selectable(
                                    selected = group == choice.id,
                                    enabled = !isSaving,
                                    role = Role.RadioButton,
                                    onClick = { group = choice.id },
                                )
                                .heightIn(min = 52.dp)
                                .padding(horizontal = 16.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(choice.name, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                            if (group == choice.id) {
                                Icon(
                                    painterResource(R.drawable.ic_check),
                                    contentDescription = null,
                                    tint = MaterialTheme.colorScheme.primary,
                                )
                            }
                        }
                    }
                }
            }
        }
        OutlinedTextField(
            value = place,
            onValueChange = { place = it },
            label = { Text("Where") },
            singleLine = true,
            enabled = !isSaving,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words),
            modifier = Modifier.fillMaxWidth().padding(top = 16.dp).testTag("plan-place"),
        )
        Group(Modifier.padding(top = 16.dp)) {
            row { ValueRow("Day", PlanWords.title(PlanWords.day(day)), onClick = { isPickingDay = true }) }
            row {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("plan-has-time")
                        .toggleable(
                            value = time != null,
                            enabled = !isSaving,
                            role = Role.Switch,
                            onValueChange = { wanted ->
                                time = if (wanted) LocalTime.now().let { it.hour to it.minute } else null
                            },
                        )
                        .heightIn(min = 52.dp)
                        .padding(horizontal = 16.dp),
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
        SectionHeader("Note")
        OutlinedTextField(
            value = note,
            onValueChange = { note = it },
            placeholder = { Text("Anything else") },
            enabled = !isSaving,
            minLines = 3,
            maxLines = 8,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            modifier = Modifier.fillMaxWidth().testTag("plan-note"),
        )
        SectionFooter(PlanWords.FORM_FOOT)
        Problem(problem)
        Button(
            onClick = {
                isSaving = true
                problem = null
                val draft = PlanDraft(
                    day = PlanWords.day(day),
                    place = place,
                    // A place on the map is not asked for yet: the name says where.
                    position = editing?.position,
                    meetAt = time?.let { (hour, minute) -> PlanWords.timeOfDay(hour, minute) },
                    note = note,
                )
                requests.launch {
                    try {
                        problem = groups.attempt {
                            if (editing != null) groups.change(editing, draft) else groups.post(draft, group)
                        }
                        if (problem == null) {
                            onSaved()
                        }
                    } finally {
                        isSaving = false
                    }
                }
            },
            enabled = !isSaving && place.isNotBlank(),
            modifier = Modifier.fillMaxWidth().padding(top = 20.dp).heightIn(min = 48.dp).testTag("plan-save"),
        ) {
            Text("Save")
        }
    }

    if (isPickingDay) {
        val today = remember { LocalDate.now() }
        val picked = rememberDatePickerState(
            initialSelectedDateMillis = day.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli(),
            // A plan is for today or a later day.
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
                        picked.selectedDateMillis?.let { millis ->
                            day = Instant.ofEpochMilli(millis).atZone(ZoneOffset.UTC).toLocalDate()
                        }
                        isPickingDay = false
                    },
                ) {
                    Text("OK")
                }
            },
            dismissButton = { TextButton(onClick = { isPickingDay = false }) { Text("Cancel") } },
        ) {
            DatePicker(picked)
        }
    }

    if (isPickingTime) {
        val picked = rememberTimePickerState(
            initialHour = time?.first ?: 9,
            initialMinute = time?.second ?: 0,
            is24Hour = Format.uses24HourClock,
        )
        AlertDialog(
            onDismissRequest = { isPickingTime = false },
            confirmButton = {
                TextButton(
                    onClick = {
                        time = picked.hour to picked.minute
                        isPickingTime = false
                    },
                ) {
                    Text("OK")
                }
            },
            dismissButton = { TextButton(onClick = { isPickingTime = false }) { Text("Cancel") } },
            text = { TimePicker(picked) },
        )
    }
}
