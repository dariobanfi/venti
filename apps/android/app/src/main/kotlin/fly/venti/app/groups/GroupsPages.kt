package fly.venti.app.groups

import fly.venti.app.community.Reported
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.SharedFiles
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.core.GroupDetail
import fly.venti.core.GroupMember
import fly.venti.core.GroupView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/** A group whose invite code is to be shown. */
private data class MadeGroup(val name: String, val code: String)

/**
 * The pilot's groups, and the two ways into one: making it, or joining
 * with the code its keeper hands out.
 */
@Composable
internal fun GroupsPage(groups: GroupsModel, requests: CoroutineScope, onGroup: (GroupView) -> Unit) {
    var isMaking by remember { mutableStateOf(false) }
    var isJoining by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    // What became of the last thing done, in a line.
    var outcome by remember { mutableStateOf<String?>(null) }
    // A group just made, to show its invite code once.
    var made by remember { mutableStateOf<MadeGroup?>(null) }

    fun act(work: suspend () -> Unit) {
        problem = null
        outcome = null
        requests.launch { problem = groups.attempt(work) }
    }

    Column(
        Modifier
            .testTag("groups")
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
    ) {
        if (groups.groups.isNotEmpty()) {
            Group(Modifier.padding(top = 16.dp)) {
                for (group in groups.groups) {
                    row { GroupRow(group, onClick = { onGroup(group) }) }
                }
            }
        }
        Group(Modifier.padding(top = 16.dp)) {
            row {
                ActionRow("Make a group", MaterialTheme.colorScheme.primary, Modifier.testTag("group-make")) {
                    isMaking = true
                }
            }
            row {
                ActionRow("Join with a code", MaterialTheme.colorScheme.primary, Modifier.testTag("group-join")) {
                    isJoining = true
                }
            }
        }
        SectionFooter(GroupWords.WHAT_A_GROUP_IS)
        outcome?.let { words ->
            Group(Modifier.padding(top = 16.dp)) {
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
                        Text(words, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.testTag("group-outcome"))
                    }
                }
            }
        }
        Problem(problem)
    }

    if (isMaking) {
        TypedDialog(
            title = "Make a group",
            message = GroupWords.MAKING,
            label = "Its name",
            confirm = "Make",
            onDismiss = { isMaking = false },
        ) { name ->
            isMaking = false
            act {
                val group = groups.make(name)
                made = MadeGroup(group.group.name, group.inviteCode)
            }
        }
    }
    if (isJoining) {
        TypedDialog(
            title = "Join a group",
            message = GroupWords.JOINING,
            label = "Invite code",
            confirm = "Join",
            isPlain = true,
            onDismiss = { isJoining = false },
        ) { code ->
            isJoining = false
            act { outcome = groups.join(code).words }
        }
    }
    made?.let { group ->
        InviteCodeDialog(group, onDismiss = { made = null })
    }
}

