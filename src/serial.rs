use core::str::FromStr;

use embedded_io_async::Write;
use heapless::{String, Vec};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while_m_n},
    character::complete::{anychar, char},
    combinator::map_res,
    multi::fold_many_m_n,
    IResult,
};

use crate::{Command, Error, Result};

// 2 characters pre byte of data plus 4 for `<[+-]c>` where c is the command
// plus 1 for the newline.
pub const MAX_ENCODED_LENGTH: usize = Command::MAX_DATA_LENGTH * 3 + 5;

#[derive(Debug, Eq, PartialEq)]
pub struct CommandFrame {
    pub command: char,
    pub data: Vec<u8, { Command::MAX_DATA_LENGTH }>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Frame {
    FromDe1(CommandFrame),
    ToDe1(CommandFrame),
    Subscribe(char),
    Unsubscribe(char),
}

fn is_hex_digit(c: char) -> bool {
    c.is_digit(16)
}

fn from_hex(i: &str) -> core::result::Result<u8, core::num::ParseIntError> {
    u8::from_str_radix(i, 16)
}

fn hex_byte(i: &str) -> IResult<&str, u8> {
    map_res(take_while_m_n(2, 2, is_hex_digit), from_hex)(i)
}

fn data(i: &str) -> IResult<&str, Vec<u8, { Command::MAX_DATA_LENGTH }>> {
    fold_many_m_n(
        0,
        Command::MAX_DATA_LENGTH,
        hex_byte,
        Vec::<u8, { Command::MAX_DATA_LENGTH }>::new,
        |mut acc, item| {
            // Ignore errors as we'll never consume more than MAX_DATA_LENGTH
            // bytes due to the bound on fold_many_m_n.
            let _ = acc.push(item);
            acc
        },
    )(i)
}

fn from_de1_packet(i: &str) -> IResult<&str, Frame> {
    let (i, _) = char('[')(i)?;
    let (i, command) = anychar(i)?;
    let (i, _) = char(']')(i)?;
    let (i, data) = data(i)?;

    Ok((i, Frame::FromDe1(CommandFrame { command, data })))
}

fn to_de1_packet(i: &str) -> IResult<&str, Frame> {
    let (i, _) = char('<')(i)?;
    let (i, command) = anychar(i)?;
    let (i, _) = char('>')(i)?;
    let (i, data) = data(i)?;

    Ok((i, Frame::ToDe1(CommandFrame { command, data })))
}

fn subscribe_packet(i: &str) -> IResult<&str, Frame> {
    let (i, _) = tag("<+")(i)?;
    let (i, command) = anychar(i)?;
    let (i, _) = char('>')(i)?;

    Ok((i, Frame::Subscribe(command)))
}

fn unsubscribe_packet(i: &str) -> IResult<&str, Frame> {
    let (i, _) = tag("<-")(i)?;
    let (i, command) = anychar(i)?;
    let (i, _) = char('>')(i)?;

    Ok((i, Frame::Unsubscribe(command)))
}

fn packet(i: &str) -> IResult<&str, Frame> {
    alt((
        from_de1_packet,
        to_de1_packet,
        subscribe_packet,
        unsubscribe_packet,
    ))(i)
}

impl Frame {
    pub async fn write<W: Write>(&self, mut w: W) -> Result<usize> {
        let mut output = String::<MAX_ENCODED_LENGTH>::new();
        match self {
            Frame::FromDe1(f) => {
                output.push('[')?;
                output.push(f.command)?;
                output.push(']')?;
                Self::append_data(&mut output, &f.data)?;
                output.push('\n')?;
            }
            Frame::ToDe1(f) => {
                output.push('<')?;
                output.push(f.command)?;
                output.push('>')?;
                Self::append_data(&mut output, &f.data)?;
                output.push('\n')?;
            }
            Frame::Subscribe(command) => {
                output.push_str("<+")?;
                output.push(*command)?;
                output.push_str(">\n")?;
            }
            Frame::Unsubscribe(command) => {
                output.push_str("<-")?;
                output.push(*command)?;
                output.push_str(">\n")?;
            }
        }

        let data = output.as_bytes();
        w.write_all(&data).await.map_err(|_| Error::IoError)?;

        Ok(data.len())
    }

