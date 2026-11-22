package fly.venti.app.alerts

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.LinkRow
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.SegmentedPills
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.app.sites.Choice
import fly.venti.core.mapLayersWords
import fly.venti.core.AlertDraft
import fly.venti.core.AlertFaultView
import fly.venti.core.AlertKind
import fly.venti.core.AlertView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/** What the pages that offer "Alert me" need: the alarms, and the way to the account to sign in. */
class AlertsEntry(val model: AlertsModel, val onSignIn: () -> Unit)

/** The alarms, for a station's page and a take-off's page, wherever those are opened. */
val LocalAlerts = staticCompositionLocalOf<AlertsEntry?> { null }

/**
 * "Alert me", a row of a page's group: signed in it opens the form with the
 * draft the page makes; signed out it says in one line that alarms live
 * on the account, and leads to signing in.
 */
@Composable
fun AlertMeRow(entry: AlertsEntry, tag: String, draft: () -> AlertDraft) {
    var editing by remember { mutableStateOf<AlertDraft?>(null) }
    var asksToSignIn by remember { mutableStateOf(false) }
    val askToNotify = rememberNotificationQuestion()
    LinkRow(AlertWords.core.alertMe, R.drawable.ic_chevron_right, Modifier.testTag(tag), leading = R.drawable.ic_notifications) {
        if (entry.model.isSignedIn) editing = draft() else asksToSignIn = true
    }
    editing?.let { shown ->
        AlertFormDialog(entry.model, id = null, initial = shown, onSaved = askToNotify, onDone = { editing = null })
    }
    if (asksToSignIn) {
        AlertDialog(
            onDismissRequest = { asksToSignIn = false },
            title = { Text(AlertWords.core.title) },
            text = { Text(AlertWords.core.signedOut, Modifier.testTag("alert-sign-in-first")) },
            confirmButton = {
                TextButton(onClick = {
                    asksToSignIn = false
                    entry.onSignIn()
                }) { Text(AlertWords.SIGN_IN) }
            },
            dismissButton = { TextButton(onClick = { asksToSignIn = false }) { Text(Common.words.cancel) } },
        )
    }
}

/**
 * Asks for the permission to show notifications, from Android 13, once an
 * alarm is saved: what an alarm fires is shown as one. Asked only while it
 * is not given; Android itself stops asking after the pilot said no twice.
 */
@Composable
private fun rememberNotificationQuestion(): () -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    return {
        if (Build.VERSION.SDK_INT >= 33 && !AlertNotifications.areAllowed(context)) {
            launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}

/**
 * "Alerts", a page of the account: the alarms, each with its line and a
 * switch, a tap to change one, and under them what fired, the newest
 * first. Shown from the phone's copy at once, and fetched again when the
 * page opens.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AlertsScreen(model: AlertsModel, scope: CoroutineScope, onSignIn: () -> Unit, onBack: () -> Unit) {
    var editing by remember { mutableStateOf<AlertView?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    val askToNotify = rememberNotificationQuestion()
    LaunchedEffect(model) {
        model.read()
        scope.launch { model.fetch() }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(AlertWords.core.title) },
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
                .testTag("alerts-page")
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s5),
        ) {
            if (!model.isSignedIn) {
                Quiet(AlertWords.core.signedOut, "alerts-sign-in-first")
                Spacer(Modifier.height(VentiSpace.s3))
                Button(onClick = onSignIn, modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch)) {
                    Text(AlertWords.SIGN_IN)
                }
                return@Column
            }
            SectionHeader(AlertWords.core.listTitle)
            if (model.held.isEmpty()) {
                Quiet(AlertWords.core.listEmpty, "alerts-empty")
            } else {
                Group(Modifier.testTag("alerts")) {
                    for (alert in model.held) {
                        row {
                            AlertRow(
                                alert,
                                onEdit = { editing = alert },
                                onSwitch = { on -> scope.launch { problem = model.switch(alert, on) } },
                            )
                        }
                    }
                }
            }

            SectionHeader(AlertWords.core.firedTitle, Modifier.testTag("alerts-fired-title"))
            if (model.fired.isEmpty()) {
                Quiet(AlertWords.core.firedEmpty, "alerts-fired-empty")
            } else {
                Group(Modifier.testTag("alerts-fired")) {
                    for (fired in model.fired) {
                        row {
                            Column(
                                Modifier
                                    .fillMaxWidth()
                                    .testTag("alert-fired")
                                    .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3),
                            ) {
                                Text(fired.line, style = MaterialTheme.typography.bodyLarge.numbers)
                                Text(
                                    "${Format.dayInYear(fired.firedAtUnixS)}, ${Format.time(fired.firedAtUnixS)}",
                                    style = MaterialTheme.typography.bodyMedium.numbers,
                                    color = Venti.colours.textSecondary,
                                )
                            }
                        }
                    }
                }
            }
            SectionFooter(AlertWords.core.firedNote)
        }
    }

    editing?.let { alert ->
        val draft = alert.draft
        if (draft != null) {
            AlertFormDialog(model, id = alert.id, initial = draft, onSaved = askToNotify, onDone = { editing = null })
        }
    }
    problem?.let { words ->
        AlertDialog(
            onDismissRequest = { problem = null },
            title = { Text(AlertWords.core.title) },
            text = { Text(words) },
            confirmButton = { TextButton(onClick = { problem = null }) { Text(Common.words.ok) } },
        )
    }
}

/** A line of the page in the secondary text's colour: an empty list, or why there is none. */
@Composable
private fun Quiet(words: String, tag: String) {
    Text(
        words,
        style = MaterialTheme.typography.bodyLarge,
        color = Venti.colours.textSecondary,
        modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s2).testTag(tag),
    )
}

