package fly.venti.app.radar

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import fly.venti.app.forecast.ThermalHoursLayers
import fly.venti.app.forecast.ThermalMapLayer
import fly.venti.core.RadarKey
import org.maplibre.android.geometry.LatLng
import org.maplibre.android.geometry.LatLngQuad
import org.maplibre.android.maps.Style
import org.maplibre.android.style.expressions.Expression
import org.maplibre.android.style.layers.Property
import org.maplibre.android.style.layers.PropertyFactory
import org.maplibre.android.style.layers.RasterLayer
import org.maplibre.android.style.sources.ImageSource

/**
 * How the rain radar is drawn: one image source, a frame's picture pinned
 * to the map by the layer's four corners, and one raster layer over it,
 * drawn without smoothing so that a step of rain stays a step. The picture
 * is the core's, in the colours its key names from the light theme's
 * tokens (the map is light in both appearances) and the forecast map's
 * veil for what the radar did not see; the layer is as see-through as the
 * forecast map's squares and lies where they lie: over the ground and the
 * photographs, under the base map's names, borders and cable cars, and
 * under everything else this map draws: airspace, tracks, sites, wind and
 * pilots.
 */
object RadarLayer {
    const val SOURCE = "rain-radar"
    const val LAYER = "rain-radar"

    /** The colour of a line of the key, from the light theme: the scale and the step the core names. */
    fun colour(key: RadarKey): Color = ThermalHoursLayers.colour(key.scale, key.scaleStep.toInt())

    /** The colour of what the radar did not see: the forecast map's quiet veil. */
    val UNSEEN: Color = ThermalMapLayer.VEIL

    /** A colour as the core takes it: red, green, blue and how solid, `0xRRGGBBAA`. */
    fun rgba(colour: Color): UInt {
        val argb = colour.toArgb().toUInt()
        return (argb shl 8) or (argb shr 24)
    }

    /** The colours a picture is written in: solid, since the layer as a whole is see-through. */
    val colours = RadarColours(rain = { key -> rgba(colour(key)) }, unseen = rgba(UNSEEN))

    /** The four corners a picture is pinned by: top left, top right, bottom right, bottom left. */
    fun corners(picture: RadarPicture): LatLngQuad = LatLngQuad(
        LatLng(picture.north, picture.west),
        LatLng(picture.north, picture.east),
        LatLng(picture.south, picture.east),
        LatLng(picture.south, picture.west),
    )

    /** A picture's bytes as Android draws them: every pixel as it is, not scaled to the screen. */
    fun bitmap(png: ByteArray): Bitmap? =
        BitmapFactory.decodeByteArray(
            png,
            0,
            png.size,
            BitmapFactory.Options().apply {
                inScaled = false
                inPreferredConfig = Bitmap.Config.ARGB_8888
            },
        )

    /**
     * Draws a picture in place of the one before, or takes the layer away.
     * The source and its layer are added with the first picture, over the
     * forecast map's squares; after that a change of frame only swaps the
     * source's picture.
     */
    fun show(picture: RadarPicture?, style: Style) {
        val layer = style.getLayerAs<RasterLayer>(LAYER)
        if (picture == null) {
            layer?.setProperties(PropertyFactory.visibility(Property.NONE))
            return
        }
        val bitmap = bitmap(picture.png) ?: return
        val source = style.getSourceAs<ImageSource>(SOURCE)
        if (source == null) {
            style.addSource(ImageSource(SOURCE, corners(picture), bitmap))
        } else {
            source.setCoordinates(corners(picture))
            source.setImage(bitmap)
        }
        if (layer != null) {
            layer.setProperties(PropertyFactory.visibility(Property.VISIBLE))
            return
        }
        val added = RasterLayer(LAYER, SOURCE).withProperties(
            // A pixel is a kilometre: a step stays a step, not smoothed into its neighbours.
            PropertyFactory.rasterResampling(Property.RASTER_RESAMPLING_NEAREST),
            // As see-through as the forecast map's squares, by the zoom.
            PropertyFactory.rasterOpacity(
                Expression.interpolate(
                    Expression.linear(),
                    Expression.zoom(),
                    Expression.stop(ThermalMapLayer.REGION_ZOOM, ThermalMapLayer.OPACITY_REGION),
                    Expression.stop(ThermalMapLayer.VALLEY_ZOOM, ThermalMapLayer.OPACITY_VALLEY),
                ),
            ),
            // A frame takes the place of the one before at once: two times are never mixed.
            PropertyFactory.rasterFadeDuration(0f),
        )
        val under = style.getLayer(ThermalHoursLayers.SQUARES)
        if (under != null) style.addLayerAbove(added, under.id) else style.addLayer(added)
    }
}
