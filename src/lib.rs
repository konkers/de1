#![no_std]

use core::fmt::Debug;
use core::str::FromStr;

use binrw::{
    binrw, // #[binrw] attribute
    io::Cursor,
    BinRead,  // trait for reading
    BinWrite, // trait for writing
};
use fixed::{
    traits::LossyFrom,
    types::{U16F16, U4F12, U4F4, U7F1, U8F24, U8F8},
};

pub mod serial;

pub use serial::{CommandFrame, Frame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    ParseError,
    BinRwError,
    UnknownCommand(char),
}

impl From<binrw::Error> for Error {
    fn from(_value: binrw::Error) -> Self {
        Self::BinRwError
    }
}

type Result<T> = core::result::Result<T, Error>;

fn read_u24(val: [u8; 3]) -> u32 {
    let data = [0u8, val[0], val[1], val[2]];
    u32::from_be_bytes(data)
}

fn write_u24(val: &u32) -> [u8; 3] {
    let data = val.to_be_bytes();
    [data[1], data[2], data[3]]
}

fn read_u8f16(val: [u8; 3]) -> U16F16 {
    let data = [0u8, val[0], val[1], val[2]];
    U16F16::from_bits(u32::from_be_bytes(data))
}

fn write_u8f16(val: &U16F16) -> [u8; 3] {
    let val = val.to_bits();
    let data = val.to_be_bytes();
    [data[1], data[2], data[3]]
}

fn read_f817(val: u8) -> U8F24 {
    let data = [val & 0x7f, 0x0, 0x0, 0x0];
    let fixed = U8F24::from_bits(u32::from_be_bytes(data));
    if val & 0x80 == 0x00 {
        fixed / 10
    } else {
        fixed
    }
}