/** One alarm: its name and the core's line, a tap to change it, and its switch. */
@Composable
private fun AlertRow(alert: AlertView, onEdit: () -> Unit, onSwitch: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (alert.draft != null) Modifier.clickable(onClick = onEdit) else Modifier)
            .testTag("alert-row")
            .heightIn(min = VentiSize.touch)
            .padding(start = VentiSpace.s4, end = VentiSpace.s3, top = VentiSpace.s3, bottom = VentiSpace.s3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
            Text(alert.name, style = MaterialTheme.typography.bodyLarge)
            Text(
                alert.line,
                style = MaterialTheme.typography.bodyMedium.numbers,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.width(VentiSpace.s3))
        Switch(
            checked = alert.enabled,
            onCheckedChange = if (alert.draft != null) onSwitch else null,
            modifier = Modifier.semantics { contentDescription = AlertWords.switch(alert.name) },
        )
    }
}

/**
 * The form of an alarm, over the whole screen: a new one without `id`, a
 * change of one with it. The core's line says what the alarm waits for as
 * the choices change; once the pilot has tried to save, the core's faults
 * stand at their fields and follow every change.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AlertFormDialog(model: AlertsModel, id: String?, initial: AlertDraft, onSaved: () -> Unit, onDone: () -> Unit) {
    val scope = rememberCoroutineScope()
    var draft by remember(initial) { mutableStateOf(initial) }
    var hasTried by remember { mutableStateOf(false) }
    var isSending by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var isDeleting by remember { mutableStateOf(false) }
    val faults = if (hasTried) model.faults(draft) else emptyList()
    val finish = { saved: Boolean ->
        if (saved && id == null) onSaved()
        onDone()
    }

    Dialog(onDismissRequest = onDone, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Scaffold(
                topBar = {
                    TopAppBar(
                        title = { Text(if (id == null) AlertWords.core.alertMe else AlertWords.core.changeTitle) },
                        navigationIcon = {
                            IconButton(onClick = onDone) {
                                Icon(painterResource(R.drawable.ic_close), contentDescription = Common.words.cancel)
                            }
                        },
                    )
                },
            ) { padding ->
                Column(
                    Modifier
                        .fillMaxSize()
                        .padding(padding)
                        .consumeWindowInsets(padding)
                        .imePadding()
                        .verticalScroll(rememberScrollState())
                        .testTag("alert-form")
                        .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s5),
                ) {
                    // What the alarm waits for, in the core's line, as it changes.
                    Group {
                        row {
                            Text(
                                model.line(draft),
                                style = MaterialTheme.typography.titleMedium.numbers,
                                modifier = Modifier.padding(VentiSpace.s4).testTag("alert-line"),
                            )
                        }
                    }
                    Spacer(Modifier.height(VentiSpace.s4))
                    OutlinedTextField(
                        value = draft.name,
                        onValueChange = { draft = draft.copy(name = it) },
                        label = { Text(AlertWords.core.name) },
                        singleLine = true,
                        isError = faults.at("name") != null,
                        supportingText = faults.at("name")?.let { fault -> { Text(fault) } },
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        modifier = Modifier.fillMaxWidth().testTag("alert-name"),
                    )
                    // A new alarm for a place is one of two kinds: of the day
                    // the forecast makes there, or of the pilots in the air
                    // there. An alarm that stands keeps its kind.
                    if (id == null && draft.kind != AlertKind.WIND) {
                        Spacer(Modifier.height(VentiSpace.s4))
                        SegmentedPills(
                            listOf(mapLayersWords().alarmDayKind, AlertWords.core.pilotsKind),
                            if (draft.kind == AlertKind.PILOTS) 1 else 0,
                            Modifier.testTag("alert-kind"),
                            fitToWords = true,
                            onChoose = { index -> draft = AlertsModel.ofKind(draft, if (index == 1) AlertKind.PILOTS else AlertKind.DAY) },
                        )
                    }
                    when (draft.kind) {
                        AlertKind.WIND -> WindChoices(model, draft, faults) { draft = it }
                        AlertKind.DAY -> DayChoices(draft, faults) { draft = it }
                        AlertKind.PILOTS -> PilotsChoices(draft, faults) { draft = it }
                    }
                    // What the core finds wrong that no field above shows.
                    faults.filter { it.field !in SHOWN_AT_A_FIELD }.forEach { fault -> Fault(fault.message) }

                    Spacer(Modifier.height(VentiSpace.s5))
                    Button(
                        onClick = {
                            hasTried = true
                            if (model.faults(draft).isNotEmpty()) return@Button
                            isSending = true
                            scope.launch {
                                val refused = model.save(id, draft)
                                isSending = false
                                if (refused == null) finish(true) else problem = refused
                            }
                        },
                        enabled = !isSending,
                        modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("alert-save"),
                    ) {
                        Text(AlertWords.core.save)
                    }
                    if (id != null) {
                        Spacer(Modifier.height(VentiSpace.s2))
                        TextButton(
                            onClick = { isDeleting = true },
                            enabled = !isSending,
                            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("alert-delete"),
                        ) {
                            Text(AlertWords.core.delete, color = MaterialTheme.colorScheme.error)
                        }
                    }
                }
            }
        }

        if (isDeleting && id != null) {
            AlertDialog(
                onDismissRequest = { isDeleting = false },
                title = { Text(AlertWords.core.deleteQuestion) },
                text = { Text(initial.name) },
                confirmButton = {
                    TextButton(
                        onClick = {
                            isDeleting = false
                            isSending = true
                            scope.launch {
                                val refused = model.remove(id)
                                isSending = false
                                if (refused == null) finish(false) else problem = refused
                            }
                        },
                        modifier = Modifier.testTag("alert-delete-confirm"),
                    ) { Text(AlertWords.core.delete, color = MaterialTheme.colorScheme.error) }
                },
                dismissButton = { TextButton(onClick = { isDeleting = false }) { Text(Common.words.cancel) } },
            )
        }
        problem?.let { words ->
            AlertDialog(
                onDismissRequest = { problem = null },
                title = { Text(AlertWords.core.title) },
                text = { Text(words, Modifier.testTag("alert-problem")) },
                confirmButton = { TextButton(onClick = { problem = null }) { Text(Common.words.ok) } },
            )
        }
    }
}

/** The fields the form shows faults at; any other fault stands above "Save". */
private val SHOWN_AT_A_FIELD = setOf(
    "name", "rule.station", "rule.station_name", "rule.directions", "rule.min_kmh", "rule.max_kmh",
    "rule.gusts_max_kmh", "rule.hours", "rule.place", "rule.place_name", "rule.days_ahead",
    "rule.min_pilots", "rule.rest_hours",
)