    fn append_data(s: &mut String<MAX_ENCODED_LENGTH>, data: &[u8]) -> Result<()> {
        for b in data {
            s.push(
                char::from_digit((b >> 4).into(), 16)
                    .ok_or(Error::Unknown)?
                    .to_ascii_uppercase(),
            )?;
            s.push(
                char::from_digit((b & 0xf).into(), 16)
                    .ok_or(Error::Unknown)?
                    .to_ascii_uppercase(),
            )?;
        }

        Ok(())
    }
}

impl FromStr for Frame {
    type Err = Error;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        let (s, packet) = packet(s).map_err(|_| Error::ParseError)?;

        // Unparsed input at end of string is an error.
        if !s.is_empty() {
            return Err(Error::ParseError);
        }

        Ok(packet)
    }
}

pub struct LineReader<const N: usize> {
    buffer: String<N>,
    overflow: bool,
}

impl<const N: usize> LineReader<N> {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            overflow: false,
        }
    }

    pub fn handle_char(&mut self, c: char) -> Result<Option<Frame>> {
        if c == '\n' {
            let frame = if !self.overflow {
                Some(self.buffer.as_str().parse::<Frame>()?)
            } else {
                None
            };
            self.buffer.clear();
            self.overflow = false;
            return Ok(frame);
        }

        // Discard non-ascii bytes
        if !c.is_ascii() {
            return Ok(None);
        }

        self.overflow = self.buffer.push(c).is_err();
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    const STARTUP_LOGS: &[&str] = &[
        include_str!("test_files/brew-log.txt"),
        include_str!("test_files/choose-profile-log.txt"),
        include_str!("test_files/connect-log.txt"),
        include_str!("test_files/startup-log.txt"),
    ];

    #[test]
    fn from_de1_frame_parses() {
        assert_eq!(
            "[M]598E00000000587659591745F55A000000009F".parse::<Frame>(),
            Ok(Frame::FromDe1(CommandFrame {
                command: 'M',
                data: [
                    0x59, 0x8E, 0x00, 0x00, 0x00, 0x00, 0x58, 0x76, 0x59, 0x59, 0x17, 0x45, 0xF5,
                    0x5A, 0x00, 0x00, 0x00, 0x00, 0x9F
                ]
                .iter()
                .cloned()
                .collect()
            }))
        );
    }

    #[test]
    fn to_de1_frame_parses() {
        assert_eq!(
            "<E>598E00000000587659591745F55A000000009F".parse::<Frame>(),
            Ok(Frame::ToDe1(CommandFrame {
                command: 'E',
                data: [
                    0x59, 0x8E, 0x00, 0x00, 0x00, 0x00, 0x58, 0x76, 0x59, 0x59, 0x17, 0x45, 0xF5,
                    0x5A, 0x00, 0x00, 0x00, 0x00, 0x9F
                ]
                .iter()
                .cloned()
                .collect()
            }))
        );
    }

    #[test]
    fn subscribe_frame_parses() {
        assert_eq!("<+E>".parse::<Frame>(), Ok(Frame::Subscribe('E')),);
    }

    #[test]
    fn unsubscribe_frame_parses() {
        assert_eq!("<-E>".parse::<Frame>(), Ok(Frame::Unsubscribe('E')),);
    }

    #[test]
    fn invalid_closing_char_fails() {
        assert_eq!("[M>FF".parse::<Frame>(), Err(Error::ParseError));
        assert_eq!("[M.FF".parse::<Frame>(), Err(Error::ParseError));
    }

    #[test]
    fn extra_input_at_end_of_line_fails() {
        // Command without extra input parses correctly.
        assert_eq!(
            "[M]FF".parse::<Frame>(),
            Ok(Frame::FromDe1(CommandFrame {
                command: 'M',
                data: [255].iter().cloned().collect()
            }))
        );

        // Command with extra input failes to parse.
        assert_eq!("[M]FF.".parse::<Frame>(), Err(Error::ParseError));
    }

    #[futures_test::test]
    async fn decoding_and_reencoding_logs_is_noop() {
        for log in STARTUP_LOGS {
            for line in log.lines() {
                let Ok(frame) = line.parse::<Frame>() else {
                    panic!("Unable to parse line: \"{line}\"");
                };
                let mut output = std::vec::Vec::new();
                if let Err(e) = frame.write(&mut output).await {
                    panic!("Unable to encode line \"{line}\": {e:?}");
                };

                let encoded_frame = core::str::from_utf8(&output).unwrap();
                assert_eq!(
                    // Normalize lines to uppercase.
                    line.to_ascii_uppercase().as_str(),
                    // Strip new line from encodded frame to match line.
                    encoded_frame.trim_end()
                );
            }
        }
    }
}
