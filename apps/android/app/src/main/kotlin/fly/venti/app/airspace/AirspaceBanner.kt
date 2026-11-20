package fly.venti.app.airspace

import fly.venti.app.design.Venti
import android.content.Context
import android.speech.tts.TextToSpeech
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.Palette
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.SectionHeader
import fly.venti.app.design.numbers
import fly.venti.app.fly.Announcer
import fly.venti.core.AirspaceCheck
import fly.venti.core.AirspaceLevel
import fly.venti.core.AirspaceOrigin
import fly.venti.core.AirspaceWarning
import java.util.Locale

/** How urgent a warning is, in words. */
val AirspaceLevel.words: String
    get() = when (this) {
        AirspaceLevel.NEAR -> "Airspace near"
        AirspaceLevel.VERY_NEAR -> "Airspace very near"
        AirspaceLevel.ENTERING -> "About to enter"
        AirspaceLevel.INSIDE -> "Inside airspace"
    }

/** What the app says about airspace warnings, wherever it shows them. */
object AirspaceWords {
    /** The warning the cockpit shows: the most urgent that has not been quieted, or else the most urgent. */
    fun shown(warnings: List<AirspaceWarning>): AirspaceWarning? =
        warnings.firstOrNull { !it.silenced } ?: warnings.firstOrNull()

    /**
     * What to say when the app is not checking, so that its silence is not
     * taken for "no airspace near".
     */
    fun notChecking(check: AirspaceCheck): String? = when (check) {
        AirspaceCheck.CHECKING, AirspaceCheck.NO_AIRSPACE -> null
        AirspaceCheck.WAITING_FOR_HEIGHT -> "Airspace is not being checked yet: no position with a height."
        AirspaceCheck.STALE -> "Airspace is not being checked: no position. What is shown is from the last one."
        AirspaceCheck.OUTSIDE_FILES -> "Your airspace does not cover this place. Nothing is checked here."
    }

    /** A warning as a screen reader says it. */
    fun spoken(warning: AirspaceWarning): String =
        listOfNotNull("${warning.headline}.", "${warning.detail}.", warning.uncertain).joinToString(" ")

    /**
     * Where a warning's airspace comes from and how old that is: a file of
     * the pilot's and the day it was brought in, or a provider by way of
     * the pilot's server and the day the provider wrote it.
     */
    fun origin(warning: AirspaceWarning): String {
        val date = warning.dataUnixS
        return when (warning.origin) {
            AirspaceOrigin.FILE ->
                if (date == null) {
                    "From your file “${warning.source}”."
                } else {
                    "From your file “${warning.source}”, brought in ${Format.day(date)}."
                }
            AirspaceOrigin.SERVER ->
                if (date == null) {
                    "From ${warning.source}, by way of your server."
                } else {
                    "From ${warning.source}, by way of your server, as of ${Format.day(date)}."
                }
        }
    }

    const val EXPLANATION =
        "A warning you have seen speaks again if it becomes more urgent. “Does not apply today” is for an " +
            "area you know to be inactive or have permission for: Venti cannot check that. Warnings are " +
            "advice from the airspace the app has; you remain responsible for where you fly."
}

/**
 * The most urgent airspace warning, over the cockpit's map. Red when the
 * glider is inside or about to be, amber when very near, plain when near
 * or quieted. A tap shows them all.
 */
@Composable
fun AirspaceBanner(
    warnings: List<AirspaceWarning>,
    /** Whether anything is being checked at all. */
    check: AirspaceCheck,
    onOpen: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        AirspaceWords.notChecking(check)?.let { notice ->
            Surface(
                modifier = Modifier.fillMaxWidth().testTag("airspace-notice"),
                shape = MaterialTheme.shapes.medium,
                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                shadowElevation = 2.dp,
            ) {
                Row(Modifier.padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(painterResource(R.drawable.ic_visibility_off), contentDescription = null, Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text(notice, style = MaterialTheme.typography.labelLarge)
                }
            }
        }
        Warning(warnings, onOpen)
    }
}

/** The band with the most urgent warning, if there is one. */
@Composable
private fun Warning(warnings: List<AirspaceWarning>, onOpen: () -> Unit) {
    val warning = AirspaceWords.shown(warnings) ?: return
    val (ground, ink) = colours(warning)
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("airspace-banner")
            .clickable(role = Role.Button, onClick = onOpen)
            .clearAndSetSemantics {
                contentDescription = if (warning.silenced) "Airspace warning, quieted" else "Airspace warning"
                stateDescription = AirspaceWords.spoken(warning)
                onClick(label = "Show all warnings") {
                    onOpen()
                    true
                }
            },
        shape = MaterialTheme.shapes.medium,
        color = ground,
        contentColor = ink,
        shadowElevation = 3.dp,
    ) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 10.dp)) {
            Icon(
                painterResource(if (warning.silenced) R.drawable.ic_notifications_off else R.drawable.venti_caution),
                contentDescription = null,
            )
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text(warning.headline, style = MaterialTheme.typography.titleMedium)
                Text(warning.detail, style = MaterialTheme.typography.bodySmall)
                warning.uncertain?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            }
            if (warnings.size > 1) {
                Spacer(Modifier.width(4.dp))
                Text("+${warnings.size - 1}", style = MaterialTheme.typography.titleSmall.numbers)
            }
        }
    }
}