/** The core's words for what is wrong at a field, all of them, or `null`. */
private fun List<AlertFaultView>.at(vararg fields: String): String? =
    filter { it.field in fields }.takeIf { it.isNotEmpty() }?.joinToString("\n") { it.message }

/** A fault in the core's words, in the colour of an error, under its field. */
@Composable
private fun Fault(words: String) {
    Text(
        words,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.error,
        modifier = Modifier.padding(horizontal = VentiSpace.s1, vertical = VentiSpace.s1).testTag("alert-fault"),
    )
}

/** The wind's choices: the directions as the sixteen points, the speeds and the hours. */
@Composable
private fun WindChoices(model: AlertsModel, draft: AlertDraft, faults: List<AlertFaultView>, onChange: (AlertDraft) -> Unit) {
    faults.at("rule.station", "rule.station_name")?.let { Fault(it) }
    SectionHeader(AlertWords.core.windFrom)
    PointsRow("alert-from") {
        Choice(AlertWords.core.any, isOn = draft.fromPoint == null, tag = "alert-from-any") {
            onChange(draft.copy(fromPoint = null, toPoint = null))
        }
        model.points.forEachIndexed { index, point ->
            val chosen = index.toUByte()
            Choice(point, isOn = draft.fromPoint == chosen, tag = "alert-from-$point") {
                onChange(draft.copy(fromPoint = chosen, toPoint = draft.toPoint ?: chosen))
            }
        }
    }
    SectionHeader(AlertWords.core.windTo)
    PointsRow("alert-to") {
        model.points.forEachIndexed { index, point ->
            val chosen = index.toUByte()
            Choice(point, isOn = draft.toPoint == chosen, tag = "alert-to-$point") {
                onChange(draft.copy(toPoint = chosen, fromPoint = draft.fromPoint ?: chosen))
            }
        }
    }
    faults.at("rule.directions")?.let { Fault(it) }

    Spacer(Modifier.height(VentiSpace.s4))
    Group {
        row {
            Stepper(AlertWords.core.least, draft.minKmh?.let { "${it} ${AlertWords.core.kmh}" }, faults.at("rule.min_kmh"), "alert-min",
                onLess = { onChange(draft.copy(minKmh = AlertSteps.less(draft.minKmh?.toInt(), SPEED_STEP, 0)?.toUShort())) },
                onMore = { onChange(draft.copy(minKmh = AlertSteps.more(draft.minKmh?.toInt(), SPEED_STEP, LEAST_START).toUShort())) },
            )
        }
        row {
            Stepper(AlertWords.core.most, draft.maxKmh?.let { "${it} ${AlertWords.core.kmh}" }, faults.at("rule.max_kmh"), "alert-max",
                onLess = { onChange(draft.copy(maxKmh = AlertSteps.less(draft.maxKmh?.toInt(), SPEED_STEP, 0)?.toUShort())) },
                onMore = { onChange(draft.copy(maxKmh = AlertSteps.more(draft.maxKmh?.toInt(), SPEED_STEP, MOST_START).toUShort())) },
            )
        }
        row {
            Stepper(AlertWords.core.gustsAtMost, draft.gustsMaxKmh?.let { "${it} ${AlertWords.core.kmh}" }, faults.at("rule.gusts_max_kmh"), "alert-gusts",
                onLess = { onChange(draft.copy(gustsMaxKmh = AlertSteps.less(draft.gustsMaxKmh?.toInt(), SPEED_STEP, 0)?.toUShort())) },
                onMore = { onChange(draft.copy(gustsMaxKmh = AlertSteps.more(draft.gustsMaxKmh?.toInt(), SPEED_STEP, GUSTS_START).toUShort())) },
            )
        }
    }
    SectionHeader(AlertWords.core.hoursTitle)
    Group {
        row {
            Stepper(AlertWords.core.fromHour, draft.fromHour?.let { AlertWords.hour(it.toInt()) }, null, "alert-from-hour",
                onLess = { onChange(draft.copy(fromHour = AlertSteps.less(draft.fromHour?.toInt(), 1, 0)?.toUByte())) },
                onMore = { onChange(draft.copy(fromHour = AlertSteps.more(draft.fromHour?.toInt(), 1, FROM_HOUR_START).toUByte())) },
            )
        }
        row {
            Stepper(AlertWords.core.toHour, draft.toHour?.let { AlertWords.hour(it.toInt()) }, null, "alert-to-hour",
                onLess = { onChange(draft.copy(toHour = AlertSteps.less(draft.toHour?.toInt(), 1, 1)?.toUByte())) },
                onMore = { onChange(draft.copy(toHour = AlertSteps.more(draft.toHour?.toInt(), 1, TO_HOUR_START).toUByte())) },
            )
        }
    }
    faults.at("rule.hours")?.let { Fault(it) }
}

