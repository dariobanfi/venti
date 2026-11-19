package fly.venti.app.wind

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import androidx.core.graphics.createBitmap
import androidx.compose.foundation.Canvas as ComposeCanvas
import androidx.compose.ui.graphics.Path as ComposePath

/**
 * The pictures the map draws stations with: a dart for each band of wind,
 * and a dot each for calm and for a reading that is too old.
 */
object WindSymbols {
    /** The side of a picture, in the screen's own units. */
    private const val SIDE = 30f

    /** The grey of a station with nothing current to say. */
    val old = Color(0xFFA9A8A2)

    /** Every picture under the name the map knows it by, drawn for a screen of this density. */
    fun all(density: Float, densityDpi: Int): Map<String, Bitmap> = buildMap {
        for (band in WindBand.entries) {
            val mark = WindMark(WindMark.Symbol.Arrow(band), heading = 0.0, label = "")
            put(mark.picture, picture(density, densityDpi) { canvas, scale -> arrow(canvas, scale, band.color) })
        }
        put(
            WindMark(WindMark.Symbol.Calm, 0.0, "").picture,
            picture(density, densityDpi) { canvas, scale -> dot(canvas, scale, WindBand.Light.color) },
        )
        put(
            WindMark(WindMark.Symbol.Old, 0.0, "").picture,
            picture(density, densityDpi) { canvas, scale -> dot(canvas, scale, old) },
        )
    }

    private fun picture(density: Float, densityDpi: Int, draw: (Canvas, Float) -> Unit): Bitmap {
        val pixels = (SIDE * density).toInt()
        val bitmap = createBitmap(pixels, pixels)
        // The map reads from this how many pixels of the picture make one of its own units.
        bitmap.density = densityDpi
        draw(Canvas(bitmap), density)
        return bitmap
    }

    /**
     * A dart pointing up, with a white edge so it stands out on any ground.
     * The map turns it to where the wind blows.
     */
    private fun arrow(canvas: Canvas, scale: Float, colour: Color) {
        val side = SIDE * scale
        val dart = Path().apply {
            moveTo(side * 0.5f, side * 0.13f)
            lineTo(side * 0.78f, side * 0.87f)
            lineTo(side * 0.5f, side * 0.69f)
            lineTo(side * 0.22f, side * 0.87f)
            close()
        }
        canvas.drawPath(dart, edge(scale))
        canvas.drawPath(dart, fill(colour))
    }

    /** A small disc with a white edge. */
    private fun dot(canvas: Canvas, scale: Float, colour: Color) {
        val middle = SIDE * scale / 2
        canvas.drawCircle(middle, middle, 4.5f * scale, edge(scale))
        canvas.drawCircle(middle, middle, 4.5f * scale, fill(colour))
    }

    private fun edge(scale: Float) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 3.5f * scale
        strokeJoin = Paint.Join.ROUND
        color = android.graphics.Color.WHITE
        setShadowLayer(1.5f * scale, 0f, 0.5f * scale, android.graphics.Color.argb(90, 0, 0, 0))
    }

    private fun fill(colour: Color) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.FILL
        color = colour.toArgb()
    }
}

/** The map's symbol for a station, drawn for a list or a heading. */
@Composable
fun WindGlyph(mark: WindMark, modifier: Modifier = Modifier) {
    val quiet = MaterialTheme.colorScheme.outline
    ComposeCanvas(modifier.size(28.dp).rotate(mark.heading.toFloat())) {
        when (val symbol = mark.symbol) {
            is WindMark.Symbol.Arrow -> {
                val side = size.minDimension
                val dart = ComposePath().apply {
                    moveTo(side * 0.5f, side * 0.13f)
                    lineTo(side * 0.78f, side * 0.87f)
                    lineTo(side * 0.5f, side * 0.69f)
                    lineTo(side * 0.22f, side * 0.87f)
                    close()
                }
                drawPath(dart, symbol.band.color)
            }
            WindMark.Symbol.Calm -> drawCircle(WindBand.Light.color, 4.5.dp.toPx(), Offset(size.width / 2, size.height / 2))
            WindMark.Symbol.Old -> drawCircle(quiet, 4.5.dp.toPx(), Offset(size.width / 2, size.height / 2))
        }
    }
}
