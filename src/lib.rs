#[macro_use]
extern crate log;
extern crate special;
extern crate stats;

use std::collections::VecDeque;
use special::Error;

#[derive(Clone,Debug)]
pub struct PhiFailureDetector {
    min_stddev: f64,
    history_size: usize,
    buf: VecDeque<u64>,
}

impl PhiFailureDetector {
    pub fn new() -> PhiFailureDetector {
        PhiFailureDetector {
            min_stddev: 0.01,
            history_size: 10,
            buf: VecDeque::new(),
        }
    }
    pub fn min_stddev(self, min_stddev: f64) -> PhiFailureDetector {
        assert!(min_stddev > 0.0, "min_stddev must be > 0.0");
        PhiFailureDetector { min_stddev: min_stddev, ..self }
    }

    pub fn history_size(self, count: usize) -> PhiFailureDetector {
        assert!(count > 0, "history_size must > 0");
        PhiFailureDetector { history_size: count, ..self }
    }
    pub fn heartbeat(&mut self, t: u64) {
        self.buf.push_back(t);
        if self.buf.len() > self.history_size {
            let _ = self.buf.pop_front();
        }
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
        let deltas = self.buf.iter().zip(self.buf.iter().skip(1)).map(|(&a, &b)| b - a);
        let mean = stats::mean(deltas);

        let deltas = self.buf.iter().zip(self.buf.iter().skip(1)).map(|(&a, &b)| b - a);
        let stddev = stats::stddev(deltas).max(self.min_stddev);
        let x = (diff as f64 - mean) / stddev;
        let cdf = Self::cdf(x);
        trace!("diff:{:?}; mean:{:?}; stddev:{:?}; x:{:?}; cdf:{:?}",
               diff,
               mean,
               stddev,
               x,
               cdf);
        1.0 - cdf
    }

    fn cdf(x:f64) -> f64 {
        0.5*(1.0+ (x/(2.0f64).sqrt()).erf())
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
            println!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            if t > 10 {
                assert!(phi < 1.0);
            }
        }
        for t in 100..110 {
            let phi = detector.phi(t);
            println!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
        }
        for &t in &[110, 200, 300] {
            let phi = detector.phi(t);
            println!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            assert!(phi > 1.0, "t:{:?}; phi:{:?} > 1.0", t, phi);
        }
    }

    #[test]
    fn should_recover() {
        env_logger::init().unwrap_or(());
        let mut detector = PhiFailureDetector::new().history_size(3);
        for t in 0..10 {
            detector.heartbeat(t);
            let phi = detector.phi(t);
            println!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
        }
        for t in 20..30 {
            detector.heartbeat(t);
            let phi = detector.phi(t);
            println!("at:{:?}, phi:{:?}; det: {:?}", t, phi, detector);
            if t > 10 {
                assert!(phi < 1.0);
            }
        }
    }
}
