//! What the vario should sound like for a climb rate.

/// The sound for one moment: a tone that beeps, or sounds without pause.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    /// Pitch in hertz.
    pub frequency_hz: f64,
    /// Length of one beep-and-pause cycle in seconds.
    pub cycle_s: f64,
    /// The share of each cycle that sounds. One means a continuous tone.
    pub duty: f64,
}

/// Climb at which the vario starts to beep, metres per second.
pub const LIFT_THRESHOLD_MS: f64 = 0.2;
/// Sink at which the alarm starts, metres per second.
pub const SINK_THRESHOLD_MS: f64 = -2.5;

/// The tone for a climb rate, or `None` for silence.
///
/// In lift the vario beeps, higher and faster the stronger the climb, up to
/// 6 m/s. In heavy sink it gives a continuous low tone that falls with the
/// sink rate. In between it is silent, as in an ordinary glide.
///
/// To verify in the air: these pitches and rhythms are our own first
/// setting, modelled on how paragliding varios behave. They are meant to be
/// tuned by ear and later made adjustable.
pub fn tone(climb_ms: f64) -> Option<Tone> {
    if climb_ms >= LIFT_THRESHOLD_MS {
        let strength = ((climb_ms - LIFT_THRESHOLD_MS) / (6.0 - LIFT_THRESHOLD_MS)).min(1.0);
        Some(Tone {
            frequency_hz: 700.0 + 900.0 * strength,
            cycle_s: 0.60 - 0.46 * strength,
            duty: 0.5,
        })
    } else if climb_ms <= SINK_THRESHOLD_MS {
        let strength = ((SINK_THRESHOLD_MS - climb_ms) / 5.5).min(1.0);
        Some(Tone {
            frequency_hz: 400.0 - 200.0 * strength,
            cycle_s: 1.0,
            duty: 1.0,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glide_is_silent() {
        for climb in [-2.4, -1.2, 0.0, 0.19] {
            assert_eq!(tone(climb), None, "{climb}");
        }
    }

    #[test]
    fn stronger_lift_beeps_higher_and_faster() {
        let (weak, good, strong) = (tone(0.3).unwrap(), tone(2.0).unwrap(), tone(5.0).unwrap());
        assert!(weak.frequency_hz < good.frequency_hz && good.frequency_hz < strong.frequency_hz);
        assert!(weak.cycle_s > good.cycle_s && good.cycle_s > strong.cycle_s);
        assert!(weak.duty < 1.0);
        // Beyond 6 m/s it gets no higher.
        assert_eq!(tone(6.0), tone(12.0));
    }

    #[test]
    fn heavy_sink_is_a_continuous_falling_tone() {
        let (alarm, worse) = (tone(-2.5).unwrap(), tone(-5.0).unwrap());
        assert_eq!(alarm.duty, 1.0);
        assert!(worse.frequency_hz < alarm.frequency_hz);
        assert!(alarm.frequency_hz < tone(0.2).unwrap().frequency_hz);
    }
}
