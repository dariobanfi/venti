package fly.venti.app.design

import androidx.compose.ui.graphics.Color

/**
 * The app's colours. `docs/design.md` explains the choices.
 *
 * Everything else on screen uses Material's own neutral colours, so the app
 * follows light and dark appearance without a colour of ours in the way.
 */
object Palette {
    /**
     * The one accent: the main button, selected controls, and a flight's
     * line in lists, charts and on the overview map. Violet, because it is
     * none of the colours that carry meaning on a track.
     */
    val accentLight = Color(0xFF6250D6)
    val accentDark = Color(0xFF9085E9)

    /** The accent as drawn on the map, which is light in both appearances. */
    val accentOnMap = accentLight

    /** The near-black of dots and dashes drawn on the map. */
    val inkOnMap = Color(0xFF1F1F1D)

    /**
     * The ground of something that warns without being the worst: an
     * airspace that is very near, a wind that leaves little way, a landing
     * only just within reach. The same in both appearances, with dark type
     * on it: a warning is not themed.
     */
    val amber = Color(0xFFFAB31A)
    val inkOnAmber = Color(0xFF0A0A0A)

    /** The ground of the worst: inside an airspace, no landing in reach. White type on it. */
    val red = Color(0xFFB82429)

    /**
     * Amber as the colour of type on a plain surface. The ground's amber
     * cannot be read on a light one, so it is darker there.
     */
    fun amberType(dark: Boolean): Color = if (dark) amber else Color(0xFF8A5A00)

    /** Mean wind in a chart: one blue, a step lighter on a dark ground. */
    fun wind(dark: Boolean): Color = if (dark) Color(0xFF6DA7EC) else Color(0xFF256ABF)

    /** The span from mean wind up to the gusts, in the same blue held back. */
    fun gusts(dark: Boolean): Color = if (dark) Color(0xFF1C5CAB) else Color(0xFFB7D3F6)
}

/**
 * How fast the glider was climbing or sinking, in five bands.
 *
 * A track is coloured by band: blue where it sank, grey where it held its
 * height, orange and red where it climbed. Two hues with a neutral middle
 * stay distinguishable for colour-blind pilots; the palette was checked for
 * that.
 */
enum class ClimbBand(
    /** The colour as red, green and blue: `0xRRGGBB`. */
    val rgb: Int,
    /** The band in words, for the legend. */
    val label: String,
) {
    StrongSink(0x184F95, "Sinking faster than 2 m/s"),
    Sink(0x3987E5, "Sinking 0.5 to 2 m/s"),
    Level(0x85847F, "Within 0.5 m/s of level"),
    Climb(0xEB6834, "Climbing 0.5 to 2 m/s"),
    StrongClimb(0xB8242B, "Climbing faster than 2 m/s");

    val color: Color get() = Color(0xFF000000 or rgb.toLong())

    companion object {
        fun of(metresPerSecond: Double): ClimbBand = when {
            metresPerSecond < -2.0 -> StrongSink
            metresPerSecond < -0.5 -> Sink
            metresPerSecond < 0.5 -> Level
            metresPerSecond < 2.0 -> Climb
            else -> StrongClimb
        }
    }
}
