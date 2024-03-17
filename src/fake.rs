use core::num::Wrapping;

use binrw::{io::Cursor, meta::WriteEndian, BinWrite};
use embassy_futures::select::{select, Either};
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    pipe::{self},
};
use embassy_time::{Duration, Instant, Timer};
use fixed::types::{U16F16, U4F12, U4F4, U8F8};
use heapless::Vec;
use log::{error, info};

use crate::{
    serial::{Frame, LineReader},
    Command, CommandFrame, Error, MmrOpperation, Packet, RequestedState, Result, ShotFrameWrite,
    ShotHeaderWrite, ShotSample, ShotSettings, State, StateInfo, SubState, WaterLevels,
};

const TICK_PERIOD: Duration = Duration::from_secs(1);
#[derive(Default)]
struct Subscriptions {
    mmr_read: bool,
    shot_sample: bool,
    state_info: bool,
    water_levels: bool,
}

pub struct De1<'rx, 'tx> {
    rx_pipe: pipe::Reader<'rx, NoopRawMutex, 256>,
    tx_pipe: pipe::Writer<'tx, NoopRawMutex, 256>,
    line_reader: LineReader<64>,
    subscriptions: Subscriptions,
    timestamp: Wrapping<u16>,
}

impl<'rx, 'tx> De1<'rx, 'tx> {
    pub fn new(
        rx_pipe: pipe::Reader<'rx, NoopRawMutex, 256>,
        tx_pipe: pipe::Writer<'tx, NoopRawMutex, 256>,
    ) -> Self {
        Self {
            rx_pipe,
            tx_pipe,
            line_reader: LineReader::new(),
            subscriptions: Default::default(),
            timestamp: Wrapping(0),
        }
    }

    pub async fn run(&mut self) -> ! {
        let mut last_tick = Instant::now();
        let mut buf = [0u8, 64];
        loop {
            let tick_target = last_tick + TICK_PERIOD;

            let either = select(self.rx_pipe.read(&mut buf), Timer::at(tick_target)).await;

            match either {
                Either::First(read_len) => self.handle_read(&buf[..read_len]).await,
                Either::Second(_) => {
                    last_tick = tick_target;
                    let _ = self.handle_tick().await;
                }
            }
        }
    }

    async fn handle_requested_state(&mut self, value: RequestedState) -> Result<()> {
        Ok(())
    }

    async fn send_command_packet<T: BinWrite>(&mut self, command: Command, data: &T) -> Result<()>
    where
        T: WriteEndian,
        for<'a> <T as BinWrite>::Args<'a>: Default,
    {
        let mut buf = [0u8; Command::MAX_DATA_LENGTH];
        data.write(&mut Cursor::new(&mut buf[..]))?;
        let frame = Frame::FromDe1(CommandFrame {
            command: command.serial_command(),
            data: Vec::from_slice(&buf[0..command.data_len()])?,
        });

        frame.write(self.tx_pipe).await?;

        Ok(())
    }

    async fn send_mmr(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        if data.len() > 16 {
            return Err(Error::Unknown);
        }

        let mut op = MmrOpperation {
            len: data.len() as u8,
            addr: addr,
            data: [0u8; 16],
        };
        op.data[..data.len()].copy_from_slice(data);
        self.send_command_packet(Command::ReadFromMmr, &op).await
    }

    async fn handle_read_from_mmr(&mut self, value: MmrOpperation) -> Result<()> {
        // MMR read's len is defined differently than its repsonses and mmr writes.
        let _op_len = (value.len as usize + 1) * 4;

        // Don't send a response if subscriptsion are not enabled.
        if !self.subscriptions.mmr_read {
            return Ok(());
        }

        match value.addr {
            0x800008 => {
                self.send_mmr(
                    value.addr,
                    &[
                        0x14, 0x05, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x35, 0x05, 0x00, 0x00,
                    ],
                )
                .await
            }
            0x803810 => {
                self.send_mmr(
                    value.addr,
                    &[
                        0x14, 0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x52, 0x03, 0x00, 0x00,
                    ],
                )
                .await
            }
            0x80381c => self.send_mmr(value.addr, &[0x07, 0x00, 0x00, 0x00]).await,
            0x803830 => self.send_mmr(value.addr, &[0x84, 0x23, 0x00, 0x00]).await,
            0x803834 => {
                self.send_mmr(
                    value.addr,
                    &[0x78, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00],
                )
                .await
            }
            0x80385c => self.send_mmr(value.addr, &[0x02, 0x00, 0x00, 0x00]).await,
            _ => Err(Error::UnsupportedMmr(value.addr)),
        }
    }

