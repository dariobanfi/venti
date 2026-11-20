package fly.venti.app.live

import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiLight
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import androidx.core.graphics.createBitmap
import fly.venti.core.LiveGlider
import org.maplibre.geojson.Feature
import org.maplibre.geojson.Point
import kotlin.math.roundToInt

/**
 * The pictures the map draws other pilots with: a paraglider seen from
 * above for one that flies, a disc for one on the ground.
 */
object LiveSymbols {
    const val FLYING = "live-flying"
    const val QUIET = "live-quiet"
    const val LANDED = "live-landed"

    /**
     * Green for a glider that is flying: the pack's "good", none of the
     * colours of wind, airspace or the pilot's own tracks. `0xRRGGBB`.
     */
    val GREEN: Int = VentiLight.good.toArgb() and 0xFFFFFF

    /** Grey for one that is on the ground, or that nothing was heard of for a while. */
    val GREY: Int = VentiLight.borderStrong.toArgb() and 0xFFFFFF

    /** The side of a picture, in the screen's own units. */
    private const val SIDE = 30f

    /** Which picture a glider is drawn with. */
    fun name(glider: LiveGlider): String = when {
        glider.landed -> LANDED
        glider.quiet -> QUIET
        else -> FLYING
    }

    /** Every picture under the name the map knows it by, drawn for this screen. */
    fun all(context: Context): Map<String, Bitmap> = mapOf(
        FLYING to drawn(context) { canvas, scale -> wing(canvas, GREEN, scale) },
        QUIET to drawn(context) { canvas, scale -> wing(canvas, GREY, scale) },
        LANDED to drawn(context) { canvas, scale -> disc(canvas, GREY, scale) },
    )

    /** A picture of [SIDE] by [SIDE] units, drawn in those units. */
    private fun drawn(context: Context, draw: (Canvas, Float) -> Unit): Bitmap {
        val metrics = context.resources.displayMetrics
        val side = (SIDE * metrics.density).roundToInt()
        val bitmap = createBitmap(side, side)
        // The map reads from this how many pixels of the picture make one of its own units.
        bitmap.density = metrics.densityDpi
        val canvas = Canvas(bitmap)
        canvas.scale(metrics.density, metrics.density)
        draw(canvas, metrics.density)
        return bitmap
    }

    /**
     * A paraglider seen from above: its wing as a crescent, the pilot as a
     * dot behind it. Drawn flying north; the map turns it.
     */
    private fun wing(canvas: Canvas, colour: Int, scale: Float) {
        val crescent = Path().apply {
            // The leading edge from tip over the middle to tip, then the
            // trailing edge back, flatter.
            arcTo(RectF(2f, 8f, 28f, 34f), 205f, 130f)
            arcTo(RectF(-9.56f, 12.5f, 39.56f, 61.62f), 298.7f, -57.4f)
            close()
        }
        val pilot = Path().apply { addCircle(15f, 20f, 2.6f, Path.Direction.CW) }
        // A white edge, with a shadow, keeps it apart from any ground.
        val edge = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeWidth = 3f
            strokeJoin = Paint.Join.ROUND
            color = android.graphics.Color.WHITE
            // The shadow is not scaled with the canvas: it is given in pixels.
            setShadowLayer(1.5f * scale, 0f, 0.5f * scale, android.graphics.Color.argb(90, 0, 0, 0))
        }
        val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.FILL
            color = opaque(colour)
        }
        canvas.drawPath(crescent, edge)
        canvas.drawPath(pilot, edge)
        canvas.drawPath(crescent, fill)
        canvas.drawPath(pilot, fill)
    }

    /** A glider on the ground: a small disc. */
    private fun disc(canvas: Canvas, colour: Int, scale: Float) {
        canvas.drawCircle(
            15f,
            15f,
            5.5f,
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.FILL
                color = opaque(colour)
                setShadowLayer(1.5f * scale, 0f, 0.5f * scale, android.graphics.Color.argb(90, 0, 0, 0))
            },
        )
        canvas.drawCircle(
            15f,
            15f,
            5.5f,
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.STROKE
                strokeWidth = 2f
                color = android.graphics.Color.WHITE
            },
        )
    }

    private fun opaque(rgb: Int): Int = (0xFF000000 or rgb.toLong()).toInt()
}

/** One glider as the map's symbol layer reads it: a place with what to draw there. */
object GliderFeature {
    const val ID = "id"
    const val PICTURE = "picture"
    const val HEADING = "heading"
    const val LABEL = "label"

    /** A glider on the ground points nowhere. */
    fun of(glider: LiveGlider): Feature =
        Feature.fromGeometry(Point.fromLngLat(glider.position.lon, glider.position.lat)).apply {
            addStringProperty(ID, glider.id)
            addStringProperty(PICTURE, LiveSymbols.name(glider))
            addNumberProperty(HEADING, if (glider.landed) 0.0 else glider.courseDeg ?: 0.0)
            addStringProperty(LABEL, glider.label)
        }
}