fn write_f817(val: &U8F24) -> u8 {
    if *val <= U8F24::from_num(12.7) {
        u8::lossy_from(*val * 10)
    } else {
        u8::lossy_from(*val) | 0x80
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Versions,
    RequestedState,
    // SetTime, // Deprecated
    // ShotDirectory, // Deprecated
    ReadFromMmr,
    WriteToMmr,
    // ShotMapRequest, // Deprecated
    // DeleteShotRange, // Deprecated
    FwMapRequest,
    // Temperatures, // Deprecated
    ShotSettings,
    // Deprecated, // Deprecated
    ShotSample,
    StateInfo,
    HeaderWrite,
    FrameWrite,
    WaterLevels,
    Calibration,
}

impl Command {
    pub const fn serial_command(&self) -> char {
        match self {
            Command::Versions => 'A',
            Command::RequestedState => 'B',
            Command::ReadFromMmr => 'E',
            Command::WriteToMmr => 'F',
            Command::FwMapRequest => 'I',
            Command::ShotSettings => 'K',
            Command::ShotSample => 'M',
            Command::StateInfo => 'N',
            Command::HeaderWrite => 'O',
            Command::FrameWrite => 'P',
            Command::WaterLevels => 'Q',
            Command::Calibration => 'R',
        }
    }

    pub const fn gatt_uu8d(&self) -> u16 {
        match self {
            Command::Versions => 0xa001,
            Command::RequestedState => 0xa002,
            Command::ReadFromMmr => 0xa005,
            Command::WriteToMmr => 0xa006,
            Command::FwMapRequest => 0xa009,
            Command::ShotSettings => 0xa00b,
            Command::ShotSample => 0xa00d,
            Command::StateInfo => 0xa00e,
            Command::HeaderWrite => 0xa00f,
            Command::FrameWrite => 0xa010,
            Command::WaterLevels => 0xa011,
            Command::Calibration => 0xa012,
        }
    }

    pub const fn data_len(&self) -> usize {
        match self {
            Command::Versions => 18,
            Command::RequestedState => 1,
            Command::ReadFromMmr => 20,
            Command::WriteToMmr => 20,
            Command::FwMapRequest => 7,
            Command::ShotSettings => 10,
            Command::ShotSample => 19,
            Command::StateInfo => 2,
            Command::HeaderWrite => 5,
            Command::FrameWrite => 8,
            Command::WaterLevels => 4,
            Command::Calibration => 14,
        }
    }
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[br(repr = u8)]
#[bw(repr = u8)]
enum State {
    Sleep = 0x00,
    GoingToSleep = 0x01,
    Idle = 0x02,
    Busy = 0x03,
    Espresso = 0x04,
    Steam = 0x05,
    HotWater = 0x06,
    ShortCal = 0x07,
    SelfTest = 0x08,
    LongCal = 0x09,
    Descale = 0x0a,
    FatalError = 0x0b,
    Init = 0x0c,
    NoRequest = 0x0d,
    SkipToNext = 0x0e,
    HotWaterRinse = 0x0f,
    SteamRinse = 0x10,
    Refill = 0x11,
    Clean = 0x12,
    InBootloader = 0x13,
    AirPurge = 0x14,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
#[br(repr = u8)]
#[bw(repr = u8)]
enum SubState {
    NoState = 0x00,
    HeatingWaterTank = 0x01,
    HeatingWaterHeater = 0x02,
    StabilizingMixTemp = 0x03,
    PreInfusion = 0x04,
    Pouring = 0x05,
    Flushing = 0x06,
    Steaming = 0x07,
    DescaleInit = 0x08,
    DescaleFillGroup = 0x09,
    DescaleReturn = 0x0a,
    DescaleGroup = 0x0b,
    DescaleSteam = 0x0c,
    CleanInit = 0x0d,
    CleanFillGroup = 0x0e,
    CleanSoak = 0x0f,
    CleanGroup = 0x10,
    PausedRefil = 0x11,
    PausedSteam = 0x12,
    ErrorNaN = 200,
    ErrorInf = 201,
    ErrorGeneric = 202,
    ErrorAcc = 203,
    ErrorTempSensor = 204,
    ErrorPressureSensor = 205,
    ErrorWaterLevelSensor = 206,
    ErrorDip = 207,
    ErrorAssertion = 208,
    ErrorUnsafe = 209,
    ErrorInvalidParam = 210,
    ErrorFlash = 211,
    ErrorOOM = 212,
    ErrorDeadline = 213,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Version {
    api_version: u8,
    release: u8,
    commits: u16,
    changes: u8,
    sha: u32,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Versions {
    bluetooth: Version,
    firmware: Version,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestedState {
    state: State,
}

pub struct SetTime {}
pub struct ShotDirectory {}

#[binrw]
#[brw(big)]
#[derive(Clone, Eq, Debug, PartialEq)]
pub struct MmrOpperation {
    len: u8,
    #[br(map = read_u24)]
    #[bw(map = write_u24)]
    addr: u32,
    data: [u8; 16],
}

// impl Debug for MmrOpperation {
//     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//         f.write_fmt(format_args!(
//             "MmrOperation {{ len: {} /* {} bytes */, addr: 0x{:x}, data: {:x?}}}",
//             self.len,
//             (self.len + 1) * 4,
//             self.addr,
//             self.data,
//         ))
//     }
// }

pub struct FwMapRequest {}
pub struct Temperatures {}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShotSettings {
    steam_flags: u8,
    target_steam_temp: u8,
    target_steam_length: u8,
    target_hot_water_temp: u8,
    target_hot_water_volume: u8,
    target_hot_water_length: u8,
    target_espresso_volume: u8,

    #[br(map = |val: u16| U8F8::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    target_group_temp: U8F8,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShotSample {
    timer: u16,

    #[br(map = |val: u16| U4F12::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    group_pressure: U4F12,

    #[br(map = |val: u16| U4F12::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    group_flow: U4F12,

    #[br(map = |val: u16| U8F8::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    mix_temp: U8F8,

    #[br(map = read_u8f16)]
    #[bw(map = write_u8f16)]
    head_temp: U16F16,

    #[br(map = |val: u16| U8F8::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    set_mix_temp: U8F8,

    #[br(map = |val: u16| U8F8::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    set_head_temp: U8F8,

    #[br(map = |val: u8| U4F4::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    set_group_pressure: U4F4,

    #[br(map = |val: u8| U4F4::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    set_group_flow: U4F4,

    frame_number: u8,
    steam_temp: u8,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateInfo {
    state: State,
    sub_state: SubState,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShotHeaderWrite {
    version: u8,
    frames: u8,
    preinfuse_frames: u8,

    #[br(map = |val: u8| U4F4::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    minimum_pressure: U4F4,

    #[br(map = |val: u8| U4F4::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    minimum_flow: U4F4,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShotFrameWrite {
    index: u8,
    flags: u8,

    #[br(map = |val: u8| U4F4::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    set_value: U4F4,

    #[br(map = |val: u8| U7F1::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    temp: U7F1,

    #[br(map = read_f817)]
    #[bw(map = write_f817)]
    frame_lenght: U8F24,

    #[br(map = |val: u8| U4F4::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    trigger_value: U4F4,

    #[br(map = |val: u16| val & 0x3ff)]
    #[bw(map = |val| val & 0x3ff)]
    max_volume: u16,
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaterLevels {
    #[br(map = |val: u16| U8F8::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    level: U8F8,

    #[br(map = |val: u16| U8F8::from_bits(val))]
    #[bw(map = |val| val.to_bits())]
    start_fill_level: U8F8,
}

pub struct Calibration {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Packet {
    RequestedState(RequestedState),
    ReadFromMmr(MmrOpperation),
    WriteToMmr(MmrOpperation),
    ShotSettings(ShotSettings),
    ShotSample(ShotSample),
    StateInfo(StateInfo),
    ShotHeaderWrite(ShotHeaderWrite),
    ShotFrameWrite(ShotFrameWrite),
    WaterLevels(WaterLevels),
    Subscribe(char),
    Unsubscribe(char),
}

impl Packet {
    fn from_command(command: &CommandFrame) -> Result<Self> {
        match command.command {
            'B' => {
                let state = RequestedState::read(&mut Cursor::new(&command.data))?;
                Ok(Self::RequestedState(state))
            }
            'E' => {
                let mmr_op = MmrOpperation::read(&mut Cursor::new(&command.data))?;
                Ok(Self::ReadFromMmr(mmr_op))
            }
            'F' => {
                let mmr_op = MmrOpperation::read(&mut Cursor::new(&command.data))?;
                Ok(Self::WriteToMmr(mmr_op))
            }
            'K' => {
                let shot_settings = ShotSettings::read(&mut Cursor::new(&command.data))?;
                Ok(Self::ShotSettings(shot_settings))
            }
            'M' => {
                let shot_sample = ShotSample::read(&mut Cursor::new(&command.data))?;
                Ok(Self::ShotSample(shot_sample))
            }
            'N' => {
                let state_info = StateInfo::read(&mut Cursor::new(&command.data))?;
                Ok(Self::StateInfo(state_info))
            }
            'O' => {
                let shot_header_write = ShotHeaderWrite::read(&mut Cursor::new(&command.data))?;
                Ok(Self::ShotHeaderWrite(shot_header_write))
            }
            'P' => {
                let shot_frame_write = ShotFrameWrite::read(&mut Cursor::new(&command.data))?;
                Ok(Self::ShotFrameWrite(shot_frame_write))
            }
            'Q' => {
                let water_levels = WaterLevels::read(&mut Cursor::new(&command.data))?;
                Ok(Self::WaterLevels(water_levels))
            }
            _ => Err(Error::UnknownCommand(command.command)),
        }
    }
}

impl FromStr for Packet {
    type Err = Error;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        let frame = s.parse::<Frame>()?;
        match frame {
            Frame::FromDe1(command) | Frame::ToDe1(command) => Self::from_command(&command),
            Frame::Subscribe(command) => Ok(Self::Subscribe(command)),
            Frame::Unsubscribe(command) => Ok(Self::Unsubscribe(command)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let packet = "[M]5F380000000058DA59C2E645F55A00000000A0"
            .parse::<Packet>()
            .unwrap();
        println!("{packet:?}");
        assert!(false);
    }
}