    async fn handle_write_to_mmr(&mut self, value: MmrOpperation) -> Result<()> {
        Ok(())
    }

    async fn handle_shot_settings(&mut self, value: ShotSettings) -> Result<()> {
        Ok(())
    }

    async fn handle_shot_sample(&mut self, value: ShotSample) -> Result<()> {
        Ok(())
    }

    async fn handle_state_info(&mut self, value: StateInfo) -> Result<()> {
        Ok(())
    }

    async fn handle_shot_header_write(&mut self, value: ShotHeaderWrite) -> Result<()> {
        Ok(())
    }

    async fn handle_shot_frame_write(&mut self, value: ShotFrameWrite) -> Result<()> {
        Ok(())
    }

    async fn handle_water_levels(&mut self, value: WaterLevels) -> Result<()> {
        Ok(())
    }

    async fn handle_subscription(&mut self, command: Command, enable: bool) -> Result<()> {
        info!("FAKE: subscription {:?} {}", command, enable);
        match command {
            Command::ReadFromMmr => self.subscriptions.mmr_read = enable,
            Command::ShotSample => self.subscriptions.shot_sample = enable,
            Command::StateInfo => self.subscriptions.state_info = enable,
            Command::WaterLevels => self.subscriptions.water_levels = enable,
            _ => (),
        }

        Ok(())
    }

    async fn handle_frame(&mut self, frame: Frame) -> Result<()> {
        // Reject packets that should be from us.
        if let Frame::FromDe1(_) = frame {
            return Err(Error::UnexpectedFrame);
        }

        let packet: Packet = frame.try_into()?;

        match packet {
            Packet::RequestedState(val) => self.handle_requested_state(val).await?,
            Packet::ReadFromMmr(val) => self.handle_read_from_mmr(val).await?,
            Packet::WriteToMmr(val) => self.handle_write_to_mmr(val).await?,
            Packet::ShotSettings(val) => self.handle_shot_settings(val).await?,
            Packet::ShotSample(val) => self.handle_shot_sample(val).await?,
            Packet::StateInfo(val) => self.handle_state_info(val).await?,
            Packet::ShotHeaderWrite(val) => self.handle_shot_header_write(val).await?,
            Packet::ShotFrameWrite(val) => self.handle_shot_frame_write(val).await?,
            Packet::WaterLevels(val) => self.handle_water_levels(val).await?,
            Packet::Subscribe(c) => self.handle_subscription(c, true).await?,
            Packet::Unsubscribe(c) => self.handle_subscription(c, false).await?,
        }

        Ok(())
    }

    async fn handle_char(&mut self, c: char) -> Result<()> {
        let Some(frame) = self.line_reader.handle_char(c)? else {
            return Ok(());
        };
        self.handle_frame(frame).await
    }

    async fn handle_read(&mut self, data: &[u8]) {
        for c in data.iter().map(|b| *b as char) {
            if let Err(e) = self.handle_char(c).await {
                error!("FAKE: error handling char '{c}': {e:?}");
            }
        }
    }

    async fn send_shot_sample(&mut self) -> Result<()> {
        self.timestamp += 25;
        let sample = ShotSample {
            timer: self.timestamp.0,
            group_pressure: U4F12::from_num(0.0103),
            group_flow: U4F12::from_num(1.8708),
            mix_temp: U8F8::from_num(77.91),
            head_temp: U16F16::from_num(85.79803),
            set_mix_temp: U8F8::from_num(90),
            set_head_temp: U8F8::from_num(90),
            set_group_pressure: U4F4::from_num(0),
            set_group_flow: U4F4::from_num(0),
            frame_number: 5,
            steam_temp: 158,
        };
        self.send_command_packet(Command::ShotSample, &sample).await
    }

    async fn send_state_info(&mut self) -> Result<()> {
        let info = StateInfo {
            state: State::Idle,
            sub_state: SubState::NoState,
        };
        self.send_command_packet(Command::StateInfo, &info).await
    }

    async fn send_water_levels(&mut self) -> Result<()> {
        let levels = WaterLevels {
            level: U8F8::from_num(13.06),
            start_fill_level: U8F8::from_num(5),
        };
        self.send_command_packet(Command::WaterLevels, &levels)
            .await
    }

    async fn handle_tick(&mut self) -> Result<()> {
        info!("FAKE: tick");
        if self.subscriptions.shot_sample {
            self.send_shot_sample().await?
        }

        if self.subscriptions.state_info {
            self.send_state_info().await?
        }

        if self.subscriptions.water_levels {
            self.send_water_levels().await?
        }

        Ok(())
    }
}
