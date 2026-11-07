use std::{fmt, str::FromStr};

pub const HEX: &[u8; 16] = b"0123456789abcdef";

pub fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ObjectId {
    digits: [u8; 64],
    len: u8,
}

#[derive(Debug)]
pub enum InvalidId {
    Length(usize),
    Digit,
}

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length(len) => write!(f, "expected 40 or 64 hex digits, got {len}"),
            Self::Digit => f.write_str("expected lowercase hexadecimal digits (0-9a-f)"),
        }
    }
}

impl ObjectId {
    pub fn parse(text: &[u8]) -> Result<Self, InvalidId> {
        if !matches!(text.len(), 40 | 64) {
            return Err(InvalidId::Length(text.len()));
        }
        let mut digits = [0; 64];
        for (value, &byte) in digits.iter_mut().zip(text) {
            *value = digit(byte).ok_or(InvalidId::Digit)?;
        }
        Ok(Self {
            digits,
            len: text.len() as u8,
        })
    }

    pub fn digits(&self) -> &[u8] {
        &self.digits[..usize::from(self.len)]
    }
}

impl FromStr for ObjectId {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text.as_bytes()).map_err(|error| error.to_string())
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = [0; 64];
        for (byte, &value) in text.iter_mut().zip(self.digits()) {
            *byte = HEX[usize::from(value)];
        }
        f.write_str(std::str::from_utf8(&text[..usize::from(self.len)]).expect("ASCII hex"))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FocusDigits(u16);

impl FocusDigits {
    pub fn contains(self, value: u8) -> bool {
        self.0 & (1 << value) != 0
    }

    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    pub fn mask(self) -> u16 {
        self.0
    }
}

impl FromStr for FocusDigits {
    type Err = &'static str;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut mask = 0;
        for byte in text.bytes() {
            let value = digit(byte).ok_or("expected lowercase hexadecimal digits (0-9a-f)")?;
            mask |= 1 << value;
        }
        Ok(Self(mask))
    }
}

impl fmt::Display for FocusDigits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (value, &byte) in HEX.iter().enumerate() {
            if self.contains(value as u8) {
                write!(f, "{}", char::from(byte))?;
            }
        }
        Ok(())
    }
}
