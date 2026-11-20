package fly.venti.app.design

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import app.venti.design.VentiLight
import app.venti.design.VentiMapLight
import app.venti.design.VentiScaleBreaks

/**
 * The colours drawn on the map, from the design pack's tokens. The map is
 * light in both appearances, so what is drawn on it takes the light theme's
 * values.
 */
object Palette {
    /** A flight's line on the overview map: the light theme's primary. */
    val accentOnMap: Color = VentiLight.primary

    /** The near-black of dots, dashes and casings drawn on the map. */
    val inkOnMap: Color = VentiMapLight.trackCasing

    /** The halo around words written on the map. */
    val haloOnMap: Color = VentiMapLight.labelHalo
}

/**
 * How fast the glider was climbing or sinking: the pack's vario scale, seven
 * steps at its breaks (−3, −1.5, −0.5, +0.5, +1.5, +3 m/s). Cool is down,
 * warm is up: teal where it sank, grey where it held its height, coral and
 * gold where it climbed.
 */
enum class ClimbBand(
    /** The band in words, for the legend. */
    val label: String,
) {
    StrongSink("Sinking faster than 3 m/s"),
    Sink("Sinking 1.5 to 3 m/s"),
    LightSink("Sinking 0.5 to 1.5 m/s"),
    Level("Within 0.5 m/s of level"),
    Climb("Climbing 0.5 to 1.5 m/s"),
    GoodClimb("Climbing 1.5 to 3 m/s"),
    StrongClimb("Climbing faster than 3 m/s"),
    ;

    /** The band's colour on the map, which is light in both appearances. */
    val color: Color get() = VentiLight.vario[ordinal]

    /** The band's colour as red, green and blue: `0xRRGGBB`. */
    val rgb: Int get() = color.toArgb() and 0xFFFFFF

    /** The band's colour on the screen's own surface, in the theme in use. */
    val ink: Color
        @Composable @ReadOnlyComposable
        get() = Venti.colours.vario[ordinal]

    companion object {
        fun of(metresPerSecond: Double): ClimbBand {
            val step = VentiScaleBreaks.vario.count { metresPerSecond >= it }
            return entries[step]
        }
    }
}
