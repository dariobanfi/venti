package fly.venti.app.fly

import fly.venti.core.ToneView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

/** The vario tone, sample by sample. */
class ToneOscillatorTest {
    private val sampleRate = 44_100

    /** The next `seconds` of sound. */
    private fun ToneOscillator.listen(seconds: Double): FloatArray =
        FloatArray((sampleRate * seconds).toInt()).also(::render)

    /** How many times the wave crosses from below zero to above. */
    private fun cycles(samples: FloatArray): Int =
        (1 until samples.size).count { samples[it - 1] < 0 && samples[it] >= 0 }

    @Test
    fun withNoToneThereIsSilence() {
        val samples = ToneOscillator(sampleRate.toDouble()).listen(0.1)

        assertTrue(samples.all { it == 0f })
    }

    @Test
    fun aContinuousToneHasThePitchTheCoreAskedFor() {
        val oscillator = ToneOscillator(sampleRate.toDouble())
        oscillator.tone = ToneView(frequencyHz = 440.0, cycleS = 1.0, duty = 1.0)

        val samples = oscillator.listen(1.0)

        assertEquals(440.0, cycles(samples).toDouble(), 2.0)
        // It reaches its full loudness and never passes it.
        val loudest = samples.maxOf { abs(it) }
        assertEquals(ToneOscillator.VOLUME.toFloat(), loudest, 0.01f)
    }

    @Test
    fun aBeepSoundsForItsShareOfEachCycle() {
        val oscillator = ToneOscillator(sampleRate.toDouble())
        // Four beeps a second, each sounding for half its cycle.
        oscillator.tone = ToneView(frequencyHz = 880.0, cycleS = 0.25, duty = 0.5)

        val samples = oscillator.listen(1.0)

        // Loudness over windows of five milliseconds: about half are loud.
        val window = sampleRate / 200
        val loud = samples.toList().chunked(window).count { chunk -> chunk.maxOf { abs(it) } > 0.3f }
        assertEquals(100.0, loud.toDouble(), 12.0)
    }

    @Test
    fun theToneFadesInsteadOfClicking() {
        val oscillator = ToneOscillator(sampleRate.toDouble())
        oscillator.tone = ToneView(frequencyHz = 440.0, cycleS = 1.0, duty = 1.0)
        oscillator.listen(0.5)

        // The core says "silence": the sound dies away within a few
        // hundredths of a second, with no jump from one sample to the next.
        oscillator.tone = null
        val tail = oscillator.listen(0.1)

        val largestStep = (1 until tail.size).maxOf { abs(tail[it] - tail[it - 1]) }
        assertTrue("$largestStep", largestStep < 0.05f)
        assertTrue(abs(tail.last()) < 0.001f)
    }
}