/** The banner's ground and the colour of what is written on it. In the theme in use: the cockpit always has the flight theme. */
@Composable
private fun colours(warning: AirspaceWarning): Pair<Color, Color> {
    val plain = MaterialTheme.colorScheme.surfaceContainerHigh
    if (warning.silenced) return plain to MaterialTheme.colorScheme.onSurfaceVariant
    return when (warning.level) {
        AirspaceLevel.INSIDE, AirspaceLevel.ENTERING -> Venti.colours.dangerFill to Venti.colours.onDangerFill
        AirspaceLevel.VERY_NEAR -> Venti.colours.cautionSoft to Venti.colours.caution
        AirspaceLevel.NEAR -> plain to MaterialTheme.colorScheme.onSurface
    }
}

/**
 * Every airspace warning of the moment, with what the pilot can say to
 * each: seen, or does not apply today.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AirspaceWarningsSheet(
    warnings: List<AirspaceWarning>,
    /** What the providers of the airspace ask to be said wherever it is shown. */
    notices: List<String>,
    onAcknowledge: (String) -> Unit,
    onIgnoreToday: (String) -> Unit,
    onUnsilence: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Airspace", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text("Done") }
        }
        LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 32.dp)) {
            items(warnings, key = { it.id }) { warning ->
                SectionHeader(if (warning.silenced) "Quieted" else warning.level.words)
                Group {
                    row {
                        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
                            Text(warning.headline, style = MaterialTheme.typography.titleMedium)
                            Text(
                                warning.detail,
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            warning.uncertain?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                        }
                    }
                    if (warning.silenced) {
                        row { Answer("Warn me again", R.drawable.ic_notifications) { onUnsilence(warning.id) } }
                    } else {
                        row {
                            Answer("Seen: quiet for 10 minutes", R.drawable.ic_notifications_off) {
                                onAcknowledge(warning.id)
                                onDismiss()
                            }
                        }
                        row {
                            Answer("Does not apply today", R.drawable.ic_bedtime) {
                                onIgnoreToday(warning.id)
                                onDismiss()
                            }
                        }
                    }
                }
                SectionFooter(AirspaceWords.origin(warning))
            }
            if (warnings.isEmpty()) {
                item {
                    Text(
                        "No airspace near.",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(horizontal = 4.dp, vertical = 16.dp),
                    )
                }
            }
            item { SectionFooter(AirspaceWords.EXPLANATION, Modifier.padding(top = 16.dp)) }
            items(notices) { notice -> SectionFooter(notice, Modifier.testTag("airspace-provider-notice")) }
        }
    }
}

/** One thing the pilot can say to a warning. */
@Composable
private fun Answer(title: String, icon: Int, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .heightIn(min = 52.dp)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = MaterialTheme.colorScheme.primary)
        Spacer(Modifier.width(16.dp))
        Text(title, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.primary)
    }
}

/**
 * Says a warning aloud with the phone's own speech, in English like the
 * warnings themselves. A phone without a speech engine stays silent; the
 * banner is there either way.
 *
 * The engine is asked for with the first warning, not when the app starts:
 * most flights have none.
 */
class AirspaceVoice(context: Context) : Announcer {
    private val context = context.applicationContext
    private var speech: TextToSpeech? = null
    private var isReady = false

    /** What to say once the engine has answered. */
    private var waiting: String? = null

    @Synchronized
    override fun say(text: String) {
        val speech = speech
        if (speech == null) {
            waiting = text
            this.speech = TextToSpeech(context) { status -> engineAnswered(status) }
        } else if (isReady) {
            // A newer warning takes the place of one still being spoken.
            speech.speak(text, TextToSpeech.QUEUE_FLUSH, null, UTTERANCE)
        } else {
            waiting = text
        }
    }

    /** Said after what is being said: a warning that is not about airspace. */
    @Synchronized
    override fun sayAfter(text: String) {
        val speech = speech
        if (speech != null && isReady) {
            speech.speak(text, TextToSpeech.QUEUE_ADD, null, UTTERANCE)
        } else {
            waiting = listOfNotNull(waiting, text).joinToString(" ")
            if (speech == null) {
                this.speech = TextToSpeech(context) { status -> engineAnswered(status) }
            }
        }
    }

    @Synchronized
    private fun engineAnswered(status: Int) {
        val speech = speech ?: return
        if (status != TextToSpeech.SUCCESS) {
            waiting = null
            return
        }
        isReady = true
        runCatching { speech.language = Locale.UK }
        waiting?.let { speech.speak(it, TextToSpeech.QUEUE_FLUSH, null, UTTERANCE) }
        waiting = null
    }

    @Synchronized
    override fun stop() {
        waiting = null
        if (isReady) {
            speech?.stop()
        }
    }

    private companion object {
        const val UTTERANCE = "airspace"
    }
}