/** One group in a line: its name, and what the core says of it. */
@Composable
private fun GroupRow(group: GroupView, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("group-row")
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 60.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .clearAndSetSemantics {
                contentDescription = group.name
                stateDescription = group.summary
            },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(group.name, style = MaterialTheme.typography.bodyLarge)
            Text(group.summary, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Icon(
            painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** A question with one line to type the answer into: a group's name, an invite code. */
@Composable
private fun TypedDialog(
    title: String,
    message: String?,
    label: String,
    confirm: String,
    onDismiss: () -> Unit,
    initial: String = "",
    /** Typed as it is, for a code: no capitals and no corrections. */
    isPlain: Boolean = false,
    onConfirm: (String) -> Unit,
) {
    var typed by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column {
                if (message != null) {
                    Text(message, modifier = Modifier.padding(bottom = 12.dp))
                }
                OutlinedTextField(
                    value = typed,
                    onValueChange = { typed = it },
                    label = { Text(label) },
                    singleLine = true,
                    keyboardOptions = if (isPlain) {
                        KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false)
                    } else {
                        KeyboardOptions(capitalization = KeyboardCapitalization.Words)
                    },
                    modifier = Modifier.fillMaxWidth().testTag("group-typed"),
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(typed) }, enabled = typed.isNotBlank()) { Text(confirm) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/**
 * A group's invite code, shown once: the server keeps no copy that can be
 * read.
 */
@Composable
private fun InviteCodeDialog(group: MadeGroup, onDismiss: () -> Unit) {
    val context = LocalContext.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Invite code") },
        text = {
            Column {
                Text(group.name, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
                SelectionContainer {
                    Text(
                        group.code,
                        style = MaterialTheme.typography.headlineSmall,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.testTag("invite-code").padding(vertical = 12.dp),
                    )
                }
                Text(GroupWords.THE_CODE, style = MaterialTheme.typography.bodyMedium)
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Done") } },
        dismissButton = {
            TextButton(onClick = { SharedFiles.shareText(context, group.code, "Hand the code on") }) {
                Text("Hand the code on")
            }
        },
    )
}

/** One group: its members, and what its keeper or a member can do. */
@Composable
internal fun GroupPage(
    id: String,
    groups: GroupsModel,
    requests: CoroutineScope,
    /** Called with the group's new name, for the page's title. */
    onRenamed: (String) -> Unit,
    /** Called when the pilot is no longer in the group, or it is gone. */
    onGone: () -> Unit,
    /** Opens the form to report the group. */
    onReport: (Reported) -> Unit,
) {
    var detail by remember(id) { mutableStateOf<GroupDetail?>(null) }
    var problem by remember { mutableStateOf<String?>(null) }
    var isWorking by remember { mutableStateOf(false) }
    var isRenaming by remember { mutableStateOf(false) }
    var code by remember { mutableStateOf<MadeGroup?>(null) }
    var asking by remember { mutableStateOf<GroupWords.Asking?>(null) }

    suspend fun load() {
        problem = groups.attempt { detail = groups.detail(id) }
    }
    LaunchedEffect(id) {
        load()
    }

    // Does one thing on the server, says what went wrong if anything did,
    // and reads the group again where the answer was not the group.
    fun act(thenLeave: Boolean = false, readAgain: Boolean = false, work: suspend () -> Unit) {
        if (isWorking) return
        isWorking = true
        problem = null
        requests.launch {
            try {
                val wrong = groups.attempt(work)
                if (wrong == null && thenLeave) {
                    onGone()
                } else {
                    if (readAgain || wrong != null) {
                        load()
                    }
                    if (wrong != null) {
                        problem = wrong
                    }
                }
            } finally {
                isWorking = false
            }
        }
    }

    val shown = detail
    Column(
        Modifier
            .testTag("group")
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
    ) {
        if (shown != null) {
            val isKeptByMe = shown.group.isKeptByMe
            SectionHeader("Members")
            Group {
                for (member in shown.members) {
                    row {
                        MemberRow(
                            member,
                            canAct = isKeptByMe && !member.isMe && !isWorking,
                            onRemove = { asking = GroupWords.Asking.Remove(member) },
                            onHandOver = { asking = GroupWords.Asking.HandOver(member) },
                        )
                    }
                }
            }
            if (isKeptByMe) {
                Group(Modifier.padding(top = 16.dp)) {
                    row {
                        ActionRow("New invite code", MaterialTheme.colorScheme.primary, enabled = !isWorking) {
                            asking = GroupWords.Asking.NewCode
                        }
                    }
                    row {
                        ActionRow("Rename the group", MaterialTheme.colorScheme.primary, enabled = !isWorking) {
                            isRenaming = true
                        }
                    }
                }
                SectionFooter(GroupWords.KEEPING)
            }
            Group(Modifier.padding(top = 16.dp)) {
                row {
                    ActionRow("Leave the group", MaterialTheme.colorScheme.error, enabled = !isWorking) {
                        asking = GroupWords.Asking.Leave
                    }
                }
                if (isKeptByMe) {
                    row {
                        ActionRow("Delete the group", MaterialTheme.colorScheme.error, enabled = !isWorking) {
                            asking = GroupWords.Asking.Delete
                        }
                    }
                }
                // A group that should not be there, to whoever runs the
                // server. Not for its keeper, who named it and can rename
                // or delete it.
                if (!isKeptByMe) {
                    row {
                        val report = Reported.group(id)
                        ActionRow(report.title, MaterialTheme.colorScheme.primary, Modifier.testTag("group-report"), enabled = !isWorking) {
                            onReport(report)
                        }
                    }
                }
            }
            SectionFooter(if (isKeptByMe) GroupWords.LEAVING_AS_KEEPER else GroupWords.LEAVING)
        } else if (problem == null) {
            Waiting()
        }
        Problem(problem)
    }

    // Everything here is asked about first, with what it means.
    asking?.let { asked ->
        AlertDialog(
            onDismissRequest = { asking = null },
            title = { Text(GroupWords.question(asked)) },
            text = { Text(GroupWords.consequence(asked)) },
            confirmButton = {
                TextButton(
                    onClick = {
                        asking = null
                        when (asked) {
                            GroupWords.Asking.NewCode -> act {
                                code = MadeGroup(shown?.group?.name.orEmpty(), groups.newInvite(id))
                            }
                            GroupWords.Asking.Leave -> act(thenLeave = true) { groups.leave(id) }
                            GroupWords.Asking.Delete -> act(thenLeave = true) { groups.deleteGroup(id) }
                            is GroupWords.Asking.Remove -> act(readAgain = true) { groups.remove(asked.member.handle, from = id) }
                            is GroupWords.Asking.HandOver -> act { detail = groups.handOver(id, asked.member.handle) }
                        }
                    },
                    modifier = Modifier.testTag("group-confirm"),
                ) {
                    Text(
                        GroupWords.confirm(asked),
                        color = if (GroupWords.isDestructive(asked)) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                    )
                }
            },
            dismissButton = { TextButton(onClick = { asking = null }) { Text("Cancel") } },
        )
    }
    if (isRenaming && shown != null) {
        TypedDialog(
            title = "Rename the group",
            message = null,
            label = "Its name",
            confirm = "Rename",
            initial = shown.group.name,
            onDismiss = { isRenaming = false },
        ) { name ->
            isRenaming = false
            act {
                val renamed = groups.rename(id, name)
                detail = renamed
                onRenamed(renamed.group.name)
            }
        }
    }
    code?.let { group ->
        InviteCodeDialog(group, onDismiss = { code = null })
    }
}

/** One member in a line, with what the keeper can do to them in a menu. */
@Composable
private fun MemberRow(member: GroupMember, canAct: Boolean, onRemove: () -> Unit, onHandOver: () -> Unit) {
    var isShowingMenu by remember { mutableStateOf(false) }
    val quiet = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier.fillMaxWidth().heightIn(min = 60.dp).padding(start = 16.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .weight(1f)
                .testTag("group-member")
                .padding(vertical = 10.dp)
                // One element for a screen reader: the name, then the rest.
                .clearAndSetSemantics {
                    contentDescription = GroupWords.name(member)
                    stateDescription = GroupWords.spoken(member)
                },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(GroupWords.name(member), style = MaterialTheme.typography.bodyLarge)
                Text("@${member.handle}", style = MaterialTheme.typography.bodyMedium, color = quiet)
            }
            if (member.isKeeper) {
                Text("Keeper", style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium, color = quiet)
            }
        }
        if (canAct) {
            Box {
                IconButton(onClick = { isShowingMenu = true }) {
                    Icon(painterResource(R.drawable.ic_more_vert), contentDescription = "More for ${member.name}")
                }
                DropdownMenu(expanded = isShowingMenu, onDismissRequest = { isShowingMenu = false }) {
                    DropdownMenuItem(
                        text = { Text("Make keeper") },
                        onClick = {
                            isShowingMenu = false
                            onHandOver()
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("Remove from the group", color = MaterialTheme.colorScheme.error) },
                        onClick = {
                            isShowingMenu = false
                            onRemove()
                        },
                    )
                }
            }
        } else {
            Spacer(Modifier.width(12.dp))
        }
    }
}
