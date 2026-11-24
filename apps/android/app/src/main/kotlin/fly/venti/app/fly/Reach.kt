package fly.venti.app.fly

import fly.venti.app.design.Venti
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import fly.venti.app.design.Format
import fly.venti.core.LandingOption
import fly.venti.core.Reach
import fly.venti.core.ReachView
import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.sign

/**
 * How a place within the glider's reach is set: plain with height to spare,
 * amber with little, red when the glide does not reach it, and held back
 * when nothing is known. Which of the four it is, is the core's to say.
 */
enum class ReachTone {
    Plain,
    Amber,
    Red,
    Quiet,
    ;

    companion object {
        fun of(reach: Reach): ReachTone = when (reach) {
            Reach.WITHIN -> Plain
            Reach.JUST -> Amber
            Reach.BEYOND -> Red
            Reach.UNKNOWN -> Quiet
        }
    }
}

/** The tone as the colour of type on a plain surface: in a list. */
val ReachTone.type: Color
    @Composable get() = when (this) {
        ReachTone.Plain -> MaterialTheme.colorScheme.onSurface
        ReachTone.Amber -> Venti.colours.caution
        ReachTone.Red -> MaterialTheme.colorScheme.error
        ReachTone.Quiet -> MaterialTheme.colorScheme.onSurfaceVariant
    }

/** What is said of a place's reach, in the cockpit's badge and in the list of places to fly to. */
object ReachWords {
    const val NO_LANDING = "No landing in reach"

    /** Said where the straight glide to a place meets a ridge first. */
    const val GROUND_IN_THE_WAY = "Ground in the way"

    const val HOW_IT_IS_WORKED_OUT =
        "The height beside a place is what the glider is expected to arrive with, by its trim speed and " +
            "glide ratio from the settings and the wind last measured, in a straight line. A mountain " +
            "marks a place with a ridge in the way of that line, where the phone has the ground; where it " +
            "has not, the ground in between is not looked at."

    /** A height to arrive with, "+510 m", to ten metres: the prediction is no finer. */
    fun height(arrivalM: Double): String {
        val tens = sign(arrivalM) * floor(abs(arrivalM) / 10 + 0.5) * 10
        return "${Format.signedMetres(tens)} m"
    }

    /**
     * The height beside a place in the list: what the glider would arrive
     * with, also when that is under the ground. Nothing where it is not
     * known.
     */
    fun beside(reach: ReachView?): String? {
        val arrival = reach?.arrivalHeightM ?: return null
        // With a ridge in the way the height it would arrive with says
        // nothing: the row shows a mountain in its place.
        return if (reach.reach == Reach.UNKNOWN || reach.groundInTheWayM != null) null else height(arrival)
    }

    /** The same for someone who cannot see the row. */
    fun spoken(reach: ReachView?): String? {
        if (reach?.groundInTheWayM != null) return "Out of reach: the ground is in the way."
        val arrival = reach?.arrivalHeightM
        return when (reach?.reach) {
            Reach.WITHIN, Reach.JUST -> arrival?.let { "Arriving ${height(it).removeSuffix(" m")} metres above it." }
            Reach.BEYOND -> "Out of reach."
            Reach.UNKNOWN, null -> null
        }
    }

    /**
     * The height to spare over the landing to think of first, for one
     * within reach; none for one that is not.
     */
    fun height(landing: LandingOption): String? {
        val arrival = landing.reach.arrivalHeightM ?: return null
        return when (landing.reach.reach) {
            Reach.WITHIN, Reach.JUST -> height(arrival)
            Reach.BEYOND, Reach.UNKNOWN -> null
        }
    }

    /** The badge to a screen reader. */
    fun spoken(landing: LandingOption): String =
        height(landing)?.let { "${landing.name}, arriving $it above it" } ?: NO_LANDING
}
