use std::io::stdin;

use de1::serial::Frame;
use de1::Packet;

fn main() {
    for line in stdin().lines() {
        let line = line.unwrap();
        if line.len() < 3 {
            continue;
        }
        let (prefix, command) = line.split_at(3);

        match command.parse::<Packet>() {
            Ok(packet) => println!("{prefix}{packet:?}"),
            Err(e) => println!("{prefix}{command} (error {e:?})"),
        }
    }
}
