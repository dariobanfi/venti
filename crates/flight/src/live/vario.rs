//! Climb rate from a stream of altitudes.

/// Smooths altitude and estimates climb rate with a two-state Kalman filter.
///
/// The filter believes the glider keeps its climb rate, disturbed by random
/// vertical accelerations, and that each altitude it is given is off by
/// random noise. From those two beliefs it works out how far to trust each
/// new reading: a noisy source is smoothed heavily, a clean one is followed
/// closely. This is the standard way to get a usable vario from a pressure
/// sensor, and it works at whatever rate readings arrive.
#[derive(Debug, Clone)]
pub struct VarioFilter {
    /// How wrong one altitude reading typically is, metres, squared.
    measurement_variance: f64,
    /// How hard the air typically pushes the glider up or down, m/s²,
    /// squared.
    acceleration_variance: f64,
    state: Option<State>,
}

#[derive(Debug, Clone, Copy)]
struct State {
    time_s: f64,
    altitude_m: f64,
    climb_ms: f64,
    /// The filter's uncertainty about altitude, about climb, and how the two
    /// errors go together.
    p: [[f64; 2]; 2],
}

impl VarioFilter {
    /// A filter for a source whose altitude readings are typically off by
    /// `measurement_sd_m` metres. `acceleration_sd` is how quickly the climb
    /// rate may change, in m/s²; about 1 suits a paraglider in thermals.
    pub fn new(measurement_sd_m: f64, acceleration_sd: f64) -> Self {
        Self {
            measurement_variance: measurement_sd_m * measurement_sd_m,
            acceleration_variance: acceleration_sd * acceleration_sd,
            state: None,
        }
    }

    /// Takes one altitude reading. Readings that are not later than the last
    /// one are ignored.
    pub fn update(&mut self, time_s: f64, altitude_m: f64) {
        let Some(state) = self.state else {
            self.state = Some(State {
                time_s,
                altitude_m,
                climb_ms: 0.0,
                // Sure of the altitude to within a reading, unsure of climb.
                p: [[self.measurement_variance, 0.0], [0.0, 25.0]],
            });
            return;
        };
        let dt = time_s - state.time_s;
        if dt <= 0.0 {
            return;
        }
        if dt > 10.0 {
            // A long gap: what the filter believed is stale. Start again.
            self.state = None;
            self.update(time_s, altitude_m);
            return;
        }

        // Predict: carry altitude forward at the current climb rate, and
        // grow the uncertainty by what random accelerations could have done.
        let altitude = state.altitude_m + state.climb_ms * dt;
        let climb = state.climb_ms;
        let q = self.acceleration_variance;
        let p = state.p;
        let p00 = p[0][0] + dt * (p[1][0] + p[0][1]) + dt * dt * p[1][1] + q * dt.powi(4) / 4.0;
        let p01 = p[0][1] + dt * p[1][1] + q * dt.powi(3) / 2.0;
        let p11 = p[1][1] + q * dt * dt;

        // Correct: move toward the reading by the share of the disagreement
        // that the uncertainties say is the filter's own fault.
        let innovation = altitude_m - altitude;
        let s = p00 + self.measurement_variance;
        let (k0, k1) = (p00 / s, p01 / s);
        self.state = Some(State {
            time_s,
            altitude_m: altitude + k0 * innovation,
            climb_ms: climb + k1 * innovation,
            p: [
                [(1.0 - k0) * p00, (1.0 - k0) * p01],
                [p01 - k1 * p00, p11 - k1 * p01],
            ],
        });
    }

    /// The smoothed altitude in metres, once there has been a reading.
    pub fn altitude_m(&self) -> Option<f64> {
        self.state.map(|state| state.altitude_m)
    }

    /// The climb rate in metres per second, once there has been a reading.
    pub fn climb_ms(&self) -> Option<f64> {
        self.state.map(|state| state.climb_ms)
    }

    /// When the last reading was taken.
    pub fn time_s(&self) -> Option<f64> {
        self.state.map(|state| state.time_s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repeatable stand-in for sensor noise: values in −1..1.
    fn noise(index: u32) -> f64 {
        let x = f64::from(index) * 12.9898;
        (x.sin() * 43_758.545_3).fract() * 2.0 - 1.0
    }

    #[test]
    fn follows_a_steady_climb_through_noise() {
        let mut filter = VarioFilter::new(0.5, 1.0);
        // Ten readings a second for 20 s, climbing at 2.5 m/s, each off by
        // up to half a metre.
        let mut late_climbs = Vec::new();
        for step in 0..200 {
            let time = f64::from(step) * 0.1;
            filter.update(time, 1000.0 + 2.5 * time + 0.5 * noise(step));
            if step >= 100 {
                late_climbs.push(filter.climb_ms().unwrap());
            }
        }
        // Each estimate wanders with the noise but stays close, and on
        // average they are right.
        assert!(late_climbs.iter().all(|climb| (climb - 2.5).abs() < 0.7));
        let mean = late_climbs.iter().sum::<f64>() / late_climbs.len() as f64;
        assert!((mean - 2.5).abs() < 0.1, "{mean}");
        let altitude = filter.altitude_m().unwrap();
        assert!((altitude - 1049.75).abs() < 0.5, "{altitude}");
    }

    #[test]
    fn notices_a_change_from_sink_to_climb_within_a_few_seconds() {
        let mut filter = VarioFilter::new(0.3, 1.0);
        let mut altitude = 1500.0;
        for step in 0..600 {
            let time = f64::from(step) * 0.1;
            altitude += if time < 30.0 { -0.12 } else { 0.3 };
            filter.update(time, altitude);
            if (time - 33.0).abs() < 1e-9 {
                // Three seconds into a 3 m/s climb after 1.2 m/s sink.
                let climb = filter.climb_ms().unwrap();
                assert!(climb > 2.0, "{climb}");
            }
        }
        assert!((filter.climb_ms().unwrap() - 3.0).abs() < 0.1);
    }

    #[test]
    fn works_at_one_reading_a_second() {
        let mut filter = VarioFilter::new(1.5, 1.0);
        for second in 0..60 {
            let time = f64::from(second);
            filter.update(time, 800.0 - 1.1 * time + 1.5 * noise(second));
        }
        let climb = filter.climb_ms().unwrap();
        assert!((climb + 1.1).abs() < 0.4, "{climb}");
    }

    #[test]
    fn starts_again_after_a_long_gap_and_ignores_readings_out_of_order() {
        let mut filter = VarioFilter::new(0.5, 1.0);
        filter.update(0.0, 1000.0);
        filter.update(1.0, 1003.0);
        filter.update(0.5, 5000.0);
        assert!(filter.altitude_m().unwrap() < 1010.0);

        filter.update(100.0, 2000.0);
        assert_eq!(filter.altitude_m(), Some(2000.0));
        assert_eq!(filter.climb_ms(), Some(0.0));
    }
}
