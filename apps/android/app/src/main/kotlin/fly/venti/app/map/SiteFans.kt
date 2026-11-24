package fly.venti.app.map

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.stateDescription
import androidx.core.graphics.createBitmap
import app.venti.design.VentiFlight
import app.venti.design.VentiLight
import app.venti.design.VentiSpace
import fly.venti.app.design.Palette
import fly.venti.app.design.Venti
import fly.venti.core.SiteKind
import fly.venti.core.SiteView
import fly.venti.core.WindSpan
import fly.venti.core.windFits
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.SymbolLayer
import org.maplibre.geojson.FeatureCollection
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * The directions a take-off works in, drawn on the map as a small fan
 * round the head of its pin, as Burnair draws them (plan item 19; the
 * owner, 2026-11-23; as on the iPhone and the web). The spans are the
 * source's (`SiteView.wind`); whether a wind fits them is the core's
 * (`windFits`). A fan points where the wind comes from: a take-off that
 * works in a north wind has its fan to the north.
 *
 * Its colour says whether the wind of the hour the map shows fits, where
 * the map shows an hour of the forecast's wind: the pack's good where it
 * fits, a hollow fan in the muted ink where it does not, so that the two
 * differ in more than their colour. Anywhere else every fan is the pin's
 * own dark. A landing has no fan: its pale pin is its sign. The cockpit's
 * map draws its take-offs with the same fans in one neutral colour of the
 * flight theme: there is no hour of the forecast in flight. The map does
 * not turn (`MapSetup`), so the fans need not either.
 */
object SiteFans {
    /** The feature's property that names its fan's picture; empty for none. */
    const val FAN = "fan"

    /** Whether the wind of the hour fits the take-off. */
    enum class Fit(val word: String) {
        Fits("fits"),
        DoesNotFit("off"),

        /** No hour of the forecast's wind on the map, or none at the place. */
        Unknown("plain"),

        /** On the cockpit's map: one neutral colour, whatever the wind. */
        InFlight("flight"),
    }

    /** The fan's picture: a circle as wide as a finger, in the screen's own units. */
    private const val SIZE = 52f

    /** How far the fan reaches from its middle. */
    private const val RADIUS = 24f

    /**
     * How far above the site's place the middle of the pin's head stands
     * (`SiteSymbols`: the pin is 34 high, the head's middle 13 from its top).
     */
    const val HEAD_ABOVE_TIP = 21f

    /**
     * Half of one of the sixteen points of the compass: a single direction
     * is drawn that wide on each side, and a span's ends too, so that "N to
     * NE" covers the two points whole.
     */
    private const val HALF_POINT = 11.25

    /** From this zoom on, as on the iPhone. */
    private const val FROM_ZOOM = 9f

    /** Whether a site gets a fan: one whose source gives the directions it works in, and that is not a landing. */
    fun hasFan(site: SiteView): Boolean = site.wind.isNotEmpty() && site.kind != SiteKind.LANDING

    /** The wedges of a fan, degrees clockwise from north, each `from` to `to` clockwise, `to` above `from` (it may pass 360). */
    fun wedges(spans: List<WindSpan>): List<Pair<Double, Double>> = spans.map { span ->
        val to = if (span.toDeg < span.fromDeg) span.toDeg + 360 else span.toDeg
        val from = span.fromDeg - HALF_POINT
        from to min(to + HALF_POINT, from + 360)
    }

    /** The name the map knows a fan's picture by: the same spans in the same state are one picture. */
    fun name(spans: List<WindSpan>, fit: Fit): String =
        "site-fan-${fit.word}-" + spans.joinToString("_") { "${it.fromDeg.roundToInt()}-${it.toDeg.roundToInt()}" }

    /**
     * Where the wind of an hour comes from, square by square: the arrows of
     * the core's `ThermalMapShapes`, whose `heading` is where the wind blows
     * to. A calm square has no direction and is left out.
     */
    fun windFrom(arrows: String): Map<UInt, Double> {
        val features = runCatching { FeatureCollection.fromJson(arrows).features() }.getOrNull().orEmpty()
        val from = HashMap<UInt, Double>()
        for (feature in features) {
            if (feature.getStringProperty("symbol") != "arrow") continue
            val cell = feature.getNumberProperty("cell")?.toLong() ?: continue
            val heading = feature.getNumberProperty("heading")?.toDouble() ?: continue
            from[cell.toUInt()] = (heading + 180) % 360
        }
        return from
    }

    /** Whether the wind fits a site, by the core: [Fit.Unknown] where there is no wind for the place. */
    fun fit(site: SiteView, windFrom: Double?): Fit {
        val fits = windFrom?.let { windFits(site.wind, it) } ?: return Fit.Unknown
        return if (fits) Fit.Fits else Fit.DoesNotFit
    }