/** The day's choices: which days ahead, and the least kind of day. */
@Composable
private fun DayChoices(draft: AlertDraft, faults: List<AlertFaultView>, onChange: (AlertDraft) -> Unit) {
    faults.at("rule.place", "rule.place_name")?.let { Fault(it) }
    SectionHeader(AlertWords.core.days)
    PointsRow("alert-days") {
        AlertWords.core.dayNames.forEachIndexed { index, name ->
            val day = index.toByte()
            val isOn = day in draft.daysAhead
            Choice(name, isOn = isOn, tag = "alert-day-$index", ticks = true) {
                val days = draft.daysAhead.toList()
                onChange(draft.copy(daysAhead = (if (isOn) days - day else days + day).sorted().toByteArray()))
            }
        }
    }
    faults.at("rule.days_ahead")?.let { Fault(it) }
    SectionHeader(AlertWords.core.atLeast)
    SegmentedPills(
        AlertWords.core.dayKinds.map { it.name },
        AlertWords.core.dayKinds.indexOfFirst { it.kind == draft.atLeast }.coerceAtLeast(0),
        Modifier.testTag("alert-at-least"),
        fitToWords = true,
        onChoose = { index -> onChange(draft.copy(atLeast = AlertWords.core.dayKinds[index].kind)) },
    )
}

