package fly.venti.app.sites

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.core.graphics.createBitmap
import app.venti.design.VentiLight
import fly.venti.app.R
import fly.venti.app.design.Palette
import fly.venti.core.SiteKind
import fly.venti.core.SiteView
import org.maplibre.geojson.Feature
import org.maplibre.geojson.Point
import org.maplibre.geojson.Polygon
import kotlin.math.roundToInt

/** The sign a site of this kind is drawn with, on the map and in lists. */
@get:DrawableRes
val SiteKind.icon: Int
    get() = when (this) {
        SiteKind.TAKEOFF -> R.drawable.venti_takeoff
        SiteKind.LANDING -> R.drawable.venti_landing
        SiteKind.TOWING -> R.drawable.venti_takeoff
        SiteKind.TRAINING -> R.drawable.venti_site
        SiteKind.OTHER -> R.drawable.venti_site
    }

/** The name the map knows the kind's picture by. */
val SiteKind.picture: String get() = "site-${name.lowercase()}"

/** The picture of a site shown apart, of which its source does not say what a filter asks. */
val SiteKind.dimmedPicture: String get() = "$picture-unsaid"

/** A landing is the light one: where the flight ends. */
private val SiteKind.isLight: Boolean get() = this == SiteKind.LANDING

/**
 * The pictures the map draws flying sites with: the design language's pin,
 * its tip on the site. A dark pin with a cream dot is a place to take off
 * from; the same turned round, cream with a dark dot, a place to land, so
 * that the two are told apart at a glance and neither is taken for wind,
 * airspace or a track, which have the colours. A ring in place of the dot
 * is a training hill or a site of another kind. The map is light in both
 * appearances, so the pin takes the light theme's pair.
 */
object SiteSymbols {
    /** The picture's width and height, in the screen's own units: a disc for a head, the tip at the bottom edge. */
    private const val WIDTH = 28f
    private const val HEIGHT = 34f
    private const val RADIUS = 10f

    /**
     * Every picture under the name the map knows it by, drawn for this
     * screen: each kind's pin, and the same pin dimmed for a site shown
     * apart. The dimmed one is the pack's disabled manner, the raised
     * surface with the muted ink, so that it is still a pin of its kind
     * and plainly not one of those that fit.
     */
    fun all(context: Context): Map<String, Bitmap> = SiteKind.entries.associate { it.picture to picture(it, context) } +
        SiteKind.entries.associate { it.dimmedPicture to picture(it, context, dimmed = true) }

    private fun picture(kind: SiteKind, context: Context, dimmed: Boolean = false): Bitmap {
        val metrics = context.resources.displayMetrics
        val scale = metrics.density
        val bitmap = createBitmap((WIDTH * scale).roundToInt(), (HEIGHT * scale).roundToInt())
        // The map reads from this how many pixels of the picture make one of its own units.
        bitmap.density = metrics.densityDpi
        val canvas = Canvas(bitmap)
        val dark = VentiLight.primary.toArgb()
        val cream = VentiLight.onPrimary.toArgb()
        val (ground, sign) = when {
            dimmed -> MUTED_GROUND.toArgb() to MUTED_SIGN.toArgb()
            kind.isLight -> cream to dark
            else -> dark to cream
        }
        val centreX = WIDTH / 2 * scale
        val centreY = (RADIUS + 3) * scale
        val radius = RADIUS * scale
        // The head, and from where its sides leave the circle two straight
        // lines to the tip.
        val pin = Path().apply {
            arcTo(RectF(centreX - radius, centreY - radius, centreX + radius, centreY + radius), 150f, 240f)
            lineTo(centreX, centreY + 2 * radius)
            close()
        }
        canvas.drawPath(pin, Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL; color = ground })
        canvas.drawPath(
            pin,
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.STROKE
                strokeWidth = 2 * scale
                strokeJoin = Paint.Join.ROUND
                color = sign
            },
        )
        val dot = 4 * scale
        when (kind) {
            SiteKind.TAKEOFF, SiteKind.TOWING, SiteKind.LANDING ->
                canvas.drawCircle(centreX, centreY, dot, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = sign })
            SiteKind.TRAINING, SiteKind.OTHER ->
                canvas.drawCircle(
                    centreX,
                    centreY,
                    dot,
                    Paint(Paint.ANTI_ALIAS_FLAG).apply {
                        style = Paint.Style.STROKE
                        strokeWidth = 2 * scale
                        color = sign
                    },
                )
        }
        return bitmap
    }
}

