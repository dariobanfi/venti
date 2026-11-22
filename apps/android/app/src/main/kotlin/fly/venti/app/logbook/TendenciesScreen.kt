package fly.venti.app.logbook

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.PillShape
import fly.venti.app.design.Venti
import fly.venti.core.Tendency
import fly.venti.core.Tendencies
import kotlinx.coroutines.CancellationException
import fly.venti.app.design.Common

/** What the screen "How you fly" has to show. */
sealed interface TendenciesState {
    /** The core is counting the flights. */
    data object Counting : TendenciesState

    /** What the tracks say; its observations may be none. */
    data class There(val tendencies: Tendencies) : TendenciesState

    /** The logbook could not be counted, and why. */
    data class Failed(val message: String) : TendenciesState
}

/**
 * The pilot's tendencies, asked of the core off the main thread: an older
 * logbook is counted flight by flight the first time.
 */
class TendenciesModel(private val count: suspend () -> Tendencies) {
    var state: TendenciesState by mutableStateOf(TendenciesState.Counting)
        private set

    suspend fun load() {
        state = try {
            TendenciesState.There(count())
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            TendenciesState.Failed(error.localizedMessage ?: error.toString())
        }
    }
}

/** The screen's words that are not the core's. */
object TendenciesWords {
    const val TITLE = "How you fly"
    const val COUNTING = "Counting your flights"
    const val NOTHING_YET = "Nothing to say yet. The observations begin once the logbook holds ten thermals."
    const val SHOWS_IT_MOST = "The flight that shows it most"

    /** A card as TalkBack reads it: the title, then the words, which say what a bar's share is of. */
    fun spoken(observation: Tendency): String = "${observation.title}. ${observation.words}"
}

/**
 * "How you fly": what the pilot's own tracks say about how they fly, as
 * cards of observations, each the core's words; no colour for good or bad.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TendenciesScreen(logbook: LogbookModel, onOpen: (String) -> Unit, onBack: () -> Unit) {
    val model = remember(logbook) { TendenciesModel(logbook::tendencies) }
    LaunchedEffect(logbook.view) { model.load() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(TendenciesWords.TITLE) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = Common.words.back)
                    }
                },
            )
        },
    ) { padding ->
        when (val state = model.state) {
            TendenciesState.Counting -> Row(
                modifier = Modifier.fillMaxWidth().padding(padding).padding(VentiSpace.s4).testTag("tendencies-counting"),
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircularProgressIndicator(Modifier.size(VentiSize.icon))
                Text(TendenciesWords.COUNTING, color = Venti.colours.textSecondary)
            }
            is TendenciesState.Failed -> Text(
                state.message,
                modifier = Modifier.padding(padding).padding(VentiSpace.s4),
            )
            is TendenciesState.There -> LazyColumn(
                modifier = Modifier.fillMaxSize().padding(padding).testTag("tendencies"),
                contentPadding = PaddingValues(start = VentiSpace.s4, end = VentiSpace.s4, bottom = VentiSpace.s5),
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
            ) {
                val observations = state.tendencies.observations
                if (observations.isEmpty()) {
                    item {
                        Text(
                            TendenciesWords.NOTHING_YET,
                            color = Venti.colours.textSecondary,
                            modifier = Modifier.padding(vertical = VentiSpace.s3).testTag("tendencies-nothing"),
                        )
                    }
                }
                items(observations) { observation -> ObservationCard(observation, onOpen) }
                if (state.tendencies.note.isNotEmpty()) {
                    item {
                        Text(
                            state.tendencies.note,
                            style = MaterialTheme.typography.bodySmall,
                            color = Venti.colours.textSecondary,
                            modifier = Modifier.padding(horizontal = VentiSpace.s1).testTag("tendencies-note"),
                        )
                    }
                }
            }
        }
    }
}

/**
 * One observation: its title, its words, a thin bar in the primary colour
 * where it is a share, and the flight that shows it most where the core
 * names one. TalkBack reads the card as one; the bar is decoration to it,
 * since the words say the share.
 */
@Composable
private fun ObservationCard(observation: Tendency, onOpen: (String) -> Unit) {
    val colours = Venti.colours
    Surface(
        modifier = Modifier.fillMaxWidth().testTag("tendency"),
        shape = MaterialTheme.shapes.medium,
        color = colours.surface,
        border = BorderStroke(1.dp, colours.border),
    ) {
        Column {
            Column(
                modifier = Modifier
                    .padding(VentiSpace.s4)
                    .clearAndSetSemantics { contentDescription = TendenciesWords.spoken(observation) },
                verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                Text(
                    observation.title,
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.semantics { heading() },
                )
                Text(observation.words, style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                observation.share?.let { share ->
                    // A thin bar: the share of the whole.
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .height(VentiSpace.s2)
                            .clip(PillShape)
                            .background(colours.surfaceRaised),
                    ) {
                        Box(
                            Modifier
                                .fillMaxWidth(share.coerceIn(0.0, 1.0).toFloat())
                                .height(VentiSpace.s2)
                                .background(colours.primary, PillShape),
                        )
                    }
                }
            }
            val flight = observation.flightId
            if (flight != null) {
                HorizontalDivider(color = colours.border)
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onOpen(flight) }
                        .heightIn(min = VentiSize.touch)
                        .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3)
                        .testTag("tendency-flight")
                        .semantics(mergeDescendants = true) {},
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(TendenciesWords.SHOWS_IT_MOST, style = MaterialTheme.typography.bodyLarge)
                        observation.flightUnixS?.let { day ->
                            Text(Format.day(day), style = MaterialTheme.typography.bodyMedium, color = colours.textSecondary)
                        }
                    }
                    Icon(
                        painterResource(R.drawable.ic_chevron_right),
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}
