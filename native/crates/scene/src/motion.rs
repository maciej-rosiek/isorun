//! Presentation timing for steps (the port of `packages/scene/src/motion.ts`). Seconds.

pub const STEP_SECONDS: f64 = 0.1;
pub const MAX_STEP_SECONDS: f64 = 0.14;
/// Bound catch-up to confirmed grid steps, not a queue of full animations.
pub const MAX_WALK_LAG: f64 = 2.0;
/// A longer gap is a fresh step, not a sample of continuous walking.
const CONTINUOUS_GAP: f64 = 0.25;
const JITTER_MARGIN: f64 = 0.02;

/// Measures actual movement arrivals, not render frames or unrelated scene updates.
#[derive(Clone, Debug, Default)]
pub struct StepCadence {
    last: Option<f64>,
    interval: Option<f64>,
}

impl StepCadence {
    /// The duration a step confirmed at `now` should take, and whether it continues a walk.
    pub fn confirm(&mut self, now: f64) -> (f64, bool) {
        let gap = self.last.map_or(f64::INFINITY, |l| now - l);
        self.last = Some(now);
        if !(0.0..=CONTINUOUS_GAP).contains(&gap) {
            self.interval = None;
            return (STEP_SECONDS, false);
        }
        // coalesced replies are not evidence that the server's normal cadence is faster
        if gap >= 0.025 {
            self.interval = Some(match self.interval {
                None => gap,
                Some(i) => i * 0.75 + gap * 0.25,
            });
        }
        let duration = MAX_STEP_SECONDS.min(STEP_SECONDS.max(self.interval.unwrap_or(0.0) + JITTER_MARGIN));
        (duration, true)
    }

    pub fn reset(&mut self) {
        self.last = None;
        self.interval = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepCurve {
    pub length: f64,
    pub duration: f64,
    pub start_speed: f64,
    pub continuous: bool,
}

pub fn plan_step(length: f64, duration: f64, speed: Option<f64>, continuous: bool) -> StepCurve {
    let default = (if continuous { 1.0 } else { 3.0 }) * length / duration;
    StepCurve { length, duration, start_speed: speed.unwrap_or(default).min(3.0 * length / duration).max(0.0), continuous }
}

/// A finite, monotone landing: (distance, speed). Its velocity can be carried into the next step.
pub fn sample_step(curve: &StepCurve, elapsed: f64) -> (f64, f64) {
    let StepCurve { length, duration, start_speed: start, continuous } = *curve;
    if duration - elapsed < 1e-9 {
        return (length, 0.0);
    }
    let t = (elapsed / duration).max(0.0);
    if !continuous {
        return (
            length * t * t * (3.0 - 2.0 * t) + start * duration * t * (1.0 - t).powi(2),
            (length * 6.0 * t * (1.0 - t) / duration + start * (1.0 - t) * (1.0 - 3.0 * t)).max(0.0),
        );
    }
    // blend velocity for the first 20%, cruise until 75%, then brake
    let ramp = 0.2;
    let brake_at = 0.75;
    let brake = 1.0 - brake_at;
    let cruise = (length / duration - start * ramp / 2.0) / (1.0 - ramp / 2.0 - brake / 2.0);
    let smooth = |u: f64| u * u * (3.0 - 2.0 * u);
    let integral = |u: f64| u.powi(3) - u.powi(4) / 2.0;
    let (distance, speed) = if t < ramp {
        let u = t / ramp;
        (start * t + (cruise - start) * ramp * integral(u), start + (cruise - start) * smooth(u))
    } else if t < brake_at {
        (ramp * (start + cruise) / 2.0 + cruise * (t - ramp), cruise)
    } else {
        let u = (t - brake_at) / brake;
        (ramp * (start + cruise) / 2.0 + cruise * (brake_at - ramp) + cruise * brake * (u - integral(u)), cruise * (1.0 - smooth(u)))
    };
    ((distance * duration).clamp(0.0, length), speed.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_land() {
        for continuous in [false, true] {
            let c = plan_step(1.0, 0.12, None, continuous);
            assert_eq!(sample_step(&c, 0.12).0, 1.0);
            let mut last = 0.0;
            for i in 0..=12 {
                let (d, _) = sample_step(&c, i as f64 * 0.01);
                assert!(d >= last - 1e-9 && d <= 1.0);
                last = d;
            }
        }
    }

    #[test]
    fn cadence() {
        let mut c = StepCadence::default();
        assert_eq!(c.confirm(0.0), (STEP_SECONDS, false));
        let (d, cont) = c.confirm(0.12);
        assert!(cont && d > 0.13 && d <= MAX_STEP_SECONDS);
    }
}
