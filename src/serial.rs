use core::str::FromStr;

use heapless::Vec;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while_m_n},
    character::complete::{anychar, char},
    combinator::map_res,
    multi::fold_many_m_n,
    IResult,
};

use crate::Error;

const MAX_DATA_LENGTH: usize = 32; // TODO: confirm max data size.

#[derive(Debug, Eq, PartialEq)]
pub struct CommandFrame {
    pub command: char,
    pub data: Vec<u8, MAX_DATA_LENGTH>,
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

fn from_hex(i: &str) -> Result<u8, core::num::ParseIntError> {
    u8::from_str_radix(i, 16)
}

fn hex_byte(i: &str) -> IResult<&str, u8> {
    map_res(take_while_m_n(2, 2, is_hex_digit), from_hex)(i)
}

fn data(i: &str) -> IResult<&str, Vec<u8, MAX_DATA_LENGTH>> {
    fold_many_m_n(
        0,
        MAX_DATA_LENGTH,
        hex_byte,
        Vec::<u8, MAX_DATA_LENGTH>::new,
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

impl FromStr for Frame {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (s, packet) = packet(s).map_err(|_| Error::ParseError)?;

        // Unparsed input at end of string is an error.
        if !s.is_empty() {
            return Err(Error::ParseError);
        }

        Ok(packet)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

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
}
