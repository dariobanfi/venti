package fly.venti.app.sites

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.core.graphics.createBitmap
import fly.venti.app.R
import fly.venti.app.design.Palette
import fly.venti.core.SiteKind
import fly.venti.core.SiteView
import org.maplibre.geojson.Feature
import org.maplibre.geojson.Point
import kotlin.math.roundToInt

/** The sign a site of this kind is drawn with, on the map and in lists. */
@get:DrawableRes
val SiteKind.icon: Int
    get() = when (this) {
        SiteKind.TAKEOFF -> R.drawable.ic_arrow_outward
        SiteKind.LANDING -> R.drawable.ic_south_east
        SiteKind.TOWING -> R.drawable.ic_vertical_align_top
        SiteKind.TRAINING -> R.drawable.ic_school
        SiteKind.OTHER -> R.drawable.ic_location_on
    }

/** The name the map knows the kind's picture by. */
val SiteKind.picture: String get() = "site-${name.lowercase()}"

/**
 * A landing is the light one: where the flight ends. Everything else is a
 * dark disc with a white sign.
 */
private val SiteKind.isLight: Boolean get() = this == SiteKind.LANDING

/**
 * The pictures the map draws flying sites with: a dark disc with a white
 * sign for a take-off, a white disc with a dark sign for a landing, so that
 * the two are told apart at a glance and neither is taken for wind,
 * airspace or a track, which have the colours.
 */
object SiteSymbols {
    /** The side of a picture, in the screen's own units. */
    private const val SIDE = 26f

    /** Every picture under the name the map knows it by, drawn for this screen. */
    fun all(context: Context): Map<String, Bitmap> = SiteKind.entries.associate { it.picture to picture(it, context) }

    private fun picture(kind: SiteKind, context: Context): Bitmap {
        val metrics = context.resources.displayMetrics
        val scale = metrics.density
        val side = (SIDE * scale).roundToInt()
        val bitmap = createBitmap(side, side)
        // The map reads from this how many pixels of the picture make one of its own units.
        bitmap.density = metrics.densityDpi
        val canvas = Canvas(bitmap)
        val ink = Palette.inkOnMap.toArgb()
        val white = android.graphics.Color.WHITE
        val (ground, sign) = if (kind.isLight) white to ink else ink to white
        val middle = side / 2f
        val radius = (SIDE - 4) / 2 * scale
        canvas.drawCircle(
            middle,
            middle,
            radius,
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.FILL
                color = ground
                setShadowLayer(1.5f * scale, 0f, 0.5f * scale, android.graphics.Color.argb(90, 0, 0, 0))
            },
        )
        // The ring is the sign's colour: it parts a dark disc from dark
        // forest and a white one from snow.
        canvas.drawCircle(
            middle,
            middle,
            radius - 0.75f * scale,
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.STROKE
                strokeWidth = 1.5f * scale
                color = sign
            },
        )
        val glyph = (GLYPH * scale / 2).roundToInt()
        ContextCompat.getDrawable(context, kind.icon)?.mutate()?.apply {
            setTint(sign)
            setBounds(side / 2 - glyph, side / 2 - glyph, side / 2 + glyph, side / 2 + glyph)
            draw(canvas)
        }
        return bitmap
    }

    /** The side of the sign inside the disc. */
    private const val GLYPH = 15f
}

/** One site as the map's symbol layer reads it: a place with what to draw there. */
object SiteFeature {
    const val ID = "id"
    const val PICTURE = "picture"
    const val LABEL = "label"

    /**
     * Only a name the source gives is written on the map: "Take-off" under
     * every take-off sign would say nothing.
     */
    fun of(site: SiteView): Feature =
        Feature.fromGeometry(Point.fromLngLat(site.position.lon, site.position.lat)).apply {
            addStringProperty(ID, site.id)
            addStringProperty(PICTURE, site.kind.picture)
            addStringProperty(LABEL, if (site.isNamed) site.name else "")
        }
}

/**
 * A site's sign as lists and sheets show it: the map's own, in both
 * appearances, because the list lies over the map and a pilot looks from
 * one to the other.
 */
@Composable
fun SiteGlyph(kind: SiteKind, modifier: Modifier = Modifier) {
    val ink = Palette.inkOnMap
    val (ground, sign) = if (kind.isLight) Palette.haloOnMap to ink else ink to Palette.haloOnMap
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