    /** The fans' layer, under the sites' signs, from the sites' source. Add it before the signs, which stand on the fans. */
    fun layer(id: String, source: String): SymbolLayer =
        SymbolLayer(id, source).withProperties(
            PropertyFactory.iconImage(Expression.get(FAN)),
            PropertyFactory.iconAllowOverlap(true),
            PropertyFactory.iconIgnorePlacement(true),
            PropertyFactory.iconAnchor(Property.ICON_ANCHOR_CENTER),
            PropertyFactory.iconOffset(arrayOf(0f, -HEAD_ABOVE_TIP)),
        ).apply {
            setFilter(Expression.neq(Expression.get(FAN), Expression.literal("")))
            minZoom = FROM_ZOOM
        }

    /** The colours of a fan: its fill (none for a hollow one) and its edge, the light theme's as everything on the map. */
    private fun colours(fit: Fit): Pair<Color?, Color> = when (fit) {
        Fit.Fits -> VentiLight.good to Palette.haloOnMap
        Fit.DoesNotFit -> null to VentiLight.textMuted
        Fit.Unknown -> VentiLight.primary to Palette.haloOnMap
        Fit.InFlight -> VentiFlight.textMuted to Palette.haloOnMap
    }

    /** The picture of a fan, north up, drawn for this screen. */
    fun picture(spans: List<WindSpan>, fit: Fit, context: Context): Bitmap {
        val metrics = context.resources.displayMetrics
        val scale = metrics.density
        val bitmap = createBitmap((SIZE * scale).roundToInt(), (SIZE * scale).roundToInt())
        bitmap.density = metrics.densityDpi
        val canvas = Canvas(bitmap)
        val (fill, edge) = colours(fit)
        val centre = SIZE / 2 * scale
        val radius = RADIUS * scale
        val oval = RectF(centre - radius, centre - radius, centre + radius, centre + radius)
        for ((from, to) in wedges(spans)) {
            // Degrees clockwise from north to the canvas's, clockwise from the right.
            val path = Path().apply {
                moveTo(centre, centre)
                arcTo(oval, (from - 90).toFloat(), (to - from).toFloat())
                close()
            }
            canvas.drawPath(
                path,
                Paint(Paint.ANTI_ALIAS_FLAG).apply {
                    style = Paint.Style.FILL
                    color = (fill ?: Palette.haloOnMap).toArgb()
                },
            )
            canvas.drawPath(
                path,
                Paint(Paint.ANTI_ALIAS_FLAG).apply {
                    style = Paint.Style.STROKE
                    strokeWidth = (if (fill == null) 2f else 1.5f) * scale
                    strokeJoin = Paint.Join.ROUND
                    color = edge.toArgb()
                },
            )
        }
        return bitmap
    }
}

/** The words of the fans' key: the app's, as the iPhone's; the core has none. */
object FansWords {
    const val TAKEOFF = "Take-off:"
    const val FITS = "wind fits"
    const val DOES_NOT_FIT = "doesn't fit"
    const val SPOKEN = "Filled: wind fits; hollow: doesn't fit"
}

/**
 * The key of the take-offs' fans while the hour's wind colours them: the
 * fan in the good colour where the wind fits, hollow in the muted ink where
 * it does not, as the map draws them.
 */
@Composable
fun FansKey(modifier: Modifier = Modifier) {
    val style = MaterialTheme.typography.labelSmall
    val ink = Venti.colours.textSecondary
    // On one line where it fits, else the parts on lines of their own.
    FlowRow(
        modifier
            .testTag("what-fans-key")
            .clearAndSetSemantics {
                contentDescription = "Take-off"
                stateDescription = FansWords.SPOKEN
            },
        horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s1),
    ) {
        Text(FansWords.TAKEOFF, style = style, color = ink, maxLines = 1)
        for (fits in listOf(true, false)) {
            Row(horizontalArrangement = Arrangement.spacedBy(VentiSpace.s2), verticalAlignment = Alignment.CenterVertically) {
                FanSwatch(fits)
                Text(if (fits) FansWords.FITS else FansWords.DOES_NOT_FIT, style = style, color = ink, maxLines = 1)
            }
        }
    }
}

/** A quarter of a circle from its middle at the foot, pointing up, in the colours the map gives a fan. */
@Composable
private fun FanSwatch(fits: Boolean) {
    val fill = if (fits) VentiLight.good else Palette.haloOnMap
    val edge = if (fits) Palette.haloOnMap else VentiLight.textMuted
    Canvas(Modifier.size(width = VentiSpace.s4, height = VentiSpace.s3)) {
        val centre = Offset(size.width / 2, size.height)
        val radius = min(size.height, size.width / 1.4142f)
        val path = androidx.compose.ui.graphics.Path().apply {
            moveTo(centre.x, centre.y)
            arcTo(
                androidx.compose.ui.geometry.Rect(centre, radius),
                startAngleDegrees = -135f,
                sweepAngleDegrees = 90f,
                forceMoveTo = false,
            )
            close()
        }
        drawPath(path, fill, style = Fill)
        drawPath(path, edge, style = Stroke(width = (if (fits) 1f else 1.5f) * density))
    }
}
