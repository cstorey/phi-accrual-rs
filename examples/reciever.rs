#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate phi_accrual;
use clap::{Arg, App};
use std::io::prelude::*;
use std::io;
use std::net::TcpListener;
use std::time::{SystemTime, Duration};
use phi_accrual::PhiFailureDetector;

const BILLION: u64 = 1000000000;
const MIN_STABLE : u64 = 5;

fn main() {
    env_logger::init().unwrap();
    let matches = App::new("receiver")
        .arg(Arg::with_name("listener").takes_value(true).index(1).required(true))
        .get_matches();

    let target = value_t!(matches, "listener", String).unwrap_or_else(|e| e.exit());

    let start = SystemTime::now();
    debug!("Listening at:{:?}", target);
    let listener = TcpListener::bind(&*target).expect("listen");

    for sock in listener.incoming() {
        let mut sock = sock.expect("accept sock");
        info!("Accepted from:{}", sock.peer_addr().expect("peer addr"));
        let mut fd = PhiFailureDetector::new().min_stddev(1000_000.0 /* ns */);
        let dur = start.elapsed().expect("elapsed");
        // Record micro-seconds
        let t = dur.as_secs() as u64 * BILLION + dur.subsec_nanos() as u64;
        fd.heartbeat(t);
        let mut prev = start;
        let mut n_stable = 0;
        loop {
            debug!("fd:{:?}", fd);
            let read_result = sock.read(&mut [0; 1]);
            let now = SystemTime::now();
            let dur = now.duration_since(start).expect("duration_since");
            let t = dur.as_secs() as u64 * BILLION + dur.subsec_nanos() as u64;
            let phi = fd.phi(t);
            match read_result {
                Ok(res) => {
                    info!("Read {}bytes", res);
                    if res == 0 {
                        break;
                    }

                    if phi <= 3.0 {
                        n_stable += 1;
                    }
                    if n_stable ==  MIN_STABLE {
                        info!("Now stable at {:?}/{:?}", n_stable, phi);
                    }

                    info!("Interval:{:?}; stable:{}; Phi:{}",
                          now.duration_since(prev).expect("duration"),
                          n_stable,
                          phi);
                    fd.heartbeat(t);

                    prev = now;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    debug!("Got read timeout! stable:{}; phi:{}", n_stable, phi);
                    if n_stable > MIN_STABLE &&phi > 6.0 {
                        warn!("Bailing on unstable connection:{}/{}", n_stable, phi);
                        break;
                    }
                }
                Err(e) => {
                    error!("Read error:{:?}", e);
                    break;
                }
            }

            let threshold = match phi {
                phi if phi <= 1.0 => Some(1.0),
                phi if phi <= 2.0 => Some(2.0),
                phi if phi <= 3.0 => Some(3.0),
                phi if phi <= 6.0 => Some(6.0),
                _ => None,
            };
            let next = threshold.map(|next| {
                    fd.next_crossing_at(t, 1000 /* ns */, next)
                })
                .map(|d| d - t)
                .map(|d| Duration::new(d / BILLION, (d % BILLION) as u32));
            debug!("phi: stable:{}; current:{}; next:{:?}; in {:?}", n_stable, phi, threshold, next);

            sock.set_read_timeout(next).expect("set_read_timeout");
        }
    }
}
