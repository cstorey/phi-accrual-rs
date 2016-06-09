#[macro_use]
extern crate log;

use std::collections::VecDeque;


#[derive(Clone,Debug)]
pub struct PhiFailureDetector {
    min_stddev: f64,
    buf: VecDeque<u64>,
}

impl PhiFailureDetector {
    pub fn new() -> PhiFailureDetector {
        PhiFailureDetector {
            min_stddev: 0.01,
            buf: VecDeque::new(),
        }
    }

    pub fn heartbeat(&mut self, t: u64) {
        self.buf.push_back(t)
    }

    /// def ϕ(Tnow ) = − log10(Plater (Tnow − Tlast))
    pub fn phi(&mut self, now: u64) -> f64 {
        if let Some(&prev_time) = self.buf.back() {
            let p_later = self.p_later(now - prev_time);
            trace!("diff: {:?}; p_later:{:?}", now - prev_time, p_later);
            -p_later.log10()
        } else {
            0.0
        }
    }

    fn p_later(&self, diff: u64) -> f64 {
        let deltasum = if let (Some(&front), Some(&back)) = (self.buf.front(), self.buf.back()) {
            back - front
        } else {
            0
        };
        let nitems = self.buf.len() - 1;
        let mean = nitems as f64 / deltasum as f64;
        let variance = self.buf
                           .iter()
                           .zip(self.buf.iter().skip(1))
                           .map(|(&a, &b)| b - a)
                           .map(|i| (mean - i as f64).powi(2))
                           .fold(0_f64, |acc, i| acc + i) / nitems as f64;

        let stddev = variance.sqrt().max(self.min_stddev);
        let y = (diff as f64 - mean) / stddev;
        let e = (-y * (1.5976 + 0.070566 * y * y)).exp();
        let cdf = if diff as f64 > mean {
            e / (1.0 + e)
        } else {
            1.0 - 1.0 / (1.0 + e)
        };
        trace!("diff:{:?}; mean:{:?}; stddev:{:?}; y:{:?}; e:{:?}; cdf:{:?}",
               diff,
               mean,
               stddev,
               y,
               e,
               cdf);
        cdf
    }
}

#[cfg(test)]
mod tests {
    extern crate env_logger;
    use super::PhiFailureDetector;
    #[test]
    fn should_fail_when_no_heartbeats() {
        env_logger::init().unwrap_or(());
        let mut detector = PhiFailureDetector::new();
        for t in 0..100 {
            detector.heartbeat(t);
            let phi = detector.phi(t);
            debug!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            if t > 10 {
                assert!(phi < 1.0);
            }
        }
        for t in 100..110 {
            let phi = detector.phi(t);
            debug!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
        }
        for &t in &[110, 200, 300] {
            let phi = detector.phi(t);
            debug!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            assert!(phi > 1.0);
        }
    }

    #[test]
    fn should_recover() {
        env_logger::init().unwrap_or(());
        let mut detector = PhiFailureDetector::new();
        for t in 0..10 {
            detector.heartbeat(t);
            let phi = detector.phi(t);
            debug!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
        }
        for t in 20..30 {
            detector.heartbeat(t);
            let phi = detector.phi(t);
            debug!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            if t > 10 {
                assert!(phi < 1.0);
            }
        }
    }
}
