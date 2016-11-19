#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
extern crate env_logger;
use clap::{Arg, App};
use std::io::prelude::*;
use std::net::TcpStream;
use std::time::Duration;
use std::thread::sleep;


fn main() {
    env_logger::init().unwrap();
    let matches = App::new("sender")
        .arg(Arg::with_name("target").takes_value(true).index(1).required(true))
        .get_matches();

    let target = value_t!(matches, "target", String).unwrap_or_else(|e| e.exit());

    debug!("Connecting to:{:?}", target);
    let mut sock = TcpStream::connect(&*target).expect("connect");

    loop {
        sock.write_all(b"\n").expect("write");
        debug!("Wrote keepalive");
        sleep(Duration::from_secs(1));
    }

}