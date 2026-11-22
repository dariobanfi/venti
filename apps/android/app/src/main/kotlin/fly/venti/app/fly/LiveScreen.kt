package fly.venti.app.fly

import app.venti.design.VentiSpace
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.ButtonDefaults
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.BorderStroke
import app.venti.design.VentiSize
import fly.venti.app.design.Venti
import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.SharedFiles
import fly.venti.app.account.AccountModel
import fly.venti.app.design.ActionRow
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.ValueRow
import fly.venti.app.settings.Preferences
import fly.venti.core.LiveAudience
import fly.venti.core.LiveShareState
import fly.venti.core.LiveShareView
import fly.venti.app.design.Common

/**
 * Whom the pilot shows flights to while they are under way. Kept between
 * flights: the choice is in the cockpit's settings before every start.
 */
enum class LiveChoice(
    /** As the settings keep it, on the iPhone too. */
    val key: String,
    val title: String,
    /** For the row of the cockpit's settings. */
    val short: String,
    /** What the choice means, in a sentence. */
    val explanation: String,
) {
    Nobody("", "Nobody", "Off", "Nothing is sent. Nobody sees where you are."),
    Link(
        "link",
        "People I give the link",
        "With a link",
        "Whoever has the link sees where you are and where you flew, with your name: send it to the people " +
            "who should know. The flight is on nobody's map.",
    ),
    Everyone(
        "public",
        "Everyone",
        "Everyone",
        "Everyone who looks at Venti's map sees your glider on it, with your name, and can follow the flight.",
    ),
    ;

    /** What the core calls it. Nobody is no audience: nothing is sent. */
    val audience: LiveAudience?
        get() = when (this) {
            Nobody -> null
            Link -> LiveAudience.LINK
            Everyone -> LiveAudience.PUBLIC
        }

    /**
     * Who will watch the next flight, for the row of the cockpit's settings. A choice
     * that cannot be kept, because this phone is signed in nowhere, is not
     * shown as if it could.
     */
    fun summary(isSignedIn: Boolean): String = if (this == Nobody || isSignedIn) short else "Not signed in"

    companion object {
        /** The choice the pilot last made. Nobody, until one was made. */
        fun stored(preferences: Preferences): LiveChoice =
            entries.firstOrNull { it.key == preferences.liveAudience } ?: Nobody
    }
}

/** What the page says apart from the choices. */
object LiveWords {
    const val NEEDS_ACCOUNT = "Sharing a flight needs a Venti account. Sign in under Settings."

    const val WHAT_IS_SENT =
        "Your positions go from this phone to Venti from take-off on, every quarter of a minute, and " +
            "nowhere else. Venti removes the track two days after its last position. A track is not a flight " +
            "in your logbook: that is the recording, which stays on this phone until you sync."

    const val DELETION =
        "Nobody can watch this flight any more, with or without the link, and what was sent is removed from " +
            "Venti. Your recording is not touched."
}

/** Live tracking, as a sheet over the cockpit, from its settings and its flight's page: who may watch, and how this flight is being shared. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LiveSheet(fly: FlySession, account: AccountModel, preferences: Preferences, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Live tracking", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text(Common.words.done) }
        }
        Column(
            Modifier
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, bottom = 32.dp),
        ) {
            LiveTracking(fly, account, preferences)
        }
    }
}

/**
 * Who may watch, and how the flight in the air is being shared. The same
 * before take-off and in the cockpit.
 */
