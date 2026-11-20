package fly.venti.app.fly

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import fly.venti.app.R
import fly.venti.app.design.ClimbBand
import fly.venti.app.design.Format
import fly.venti.app.design.Group
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.numbers
import fly.venti.app.wind.color
import fly.venti.core.WindAtHeightView
import fly.venti.core.windBand

/** What the list of winds by height says, in words. */
object WindByHeightWords {
    /** The band's heights: "1,600 to 1,800 m". */
    fun heights(band: WindAtHeightView): String = "${Format.metres(band.floorM)} to ${Format.metres(band.ceilingM)} m"

    /** When it was measured, and whether the glider is in the band now. */
    fun detail(band: WindAtHeightView): String {
        val age = age(band)
        return if (band.isHere) "You are here · measured $age" else "Measured $age"
    }

    /** The wind in numbers: "11 km/h W". */
    fun wind(band: WindAtHeightView): String =
        "${Format.kilometresPerHour(band.speedMs)} km/h ${Format.compassPoint(band.fromDeg)}"

    /** The band as a screen reader names it. */
    fun spokenHeights(band: WindAtHeightView): String =
        "${Format.metres(band.floorM)} to ${Format.metres(band.ceilingM)} metres" +
            if (band.isHere) ", where you are" else ""

    /**
     * How fast the glider climbed while it circled through the band:
     * "+2.4 m/s". Absent for a band it circled in only briefly.
     */
    fun climb(band: WindAtHeightView): String? = band.climbMs?.let { "${Format.climb(it)} m/s" }

    /** Its wind as a screen reader says it, and the climb if there is one. */
    fun spokenWind(band: WindAtHeightView): String =
        "${Format.spokenKilometresPerHour(band.speedMs)} from ${Format.compassPoint(band.fromDeg)}, measured ${age(band)}" +
            band.climbMs?.let { ", climbed ${Format.climb(it)} metres per second" }.orEmpty()

    private fun age(band: WindAtHeightView): String = Format.age((band.ageS / 60).toLong())

    const val NONE_YET = "No wind measured yet. It is worked out from your drift while you circle."

    const val EXPLANATION =
        "Measured from your drift while you circled, as the mean over each visit to a band of 200 m. A band " +
            "you have not circled in has no wind to show, and an old one may have changed. Under the wind " +
            "stands how fast you climbed through the band while circling."
}

/**
 * The wind in each band of height the glider has circled in, the highest
 * first. The wind in the valley and the wind above the ridges are two
 * winds, and a pilot deciding where to go next wants both.
 *
 * It is given the running session's bands, so the list fills while it is
 * open.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WindByHeightSheet(bands: List<WindAtHeightView>, onDismiss: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Wind by height", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss) { Text("Done") }
        }
        LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 32.dp)) {
            item {
                if (bands.isEmpty()) {
                    Text(
                        WindByHeightWords.NONE_YET,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(horizontal = 4.dp, vertical = 16.dp),
                    )
                } else {
                    Group {
                        for (band in bands) {
                            row { WindByHeightRow(band) }
                        }
                    }
                }
            }
            item { SectionFooter(WindByHeightWords.EXPLANATION) }
        }
    }
}

/** One band: where the air moves, the band's heights, and the wind in numbers. */
@Composable
private fun WindByHeightRow(band: WindAtHeightView) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("wind-by-height-row")
            .clearAndSetSemantics {
                contentDescription = WindByHeightWords.spokenHeights(band)
                stateDescription = WindByHeightWords.spokenWind(band)
            }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            painterResource(R.drawable.ic_arrow_upward),
            contentDescription = null,
            // The arrow shows where the wind goes, north at the top.
            modifier = Modifier.size(24.dp).rotate((band.fromDeg + 180).toFloat()),
            tint = windBand(band.speedMs).color,
        )
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(
                WindByHeightWords.heights(band),
                style = MaterialTheme.typography.bodyLarge.numbers,
                fontWeight = if (band.isHere) FontWeight.SemiBold else FontWeight.Normal,
            )
            Text(
                WindByHeightWords.detail(band),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.width(8.dp))
        Column(horizontalAlignment = Alignment.End) {
            Text(
                WindByHeightWords.wind(band),
                style = MaterialTheme.typography.titleMedium.numbers,
            )
            // How the thermal climbed through this band: where it is
            // strong, and where it weakens toward the top.
            WindByHeightWords.climb(band)?.let { climb ->
                Text(
                    climb,
                    style = MaterialTheme.typography.bodyMedium.numbers,
                    fontWeight = FontWeight.Medium,
                    color = ClimbBand.of(band.climbMs ?: 0.0).color,
                )
            }
        }
    }
}
