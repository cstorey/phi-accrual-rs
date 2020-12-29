#[macro_use]
extern crate log;

use special_fun::FloatSpecial;
use std::collections::VecDeque;
use std::f64;

#[derive(Clone, Debug)]
pub struct PhiFailureDetector {
    min_stddev: f64,
    history_size: usize,
    buf: VecDeque<u64>,
    prev_heartbeat: Option<u64>,
}

impl PhiFailureDetector {
    pub fn new() -> PhiFailureDetector {
        Self::default()
    }

    pub fn min_stddev(self, min_stddev: f64) -> PhiFailureDetector {
        assert!(min_stddev > 0.0, "min_stddev must be > 0.0");
        PhiFailureDetector { min_stddev, ..self }
    }

    pub fn history_size(self, count: usize) -> PhiFailureDetector {
        assert!(count > 0, "history_size must > 0");
        PhiFailureDetector {
            history_size: count,
            ..self
        }
    }
    pub fn heartbeat(&mut self, t: u64) {
        match &mut self.prev_heartbeat {
            prev @ &mut None => {
                *prev = Some(t);
            }
            &mut Some(ref mut prev) => {
                if t < *prev {
                    return;
                };
                let delta = t - *prev;
                self.buf.push_back(delta);
                *prev = t;
                if self.buf.len() > self.history_size {
                    let _ = self.buf.pop_front();
                }
            }
        }
    }

    /// def ϕ(Tnow ) = − log10(Plater (Tnow − Tlast))
    pub fn phi(&self, now: u64) -> f64 {
        match &self.prev_heartbeat {
            Some(prev_time) if now > *prev_time => {
                trace!(
                    "now:{} - prev_heartbeat:{} = {:?}",
                    now,
                    prev_time,
                    now - prev_time
                );
                let p_later = self.p_later(now - prev_time);
                -p_later.log10()
            }
            Some(prev_time) => {
                trace!("now:{} <= prev_heartbeat:{}", now, prev_time);
                0.0
            }
            None => 0.0,
        }
    }

    /// Returns the time t (within epsilon) at which phi will be >= val .
    pub fn next_crossing_at(&self, now: u64, threshold: f64) -> u64 {
        let phappened = 1.0 - (10.0f64).powf(-threshold);

        let x = phappened.norm_inv();
        let mean = stats::mean(self.buf.iter().cloned());
        let stddev = stats::stddev(self.buf.iter().cloned()).max(self.min_stddev);
        let diff = x * stddev + mean;
        let then = now + diff.ceil() as u64;

        trace!(
            "threshold:{}; phappened:{}; x:{}; mean:{}; stddev:{}; diff:{}; then:{}",
            threshold,
            phappened,
            x,
            mean,
            stddev,
            diff,
            then
        );

        then
    }

    fn p_later(&self, diff: u64) -> f64 {
        let mean = stats::mean(self.buf.iter().cloned());
        let stddev = stats::stddev(self.buf.iter().cloned()).max(self.min_stddev);
        let x = (diff as f64 - mean) / stddev;
        // let cdf = 0.5*(1.0+ (x/(2.0f64).sqrt()).erf())
        let p = 1.0 - x.norm();
        trace!(
            "diff:{:e}; mean:{:e}; stddev:{:e} x:{:e}; p_later:{:e}",
            diff as f64,
            mean,
            stddev,
            x,
            p
        );
        // We want to avoid returning zero, as we want the logarithm of the probability.
        // And the log of zero is meaningless.
        if p < f64::MIN_POSITIVE {
            f64::MIN_POSITIVE
        } else {
            p
        }
    }
}

impl Default for PhiFailureDetector {
    fn default() -> Self {
        PhiFailureDetector {
            min_stddev: 1.0,
            history_size: 10,
            buf: VecDeque::new(),
            prev_heartbeat: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PhiFailureDetector;
    use rand::thread_rng;
    use rand_distr::Distribution;
    use rand_distr::LogNormal;
    #[test]
    fn should_fail_when_no_heartbeats() {
        env_logger::try_init().unwrap_or_default();

        let mut detector = PhiFailureDetector::new();
        for t in 0..100 {
            detector.heartbeat(t);
            let phi = detector.phi(t);
            trace!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            if t > 10 {
                assert!(phi < 1.0);
            }
        }
        for t in 100..110 {
            let phi = detector.phi(t);
            trace!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
        }
        for &t in &[110, 200, 300] {
            let phi = detector.phi(t);
            trace!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            assert!(phi > 1.0, "t:{:?}; phi:{:?} > 1.0", t, phi);
        }
    }

    #[test]
    fn should_recover() {
        env_logger::try_init().unwrap_or_default();
        let mut detector = PhiFailureDetector::new().history_size(3);
        for t in 0..10 {
            detector.heartbeat(t);
            let phi = detector.phi(t);
            trace!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
        }
        for t in 20..30 {
            detector.heartbeat(t);
            let phi = detector.phi(t);
            trace!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            if t > 10 {
                assert!(phi < 1.0);
            }
        }
    }

    #[test]
    fn should_estimate_threshold_times() {
        env_logger::try_init().unwrap_or_default();
        let epsilon = 2;
        let mut detector = PhiFailureDetector::new().history_size(3);

        let mut t = 0;
        for n in 0u64..10 {
            let dist = LogNormal::new(10.0, 100.0).expect("lognormal");
            let diff = dist.sample(&mut thread_rng());
            t = n * 1000;
            trace!(
                "at:{:?}, diff:{:e}; phi:{:?}; det: {:?}",
                t,
                diff,
                detector.phi(t),
                detector
            );
            detector.heartbeat(t);
        }
        // Estimate the point at which
        let threshold = 1.0;
        let est_1 = detector.next_crossing_at(t, threshold);

        let pre = detector.phi(est_1 - epsilon);
        let at = detector.phi(est_1);
        assert!(
            pre < threshold && at >= threshold,
            "phi({}):{:?} < {:?} && phi({}):{:?} >= {:?}",
            est_1 - epsilon,
            pre,
            threshold,
            est_1,
            at,
            threshold
        );
    }
}
