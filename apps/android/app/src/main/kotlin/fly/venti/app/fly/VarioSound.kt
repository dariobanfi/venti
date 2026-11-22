package fly.venti.app.fly

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.os.Process
import fly.venti.core.ToneView
import kotlin.concurrent.thread

/**
 * Plays the vario tone the core describes.
 *
 * The sound is made sample by sample by a [ToneOscillator] on a thread of
 * its own and written to the phone's audio output. It plays as media: the
 * ringer's silent mode does not mute it, and music or navigation from other
 * apps keeps playing beside it. With the recording service running it goes
 * on with the screen off.
 */
class VarioSound : TonePlayer {
    private val oscillator = ToneOscillator(SAMPLE_RATE.toDouble())

    @Volatile
    private var player: Thread? = null

    override fun start() {
        if (player != null) return
        player = thread(name = "venti-vario-sound") { playUntilStopped() }
    }

    override fun stop() {
        oscillator.tone = null
        val stopped = player
        player = null
        stopped?.join(STOP_TIMEOUT_MILLISECONDS)
    }

    override fun play(tone: ToneView?) {
        oscillator.tone = tone
    }

    private fun playUntilStopped() {
        Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
        val track = try {
            makeTrack()
        } catch (_: RuntimeException) {
            // No audio output to be had. The cockpit works without sound.
            return
        }
        // Ten milliseconds at a time keeps the tone close behind the climb.
        val samples = FloatArray(SAMPLE_RATE / 100)
        try {
            track.play()
            while (player === Thread.currentThread()) {
                oscillator.render(samples)
                if (track.write(samples, 0, samples.size, AudioTrack.WRITE_BLOCKING) < 0) break
            }
        } catch (_: IllegalStateException) {
            // The output went away under us.
        } finally {
            track.release()
        }
    }

    private fun makeTrack(): AudioTrack {
        val format = AudioFormat.Builder()
            .setSampleRate(SAMPLE_RATE)
            .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
            .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
            .build()
        val attributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        val smallest = AudioTrack.getMinBufferSize(
            SAMPLE_RATE, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_FLOAT,
        )
        return AudioTrack.Builder()
            .setAudioAttributes(attributes)
            .setAudioFormat(format)
            .setBufferSizeInBytes(smallest)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
            .build()
    }

    private companion object {
        const val SAMPLE_RATE = 44_100
        const val STOP_TIMEOUT_MILLISECONDS = 500L
    }
}

/** No sound at all: for tests, which have nobody listening. */
class SilentTone : TonePlayer {
    override fun start() = Unit

    override fun stop() = Unit

    override fun play(tone: ToneView?) = Unit
}