@Composable
private fun LiveTracking(fly: FlySession, account: AccountModel, preferences: Preferences) {
    val context = LocalContext.current
    var choice by remember { mutableStateOf(LiveChoice.stored(preferences)) }
    var isConfirmingDeletion by remember { mutableStateOf(false) }
    val isSignedIn = account.isSignedIn

    fun choose(chosen: LiveChoice) {
        // Taking a flight in the air away from everyone deletes its track:
        // asked first.
        if (chosen == LiveChoice.Nobody && fly.isSharingLive) {
            isConfirmingDeletion = true
        } else {
            preferences.liveAudience = chosen.key
            choice = chosen
            fly.share(chosen.audience)
        }
    }

    Column(Modifier.testTag("live-tracking")) {
        if (!isSignedIn) {
            Group(Modifier.padding(top = 16.dp)) {
                row {
                    Row(
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 14.dp).testTag("live-needs-account"),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            painterResource(R.drawable.ic_no_accounts),
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.width(16.dp))
                        Text(
                            LiveWords.NEEDS_ACCOUNT,
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }

        SectionHeader("Who can watch you fly")
        Group {
            for (each in LiveChoice.entries) {
                row { ChoiceRow(each, isChosen = each == choice, enabled = isSignedIn) { choose(each) } }
            }
        }

        fly.live?.let { live ->
            ThisFlight(
                live,
                onSend = { SharedFiles.shareText(context, live.link, "Send the link") },
                onDelete = { isConfirmingDeletion = true },
            )
        }

        SectionFooter(LiveWords.WHAT_IS_SENT, Modifier.padding(top = 16.dp))
    }

    if (isConfirmingDeletion) {
        AlertDialog(
            onDismissRequest = { isConfirmingDeletion = false },
            title = { Text("Delete the track?") },
            text = { Text(LiveWords.DELETION) },
            confirmButton = {
                TextButton(
                    onClick = {
                        isConfirmingDeletion = false
                        preferences.liveAudience = LiveChoice.Nobody.key
                        choice = LiveChoice.Nobody
                        fly.deleteLiveTrack()
                    },
                    modifier = Modifier.testTag("live-delete-confirm"),
                ) {
                    Text("Delete the track", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { isConfirmingDeletion = false }) { Text(Common.words.cancel) } },
        )
    }
}

/**
 * The cockpit's way to live tracking: a round button that says by its
 * colour whether the flight is going out. Green while positions go out,
 * amber while they wait, plain when nothing is shared.
 */
@Composable
fun LiveButton(live: LiveShareView?, modifier: Modifier = Modifier, onOpen: () -> Unit) {
    val state = live?.state
    // Green while positions go out, the caution colour while they wait,
    // plain when nothing is shared: sign, word and colour together.
    val (ground, ink) = when (state) {
        LiveShareState.SHARING -> Venti.colours.goodSoft to Venti.colours.good
        LiveShareState.WAITING -> Venti.colours.cautionSoft to Venti.colours.caution
        else -> Color.Transparent to MaterialTheme.colorScheme.onSurface
    }
    OutlinedButton(
        onClick = onOpen,
        modifier = modifier
            .heightIn(min = VentiSize.touchFlight)
            .testTag("live-button")
            .semantics { stateDescription = live?.words ?: "Off" },
        colors = ButtonDefaults.outlinedButtonColors(containerColor = ground, contentColor = ink),
        border = BorderStroke(1.5.dp, MaterialTheme.colorScheme.outline),
        contentPadding = PaddingValues(horizontal = VentiSpace.s3),
    ) {
        Icon(painterResource(LiveLook.icon(state)), contentDescription = null, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(VentiSpace.s2))
        Text("Live tracking", style = MaterialTheme.typography.labelLarge, maxLines = 1)
    }
}

/** How the cockpit's button shows where the sharing stands. */
object LiveLook {
    @DrawableRes
    fun icon(state: LiveShareState?): Int = when (state) {
        LiveShareState.SHARING, LiveShareState.ENDED -> R.drawable.venti_live
        LiveShareState.WAITING -> R.drawable.ic_sync_problem
        LiveShareState.STOPPED, null -> R.drawable.ic_location_off
    }
}

/** How the flight in the air is being shared, with the link to send and the way to take it back. */
@Composable
private fun ThisFlight(live: LiveShareView, onSend: () -> Unit, onDelete: () -> Unit) {
    SectionHeader("This flight")
    Group {
        // Read as one by a screen reader, and found as one by a test.
        row { ValueRow("Status", live.words, Modifier.testTag("live-status").semantics(mergeDescendants = true) {}) }
        if (live.state != LiveShareState.STOPPED) {
            row { ValueRow("Positions sent", live.taken.toString()) }
            row { ActionRow("Send the link", MaterialTheme.colorScheme.primary, Modifier.testTag("live-send"), onClick = onSend) }
            row {
                ActionRow("Delete the track", MaterialTheme.colorScheme.error, Modifier.testTag("live-delete"), onClick = onDelete)
            }
        }
    }
}

/** One answer to "who can watch you fly", with what it means; the chosen one is ticked. */
@Composable
private fun ChoiceRow(choice: LiveChoice, isChosen: Boolean, enabled: Boolean, onClick: () -> Unit) {
    // A choice that cannot be made is set in grey, as Material sets what is switched off.
    val faded = if (enabled) 1f else 0.38f
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("live-${choice.name.lowercase()}")
            .selectable(selected = isChosen, enabled = enabled, role = Role.RadioButton, onClick = onClick)
            .heightIn(min = 64.dp)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                choice.title,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = faded),
            )
            Text(
                choice.explanation,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = faded),
            )
        }
        Spacer(Modifier.width(12.dp))
        if (isChosen) {
            Icon(
                painterResource(R.drawable.ic_check),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary.copy(alpha = faded),
            )
        } else {
            Spacer(Modifier.width(24.dp))
        }
    }
}
