package fly.venti.app.fly

import fly.venti.core.ToneView
import kotlin.math.PI
import kotlin.math.sin

/**
 * Makes the samples of the vario tone.
 *
 * The core says what the vario should sound like a few times a second: a
 * pitch and a beep rhythm, or silence. `render` runs on the audio thread,
 * which must never wait: the only thing shared with the rest of the app is
 * the wanted tone, a single reference that is swapped whole.
 */
class ToneOscillator(private val sampleRate: Double) {
    /** What to sound like from now on. `null` is silence. */
    @Volatile
    var tone: ToneView? = null

    // Touched only by the audio thread.
    private var phase = 0.0
    private var positionInCycle = 0.0
    private var frequency = 440.0
    private var loudness = 0.0

    /** Fills `samples` with the next stretch of sound, each between -1 and 1. */
    fun render(samples: FloatArray) {
        val tone = tone
        for (index in samples.indices) {
            var target = 0.0
            if (tone != null) {
                frequency = tone.frequencyHz
                positionInCycle += 1 / sampleRate
                if (positionInCycle >= tone.cycleS) {
                    positionInCycle = 0.0
                }
                target = if (positionInCycle < tone.cycleS * tone.duty) 1.0 else 0.0
            }
            // Fade in and out over about ten milliseconds; a hard edge clicks.
            loudness += (target - loudness) * FADE
            phase += 2 * PI * frequency / sampleRate
            if (phase > 2 * PI) {
                phase -= 2 * PI
            }
            samples[index] = (sin(phase) * loudness * VOLUME).toFloat()
        }
    }

    companion object {
        /** The loudest a sample gets, with room to spare below full scale. */
        const val VOLUME = 0.6

        private const val FADE = 0.002
    }
}
