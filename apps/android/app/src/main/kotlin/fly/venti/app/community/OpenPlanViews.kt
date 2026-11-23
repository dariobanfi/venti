package fly.venti.app.community

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import app.venti.design.VentiType
import fly.venti.app.design.OnChip
import fly.venti.app.R
import fly.venti.app.design.Venti
import fly.venti.app.design.text
import fly.venti.core.OpenPlanView
import kotlinx.coroutines.launch

/** What open plans' screens say that is not the core's to say. */
object OpenPlanWords {
    const val TITLE = "Open plans near"
    const val NEW = "New open plan"
    const val CHANGE = "Change the open plan"
    const val PLAN = "Open plan"
    const val I_AM_IN = "I'm in"
    const val I_AM_OUT = "I'm out"
    const val SIGN_IN_TO_JOIN = "Sign in to say you are in"
    const val CUT_SHORT = "There are more open plans near here: the later days are missing."
    const val WHERE = "Where"
    const val PLACE_HINT = "The name of the place"
    const val TAKEOFF_NEAR = "A take-off near"
    const val ON_THE_MAP = "On the map"
    const val MAP_MIDDLE = "Where the map is"
    const val MAP_MIDDLE_NOTE =
        "An open plan is found by its place. Take a take-off near, or move the map to the place on the Map tab and take where the map is."
    const val SEATS_FREE = "Seats free"
    const val COMPANY = "Looking for company"
    const val SEATS = "Seats"
    const val FEWER = "One seat fewer"
    const val MORE = "One seat more"
    const val NOTE = "Note"
    const val NOTE_HINT = "Anything else"
    const val POST = "Post"
    const val SAVE = "Save"
    const val POSTED_BY = "Posted by"
    const val OFFER = "Offer"
    const val WHO_IS_IN = "Who is in"
    const val NOBODY = "Nobody yet"
    const val FORECAST = "Forecast for this place"
    const val DELETE = "Delete the open plan"
    const val DELETE_QUESTION = "Delete this open plan?"
    const val DELETE_MEANS = "It goes for everyone, with who was in."
    const val YOU = "You"

    /** How many days the list holds, today the first. */
    const val DAYS = 14L

    /** How many take-offs near the form offers, the nearest first. */
    const val TAKEOFFS = 30

    /** The seats a pilot can offer, as the core takes them. */
    val SEAT_COUNTS = 1..8

    /** One plan's line for someone who cannot see it: the place is its name, this the rest. */
    fun spoken(plan: OpenPlanView): String = listOfNotNull(
        plan.meetAt?.let { "At $it." },
        "${plan.offerWords}.",
        "Posted by ${if (plan.isMine) YOU else plan.author}.",
        plan.inWords.takeIf { it.isNotEmpty() }?.let { "$it." },
        "You are in.".takeIf { plan.amIn },
    ).joinToString(" ")
}

/**
 * One open plan as a card of Community, in the manner of a group's plan
 * card: its day and time large at the top, the place, the author's face
 * and name with what they offer in the core's words ("2 seats free",
 * "Looking for company"), the note, the faces of who is in with the
 * core's sentence, and "I'm in", a switch that stays on while the pilot
 * is in. Signed out, "I'm in" leads to the account. The card opens the
 * plan's page.
 *
 * An open plan names its pilots by name: the faces of who is in are
 * initials, and the author's is the picture another list named for the
 * handle, if any.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun OpenPlanCard(
    plan: OpenPlanView,
    day: String,
    model: OpenPlansModel,
    myName: String?,
    myPicture: String?,
    onOpen: () -> Unit,
    onSignIn: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var isWorking by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    val colours = Venti.colours
    Surface(
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        border = BorderStroke(1.dp, colours.border),
        modifier = Modifier.fillMaxWidth().testTag("open-plan-card"),
    ) {
        Column(Modifier.padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s3, bottom = VentiSpace.s4)) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .clickable(role = Role.Button, onClick = onOpen)
                    .semantics(mergeDescendants = true) {
                        contentDescription = plan.place
                        stateDescription = listOf(day, OpenPlanWords.spoken(plan)).joinToString(". ")
                    },
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
            ) {
                // The day leads, as a calendar's page does; the time at its right.
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        day,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = colours.link,
                        modifier = Modifier.weight(1f),
                    )
                    plan.meetAt?.let { time -> Text(time, style = VentiType.instrumentM.text, color = colours.text, maxLines = 1) }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(painterResource(R.drawable.ic_location_on), contentDescription = null, tint = colours.textSecondary)
                    Spacer(Modifier.width(VentiSpace.s2))
                    Text(plan.place, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold, color = colours.text)
                }
                // Who offers what.
                Row(Modifier.padding(top = VentiSpace.s2), verticalAlignment = Alignment.CenterVertically) {
                    PilotFace(
                        if (plan.isMine && myName != null) myName else plan.author,
                        if (plan.isMine) myPicture else PilotPictures.of(plan.authorHandle),
                        size = FaceSize.Medium,
                    )
                    Spacer(Modifier.width(VentiSpace.s3))
                    Column(Modifier.weight(1f)) {
                        Text(
                            if (plan.isMine) OpenPlanWords.YOU else plan.author,
                            style = MaterialTheme.typography.bodyMedium,
                            color = colours.textSecondary,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(plan.offerWords, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold, color = colours.text)
                    }
                }
                plan.note?.takeIf { it.isNotBlank() }?.let { note ->
                    Text(
                        note,
                        style = MaterialTheme.typography.bodyMedium,
                        color = colours.textSecondary,
                        maxLines = 3,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag("open-plan-card-note"),
                    )
                }
            }
            Spacer(Modifier.size(VentiSpace.s3))
            FlowRow(
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                itemVerticalAlignment = Alignment.CenterVertically,
            ) {
                Faces(plan.pilotsIn, plan.amIn, myName, myPicture)
                if (plan.inWords.isNotEmpty()) {
                    Text(plan.inWords, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary, modifier = Modifier.weight(1f, fill = false))
                }
            }
            Spacer(Modifier.size(VentiSpace.s3))
            OnChip(
                selected = plan.amIn,
                onClick = {
                    // Saying so needs an account: the way to one.
                    if (!model.isSignedIn) return@OnChip onSignIn()
                    if (isWorking) return@OnChip
                    isWorking = true
                    problem = null
                    scope.launch {
                        try {
                            problem = model.attempt { model.setIn(plan.id, !plan.amIn) }
                        } finally {
                            isWorking = false
                        }
                    }
                },
                enabled = !isWorking,
                label = OpenPlanWords.I_AM_IN,
                modifier = Modifier.heightIn(min = VentiSize.touch).testTag("open-plan-card-in"),
            )
            problem?.let { said ->
                Text(said, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error, modifier = Modifier.testTag("open-plan-card-problem"))
            }
        }
    }
}
