package fly.venti.app.community

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.account.AccountModel
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.core.ReportForm
import fly.venti.core.ReportReason
import fly.venti.core.ReportReasonChoice
import fly.venti.core.ReportSubject
import fly.venti.core.reportFormFor
import fly.venti.core.siteWords
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import fly.venti.app.design.Common

/** Something a pilot reports to whoever runs the server. */
data class Reported(
    val subject: ReportSubject,
    /** The form's title, and the words of the entry that opens it. */
    val title: String,
) {
    companion object {
        fun flight(id: String) = Reported(ReportSubject.Flight(id), "Report this flight")

        fun pilot(handle: String) = Reported(ReportSubject.Pilot(handle), "Report this pilot")

        /** A pilot's picture, the one they show now, under the core's words for it. */
        fun picture(handle: String) = Reported(ReportSubject.Picture(handle), PilotPictures.words.report)

        fun liveTrack(id: String) = Reported(ReportSubject.LiveTrack(id), "Report this track")

        fun group(id: String) = Reported(ReportSubject.Group(id), "Report this group")

        fun plan(groupId: String, id: String) = Reported(ReportSubject.Plan(groupId, id), "Report this plan")

        /** A site of Venti's own layers, under the core's words for it. */
        fun site(id: String) = Reported(ReportSubject.Site(id), siteWords().report)

        /** One revision of such a site: what one pilot wrote into it. */
        fun siteRevision(id: String, revision: UInt) =
            Reported(ReportSubject.SiteRevision(id, revision), siteWords().reportRevision)
    }
}

/** What the form for a report says that is not the core's to say. */
object ReportWords {
    const val NO_ACCOUNT =
        "A report is sent from your account, so that Venti knows who it comes from. " +
            "Sign in under Settings, Account."

    const val OTHER_SERVER =
        "You are signed in on another server than the one this is on. " +
            "A report goes to the server it is about, from an account there."

    /**
     * Why no report can be sent from here, if none can. `mine` is the
     * server of the pilot's account and `shown` the one the thing that is
     * reported is on.
     */
    fun obstacle(isSignedIn: Boolean, mine: String?, shown: String?): String? = when {
        !isSignedIn || mine == null -> NO_ACCOUNT
        shown == null || !same(mine, shown) -> OTHER_SERVER
        else -> null
    }

    /** Whether two addresses name the same server. */
    fun same(one: String, other: String): Boolean {
        fun bare(address: String) = address.lowercase().trimEnd('/')
        return bare(one) == bare(other)
    }

    /** What stands over the field for the pilot's own words. */
    fun overTheText(reason: ReportReason?): String = if (reason == ReportReason.OTHER) "What is it?" else "Anything to add?"

    /** Said under the field once the text is longer than a report has room for. */
    fun tooLong(text: String, atMost: UInt): String? {
        val characters = text.codePointCount(0, text.length)
        return if (characters > atMost.toInt()) "$characters of $atMost characters" else null
    }
}

/**
 * The form for telling whoever runs the server that a flight, a pilot's
 * page or a live track should not be there.
 *
 * A report goes to the operator and nobody else, with the name of the
 * account it comes from: the form says so before anything is sent. The
 * reasons, the notice and the answer's words are the core's.
 */
@Composable
fun ReportSheet(
    reported: Reported,
    account: AccountModel,
    /** The address of the server the reported thing is on. */
    server: String?,
    /** Where the request runs, so that it carries on when the sheet is closed. */
    requests: CoroutineScope,
    onDismiss: () -> Unit,
) {
    ReportSheet(
        title = reported.title,
        // The reasons and the notice for this kind of thing: a group's and a plan's are their own.
        form = remember(reported.subject) { reportFormFor(reported.subject) },
        account = account,
        server = server,
        requests = requests,
        send = { reason, text -> account.report(reported.subject, reason, text) },
        onDismiss = onDismiss,
    )
}

