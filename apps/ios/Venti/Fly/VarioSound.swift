import AVFoundation
import os
import VentiCore

/// Plays the vario tone the core describes.
///
/// The core says what the vario should sound like a few times a second:
/// a pitch and a beep rhythm, or silence. The sound itself is made here,
/// sample by sample, on the audio thread. The app declares the audio
/// background mode, so the vario keeps sounding with the screen off.
/// When the phone sounds is the core's (`CockpitView.phoneTone`): there is
/// no mute here.
@MainActor
final class VarioSound {
    private let engine = AVAudioEngine()
    private let oscillator = ToneOscillator()
    private var source: AVAudioSourceNode?

    func start() {
        guard !engine.isRunning else { return }
        let session = AVAudioSession.sharedInstance()
        // Playback, so the ring/silent switch does not mute the vario, and
        // mixing, so music or navigation from other apps keeps playing.
        try? session.setCategory(.playback, mode: .default, options: [.mixWithOthers])
        try? session.setActive(true)

        guard let format = AVAudioFormat(standardFormatWithSampleRate: Self.sampleRate, channels: 1) else {
            return
        }
        if source == nil {
            let node = oscillator.makeNode(format: format, sampleRate: Self.sampleRate)
            engine.attach(node)
            engine.connect(node, to: engine.mainMixerNode, format: format)
            source = node
        }
        try? engine.start()
    }

    func stop() {
        oscillator.set(nil)
        engine.stop()
        try? AVAudioSession.sharedInstance().setActive(false, options: [.notifyOthersOnDeactivation])
    }

    /// Sets what the vario sounds like from now on. `nil` is silence.
    func play(_ tone: ToneView?) {
        oscillator.set(tone)
    }

    private nonisolated static let sampleRate = 44_100.0
}

/// Makes the samples. Its `render` runs on the audio thread, which must
/// never wait: the only thing shared with the rest of the app is the wanted
/// tone, behind a lock held for a few nanoseconds.
private final class ToneOscillator: @unchecked Sendable {
    private let wanted = OSAllocatedUnfairLock<ToneView?>(initialState: nil)

    // Touched only by the audio thread.
    private var phase = 0.0
    private var positionInCycle = 0.0
    private var frequency = 440.0
    private var loudness = 0.0

    func set(_ tone: ToneView?) {
        wanted.withLock { $0 = tone }
    }

    /// The audio node that plays this oscillator.
    ///
    /// It is made here, outside the main actor, on purpose: a closure
    /// written inside main-actor code is taken to run on the main thread,
    /// and Swift stops the app when the audio thread calls it.
    func makeNode(format: AVAudioFormat, sampleRate: Double) -> AVAudioSourceNode {
        AVAudioSourceNode(format: format) { [self] _, _, frameCount, buffers in
            render(frameCount: frameCount, into: buffers, sampleRate: sampleRate)
            return noErr
        }
    }

    func render(
        frameCount: AVAudioFrameCount, into list: UnsafeMutablePointer<AudioBufferList>, sampleRate: Double
    ) {
        let tone = wanted.withLock { $0 }
        let buffers = UnsafeMutableAudioBufferListPointer(list)

        for frame in 0..<Int(frameCount) {
            var target = 0.0
            if let tone {
                frequency = tone.frequencyHz
                positionInCycle += 1 / sampleRate
                if positionInCycle >= tone.cycleS {
                    positionInCycle = 0
                }
                target = positionInCycle < tone.cycleS * tone.duty ? 1 : 0
            }
            // Fade in and out over about ten milliseconds; a hard edge clicks.
            loudness += (target - loudness) * 0.002
            phase += 2 * .pi * frequency / sampleRate
            if phase > 2 * .pi {
                phase -= 2 * .pi
            }
            let sample = Float(sin(phase) * loudness * 0.6)
            for buffer in buffers {
                UnsafeMutableBufferPointer<Float>(buffer)[frame] = sample
            }
        }
    }
}
