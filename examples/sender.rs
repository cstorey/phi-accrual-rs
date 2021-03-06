use std::io::prelude::*;
use std::net::TcpStream;
use std::thread::sleep;
use std::time::Duration;

use clap::{value_t, App, Arg};
use log::debug;

fn main() {
    env_logger::init();
    let matches = App::new("sender")
        .arg(
            Arg::with_name("target")
                .takes_value(true)
                .index(1)
                .required(true),
        )
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