/**
 * The same form for anything a report can be about: `form` is the core's
 * for that kind of thing, and `send` its request. A comment under a flight
 * is reported this way, with a form and a request of its own.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReportSheet(
    title: String,
    form: ReportForm,
    account: AccountModel,
    server: String?,
    requests: CoroutineScope,
    send: suspend (ReportReason, String) -> AccountModel.ReportOutcome,
    onDismiss: () -> Unit,
) {
    var reason by remember { mutableStateOf<ReportReason?>(null) }
    var text by remember { mutableStateOf("") }
    var isSending by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    // What became of the report, once it is sent.
    var outcome by remember { mutableStateOf<String?>(null) }
    val obstacle = ReportWords.obstacle(account.isSignedIn, account.account?.server, server)
    val sent = outcome

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss, modifier = Modifier.testTag("report-close")) {
                Text(if (sent == null && obstacle == null) Common.words.cancel else Common.words.done)
            }
        }
        Column(
            Modifier
                .testTag("report")
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
        ) {
            when {
                sent != null -> Group(Modifier.padding(top = 16.dp)) {
                    row {
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(
                                painterResource(R.drawable.ic_check),
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.primary,
                            )
                            Spacer(Modifier.width(12.dp))
                            Text(sent, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("report-outcome"))
                        }
                    }
                }
                // Nothing to fill in where nothing can be sent.
                obstacle != null -> Text(
                    obstacle,
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.testTag("report-obstacle").padding(horizontal = 4.dp, vertical = 16.dp),
                )
                else -> {
                    SectionHeader("What is wrong?")
                    Group {
                        for (choice in form.reasons) {
                            row {
                                ReasonRow(choice, isChosen = reason == choice.reason, enabled = !isSending) {
                                    reason = choice.reason
                                }
                            }
                        }
                    }
                    SectionHeader(ReportWords.overTheText(reason))
                    val tooLong = ReportWords.tooLong(text, form.textAtMost)
                    OutlinedTextField(
                        value = text,
                        onValueChange = { text = it },
                        placeholder = { Text("In your own words") },
                        enabled = !isSending,
                        isError = tooLong != null,
                        supportingText = tooLong?.let { words -> { Text(words) } },
                        minLines = 3,
                        maxLines = 8,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        modifier = Modifier.fillMaxWidth().testTag("report-text"),
                    )
                    // What happens to a report, read before it is sent.
                    SectionFooter(form.notice, Modifier.testTag("report-notice"))
                    problem?.let { words ->
                        Text(
                            words,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.testTag("report-problem").padding(start = 4.dp, end = 4.dp, top = 12.dp),
                        )
                    }
                    Button(
                        onClick = {
                            val chosen = reason ?: return@Button
                            isSending = true
                            problem = null
                            requests.launch {
                                try {
                                    val result = send(chosen, text)
                                    outcome = result.sent
                                    problem = result.problem
                                } finally {
                                    isSending = false
                                }
                            }
                        },
                        // Nothing can be sent before a reason is chosen.
                        enabled = reason != null && !isSending,
                        modifier = Modifier.fillMaxWidth().padding(top = 20.dp).heightIn(min = 48.dp).testTag("report-send"),
                    ) {
                        Text("Send")
                    }
                }
            }
        }
    }
}

/** One reason with what it means; the chosen one is ticked. */
@Composable
private fun ReasonRow(choice: ReportReasonChoice, isChosen: Boolean, enabled: Boolean, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("report-reason")
            .selectable(selected = isChosen, enabled = enabled, role = Role.RadioButton, onClick = onClick)
            .heightIn(min = 64.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(choice.title, style = MaterialTheme.typography.bodyLarge)
            Text(
                choice.explanation,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.width(12.dp))
        if (isChosen) {
            Icon(painterResource(R.drawable.ic_check), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
        } else {
            Spacer(Modifier.width(24.dp))
        }
    }
}
