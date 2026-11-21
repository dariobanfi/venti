package fly.venti.app.today

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Format
import fly.venti.app.design.Venti
import fly.venti.app.design.numbers
import fly.venti.core.OutlookHour
import fly.venti.core.OutlookNearby
import kotlin.math.floor

/**
 * The air around and above a take-off, as the screens word it: the wind
 * near by and the layers of an hour. Which wind is much stronger, where a
 * layer lies and every sentence about either are the core's; here are only
 * the names of the two figures and how an hour is read out.
 */
object AirAroundWords {
    const val WIND_NEAR_BY = "Wind near by"
    const val GUSTS_NEAR_BY = "Gusts near by"

    /** The hour on the place's clock as its detail is headed: "14:00". */
    fun clock(hour: OutlookHour): String = "${hour.hour}:00"

    /**
     * The strongest wind and the strongest gust near by with where they
     * are, as the core gives them and in the words of its own line: "28
     * km/h 20 km to the south over ground at 2400 m". The ground's height
     * stands with the figure because a square on a ridge has more wind
     * than a valley on any day. None for
     * an hour without a neighbourhood, and none for a figure the model's
     * squares do not have.
     */
    fun figures(nearby: OutlookNearby?): List<Pair<String, String>> {
        if (nearby == null) return emptyList()
        fun figure(kmh: String, where: String, groundM: Double?): String? {
            if (kmh.isEmpty()) return null
            val place = if (where.isEmpty()) "" else " $where"
            val ground = groundM?.let { " over ground at ${Format.metres(it)} m" }.orEmpty()
            return "$kmh km/h$place$ground"
        }
        return listOfNotNull(
            figure(nearby.windKmh, nearby.windWhere, nearby.windGroundM)?.let { WIND_NEAR_BY to it },
            figure(nearby.gustKmh, nearby.gustWhere, nearby.gustGroundM)?.let { GUSTS_NEAR_BY to it },
        )
    }

    /** Whether an hour has something to say beyond the picture: a line, a figure or a layer. */
    fun hasDetail(hour: OutlookHour): Boolean =
        hour.layers.isNotEmpty() || figures(hour.nearby).isNotEmpty() || hour.nearby?.line.orEmpty().isNotEmpty()

    /**
     * An hour of the picture read out: what is much stronger near by, then
     * its layers, then the two figures. Empty for an hour with none of them.
     */
    fun spoken(hour: OutlookHour): String =
        (
            listOfNotNull(hour.nearby?.line?.takeIf { it.isNotEmpty() }) +
                hour.layers.map { it.words } +
                figures(hour.nearby).map { (name, figure) -> "$name: $figure" }
            ).joinToString(". ")
}

/**
 * The wind near by when it is much stronger than at the take-off, as a
 * note in the caution manner: the pack's caution sign and colour beside
 * the core's line. It is no verdict, so it has no ground and no pill, and
 * stands under the day's own reason and never in the status's place.
 */
@Composable
fun NearbyLine(
    words: String,
    modifier: Modifier = Modifier,
    style: TextStyle = MaterialTheme.typography.labelLarge,
    tag: String = "nearby-line",
) {
    if (words.isEmpty()) return
    val colours = Venti.colours
    Row(
        modifier.testTag(tag),
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            painterResource(R.drawable.venti_caution),
            contentDescription = null,
            tint = colours.caution,
            modifier = Modifier.size(VentiSpace.s4),
        )
        Text(words, style = style, color = colours.caution)
    }
}

/**
 * One hour of the picture in words, under it: what is much stronger near
 * by, the hour's layers each beside the band that marks it in the picture,
 * and the strongest wind and gust near by as two quiet figures.
 */
@Composable
fun HourAir(hour: OutlookHour, modifier: Modifier = Modifier) {
    val colours = Venti.colours
    val small = MaterialTheme.typography.bodySmall
    Column(modifier.fillMaxWidth().testTag("air-hour"), verticalArrangement = Arrangement.spacedBy(VentiSpace.s1)) {
        Text(
            AirAroundWords.clock(hour),
            style = MaterialTheme.typography.labelLarge.numbers,
            fontWeight = FontWeight.SemiBold,
            color = colours.text,
        )
        NearbyLine(hour.nearby?.line.orEmpty(), style = small, tag = "air-hour-line")
        for (layer in hour.layers) {
            Row(
                Modifier.testTag("air-hour-layer").clearAndSetSemantics { contentDescription = layer.words },
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
                verticalAlignment = Alignment.Top,
            ) {
                // The band as the picture draws it, so that the words name their mark.
                Canvas(Modifier.padding(top = VentiSpace.s1).size(VentiSpace.s4, VentiSpace.s3)) {
                    layerBand(Rect(Offset.Zero, size), layer.inversion, colours.textMuted)
                }
                Text(layer.words, style = small, color = colours.textSecondary)
            }
        }
        for ((name, figure) in AirAroundWords.figures(hour.nearby)) {
            Row(
                Modifier.testTag("air-hour-figure").clearAndSetSemantics {
                    contentDescription = name
                    stateDescription = figure
                },
                horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
            ) {
                Text(name, style = small, color = colours.textSecondary)
                Text(figure, style = small.numbers, color = colours.textSecondary)
            }
        }
    }
}

/**
 * A layer's band in the picture of the day's air: filled, with an edge
 * above and below, for an inversion; hatched for a stable layer. The pack
 * has no token for a band, so both are drawn in `text-muted`, the stand-in
 * the design document proposes; the fill is thinned so that the arrows and
 * their speeds over it stay readable.
 */
internal fun DrawScope.layerBand(band: Rect, inversion: Boolean, ink: Color) {
    if (band.width <= 0f || band.height <= 0f) return
    val line = 1.dp.toPx()
    if (inversion) {
        drawRect(ink.copy(alpha = INVERSION_FILL), band.topLeft, band.size)
        drawLine(ink, band.topLeft, band.topRight, line)
        drawLine(ink, band.bottomLeft, band.bottomRight, line)
    } else {
        // Lines rising to the right, the same in every hour, so that the
        // bands of neighbouring hours read as one layer through the day.
        val step = HATCH_STEP.toPx()
        clipRect(band.left, band.top, band.right, band.bottom) {
            var x = floor((band.left - band.height) / step) * step
            while (x < band.right) {
                drawLine(ink.copy(alpha = HATCH_INK), Offset(x, band.bottom), Offset(x + band.height, band.top), line)
                x += step
            }
        }
    }
}

/** How much of `text-muted` an inversion's fill takes. */
private const val INVERSION_FILL = 0.4f

/** How much of it a stable layer's lines take: quieter than the arrows drawn over them. */
private const val HATCH_INK = 0.7f

/** How far apart the lines of a stable layer stand. */
private val HATCH_STEP = VentiSpace.s3