/** The pack's disabled manner, in the light theme's values as everything on the map: the ground and the ink. */
internal val MUTED_GROUND = VentiLight.surfaceRaised
internal val MUTED_SIGN = VentiLight.textMuted

/** One site as the map's symbol layer reads it: a place with what to draw there. */
object SiteFeature {
    const val ID = "id"
    const val PICTURE = "picture"
    const val LABEL = "label"

    /** Whether the site is one shown apart: its sign and its name are dimmed. */
    const val DIMMED = "dimmed"

    /**
     * Only a name the source gives is written on the map: "Take-off" under
     * every take-off sign would say nothing. A place from pilots has the
     * core's label for its tier under its name: its sign is the one any
     * site of its kind has, and the label says what it is.
     */
    fun label(site: SiteView): String = listOfNotNull(
        site.name.takeIf { site.isNamed },
        site.byPilots?.takeIf { it.fromPilots }?.label?.takeIf { it.isNotEmpty() },
    ).joinToString("\n")

    fun of(site: SiteView, dimmed: Boolean = false): Feature =
        Feature.fromGeometry(Point.fromLngLat(site.position.lon, site.position.lat)).apply {
            addStringProperty(ID, site.id)
            addStringProperty(PICTURE, if (dimmed) site.kind.dimmedPicture else site.kind.picture)
            addStringProperty(LABEL, label(site))
            addBooleanProperty(DIMMED, dimmed)
        }

    /**
     * The signs of the sites a filter leaves: those shown apart first, so
     * that the ones that fit lie over them where two stand close.
     */
    fun all(fitting: List<SiteView>, unsaid: List<SiteView>): List<Feature> =
        unsaid.map { of(it, dimmed = true) } + fitting.map { of(it) }
}

/**
 * A site that its source draws as an area, a landing field mostly: its
 * outline under its sign, as the design language draws an area on a map,
 * a 2 px outline and a faint fill. The colour is the sign's own, the light
 * theme's primary, and the muted ink for a site shown apart.
 */
object SiteAreas {
    const val SOURCE = "flying-sites-areas"
    const val FILL = "flying-sites-areas"
    const val EDGE = "flying-sites-areas-edge"

    /**
     * The zoom from which the outlines are drawn. A field a hundred metres
     * across is as wide as its sign at about zoom 14 at this latitude;
     * from a zoom earlier it shows as the patch the sign stands on.
     */
    const val FROM_ZOOM = 13f

    /** The pack's outline of an area on a map, and the lower end of its fill (12 to 16 %). */
    const val EDGE_WIDTH = 2f
    const val FILL_OPACITY = 0.12f

    val colour: Int = VentiLight.primary.toArgb()
    val dimmedColour: Int = MUTED_SIGN.toArgb()

    /** The outlines of the sites that have one: those shown apart first, as the signs are. */
    fun all(fitting: List<SiteView>, unsaid: List<SiteView>): List<Feature> =
        unsaid.mapNotNull { of(it, dimmed = true) } + fitting.mapNotNull { of(it) }

    /** A site's outline as the map reads it, or `null` for a site that is a point. */
    fun of(site: SiteView, dimmed: Boolean = false): Feature? {
        val ring = site.outline.map { Point.fromLngLat(it.lon, it.lat) }
        // A ring is closed and has an area: three corners and the first again, at the least.
        if (ring.size < 4) return null
        val closed = if (ring.first() == ring.last()) ring else ring + ring.first()
        return Feature.fromGeometry(Polygon.fromLngLats(listOf(closed))).apply {
            addStringProperty(SiteFeature.ID, site.id)
            addBooleanProperty(SiteFeature.DIMMED, dimmed)
        }
    }
}

/**
 * A site's sign as lists and sheets show it: the map's own, in both
 * appearances, because the list lies over the map and a pilot looks from
 * one to the other.
 */
@Composable
fun SiteGlyph(
    kind: SiteKind,
    modifier: Modifier = Modifier,
    /** For a site shown apart from those that fit a filter: dimmed, as its sign on the map is. */
    dimmed: Boolean = false,
) {
    val ink = Palette.inkOnMap
    val (ground, sign) = when {
        dimmed -> MUTED_GROUND to MUTED_SIGN
        kind.isLight -> Palette.haloOnMap to ink
        else -> ink to Palette.haloOnMap
    }
    Box(
        modifier = modifier
            .size(28.dp)
            .background(ground, CircleShape)
            .border(1.5.dp, sign, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(kind.icon), contentDescription = null, modifier = Modifier.size(16.dp), tint = sign)
    }
}
