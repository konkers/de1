use std::io::stdin;

use de1::serial::Frame;
use de1::Packet;
use embassy_executor::Spawner;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    for line in stdin().lines() {
        let line = line.unwrap();
        if line.len() < 3 {
            continue;
        }
        match line.parse::<Packet>() {
            Ok(packet) => println!("{packet:?}"),
            Err(e) => println!("{line} (error {e:?})"),
        }
    }
}