/**
 * The choices of an alarm for pilots in the air at the place: how many at
 * least, and for how many hours it then stays quiet, with the core's note
 * on who is counted.
 */
@Composable
private fun PilotsChoices(draft: AlertDraft, faults: List<AlertFaultView>, onChange: (AlertDraft) -> Unit) {
    faults.at("rule.place", "rule.place_name")?.let { Fault(it) }
    val numbers = AlertsModel.NUMBERS
    val pilots = (draft.minPilots ?: numbers.minPilotsDefault).toInt()
    val rest = (draft.restHours ?: numbers.restHoursDefault).toInt()
    Spacer(Modifier.height(VentiSpace.s4))
    Group {
        row {
            Stepper(AlertWords.core.pilotsAtLeast, pilots.toString(), faults.at("rule.min_pilots"), "alert-pilots",
                onLess = { onChange(draft.copy(minPilots = (pilots - 1).coerceAtLeast(numbers.minPilotsLeast.toInt()).toUShort())) },
                onMore = { onChange(draft.copy(minPilots = (pilots + 1).coerceAtMost(numbers.minPilotsMost.toInt()).toUShort())) },
            )
        }
        row {
            Stepper(AlertWords.core.pilotsRest, rest.toString(), faults.at("rule.rest_hours"), "alert-rest",
                onLess = { onChange(draft.copy(restHours = (rest - 1).coerceAtLeast(numbers.restHoursLeast.toInt()).toUByte())) },
                onMore = { onChange(draft.copy(restHours = (rest + 1).coerceAtMost(numbers.restHoursMost.toInt()).toUByte())) },
            )
        }
    }
    SectionFooter(AlertWords.core.pilotsNote, Modifier.testTag("alert-pilots-note"))
}

/** A row of choices that scrolls sideways, as the sites' filter shows the winds. */
@Composable
private fun PointsRow(tag: String, content: @Composable () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .selectableGroup()
            .testTag(tag),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        content()
    }
}

/** A value with less and more: "Any" while none is chosen; the core's fault for it under it. */
@Composable
private fun Stepper(title: String, value: String?, fault: String?, tag: String, onLess: () -> Unit, onMore: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag(tag)
            .heightIn(min = VentiSize.touch)
            .padding(start = VentiSpace.s4, end = VentiSpace.s1, top = VentiSpace.s1, bottom = VentiSpace.s1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            if (fault != null) {
                Text(
                    fault,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.testTag("alert-fault"),
                )
            }
        }
        IconButton(onClick = onLess, enabled = value != null, modifier = Modifier.testTag("$tag-less")) {
            Icon(painterResource(R.drawable.ic_remove), contentDescription = "Less $title")
        }
        Text(
            value ?: AlertWords.core.any,
            style = MaterialTheme.typography.bodyLarge.numbers,
            color = if (value == null) Venti.colours.textSecondary else MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.testTag("$tag-value"),
        )
        IconButton(onClick = onMore, modifier = Modifier.testTag("$tag-more")) {
            Icon(painterResource(R.drawable.ic_add), contentDescription = "More $title")
        }
    }
}

/** Where a value that was "Any" starts when the pilot first asks for more. */
private const val LEAST_START = 10
private const val MOST_START = 20
private const val GUSTS_START = 30
private const val FROM_HOUR_START = 10
private const val TO_HOUR_START = 16
private const val SPEED_STEP = 5

/** How a stepper of the form steps: none is "Any". */
object AlertSteps {
    /** One step more; from "Any", `start`. */
    fun more(value: Int?, step: Int, start: Int): Int = value?.plus(step) ?: start

    /** One step less; below `least` it is "Any" again. */
    fun less(value: Int?, step: Int, least: Int): Int? = value?.minus(step)?.takeIf { it >= least }
}
