#[macro_use]
extern crate log;
extern crate stats;

use std::collections::VecDeque;
use std::iter;

#[derive(Clone,Debug)]
pub struct PhiFailureDetector {
    min_stddev: f64,
    history_size: usize,
    buf: VecDeque<u64>,
    prev_heartbeat: Option<u64>,
}

impl PhiFailureDetector {
    pub fn new() -> PhiFailureDetector {
        PhiFailureDetector {
            min_stddev: 1.0,
            history_size: 10,
            buf: VecDeque::new(),
            prev_heartbeat: None,
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
        match &mut self.prev_heartbeat {
            prev @ &mut None => {
                *prev = Some(t);
                return;
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
            &Some(prev_time) if now > prev_time => {
                trace!("now:{} - prev_heartbeat:{} = {:?}",
                       now,
                       prev_time,
                       now - prev_time);
                let p_later = self.p_later(now - prev_time);
                -p_later.log10()
            }
            &Some(prev_time) => {
                trace!("now:{} <= prev_heartbeat:{}", now, prev_time);
                0.0
            }
            &None => 0.0,
        }
    }

    /// Returns the time t (within epsilon) at which phi will be >= val .
    pub fn next_crossing_at(&self, now: u64, epsilon: u64, threshold: f64) -> u64 {
        trace!("Approxmating next crossing...");
        let res = approximate_inverse(now, epsilon, |t| self.phi(t as u64) - threshold);
        trace!("Approxmation done");
        res as u64
    }

    fn p_later(&self, diff: u64) -> f64 {
        let mean = stats::mean(self.buf.iter().cloned());
        let stddev = stats::stddev(self.buf.iter().cloned()).max(self.min_stddev);
        let x = (diff as f64 - mean) / stddev;
        // let cdf = 0.5*(1.0+ (x/(2.0f64).sqrt()).erf())
        trace!("diff:{:?}; mean:{:?}; stddev:{:?}; x:{:?}",
               diff,
               mean,
               stddev,
               x);
        let e = (-x * (1.5976 + 0.070566 * x * x)).exp();
        let cdf = if e.is_finite() {
            e / (1.0 + e)
        } else if e > 0.0 {
            1.0
        } else {
            0.0
        };
        let p = cdf /* if diff >mean {
            1.0 - cdf
        } else {
            cdf
        }*/;
        trace!("e.is_finite():{:?}; e >0.0:{:?}; e <= 0.0:{:?}",
               e.is_finite(),
               e > 0.0,
               e <= 0.0);
        trace!("x:{:e}; e:{:e}; cdf:{:e} p_later:{:e}", x, e, cdf, p);
        trace!("diff:{:e}; mean:{:e}; stddev:{:e} x:{:e}; cdf:{:e}; p_later:{:e}",
               diff as f64,
               mean,
               stddev,
               x,
               cdf,
               p);
        p

    }
}

// Searches for the root of f(x) = 0.0.
// This assumes that the function is monotonic, and the answer is positive.
fn approximate_inverse<F: Fn(u64) -> f64>(mut lower: u64, in_tolerance: u64, f: F) -> u64 {

    // Exponential search for the upper bound.
    let mut upper = iter::repeat(())
        .scan(lower, |state, ()| {
                let r = Some(*state);
                *state = *state * 2;
                r
                })
        .inspect(|&x| trace!("f({:?}) = {:?}", x, f(x)))
        .skip_while(|&x| f(x) < 0.0)
        .inspect(|&x| trace!("upper bound: f({:?}) = {:?}", x, f(x)))
        .next().unwrap();


    let mut l_r = f(lower);
    let mut u_r = f(upper);

    // Check that the authors assumptions hold.
    assert!(l_r < 0.0);
    assert!(u_r >= 0.0);
    assert!(!l_r.is_nan());
    assert!(!u_r.is_nan());

    // Binary search between upper and lower for the zero-crossing.
    for _ in 0..64 {
        trace!("f({}) = {} < x < f({}) = {}", lower, l_r, upper, u_r);
        assert!(l_r < 0.0);
        assert!(u_r >= 0.0);
        trace!("upper-lower:{} <=? in_tolerance:{}",
               upper - lower,
               in_tolerance);
        if (upper - lower) <= in_tolerance {
            return upper;
        }

        let mid = (lower + upper) / 2;
        let m_r = f(mid);
        trace!("f({}) => {}", mid, m_r);
        assert!(!m_r.is_nan());

        if m_r < 0.0 {
            trace!("right half");
            lower = mid;
            l_r = m_r;
        } else {
            trace!("left half");
            upper = mid;
            u_r = m_r;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    extern crate env_logger;
    extern crate rand;
    use super::PhiFailureDetector;
    use self::rand::distributions::normal::LogNormal;
    use self::rand::distributions::Sample;
    use self::rand::thread_rng;
    #[test]
    fn should_fail_when_no_heartbeats() {
        env_logger::init().unwrap_or(());
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
        env_logger::init().unwrap_or(());
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
        env_logger::init().unwrap_or(());
        let epsilon = 2;
        let mut detector = PhiFailureDetector::new().history_size(3);

        for n in 0u64..10 {
            let mut dist = LogNormal::new(10.0, 100.0);
            let diff = dist.sample(&mut thread_rng());
            let t = n as f64 * 1000.0;
            trace!("at:{:?}, diff:{:e}; phi:{:?}; det: {:?}",
                   t,
                   diff,
                   detector.phi(t as u64),
                   detector);
            detector.heartbeat(t as u64);
        }
        // Estimate the point at which
        let threshold = 1.0;
        let est_1 = detector.next_crossing_at(10_000, epsilon, threshold);

        let pre = detector.phi(est_1 - epsilon);
        let at = detector.phi(est_1);
        assert!(pre < threshold && at >= threshold,
                "phi({}):{:?} < {:?} && phi({}):{:?} >= {:?}",
                est_1 - epsilon,
                pre,
                threshold,
                est_1,
                at,
                threshold);
    }
}
