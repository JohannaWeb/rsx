use std::fmt;
use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidArgument(String),
    InvalidBiosSize { expected: usize, actual: usize },
    InvalidCue(String),
    InvalidEcm(String),
    InvalidExe(&'static str),
    Window(String),
    AddressOutOfRange(u32),
    UnalignedAccess { address: u32, width: usize },
    UnsupportedInstruction { pc: u32, instruction: u32 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::InvalidArgument(message) => write!(f, "{message}"),
            Self::InvalidBiosSize { expected, actual } => {
                write!(
                    f,
                    "invalid BIOS size: expected {expected} bytes, got {actual}"
                )
            }
            Self::InvalidCue(message) => write!(f, "invalid CUE sheet: {message}"),
            Self::InvalidEcm(message) => write!(f, "invalid ECM image: {message}"),
            Self::InvalidExe(message) => write!(f, "invalid PS-X EXE: {message}"),
            Self::Window(message) => write!(f, "window error: {message}"),
            Self::AddressOutOfRange(address) => write!(f, "address out of range: {address:#010x}"),
            Self::UnalignedAccess { address, width } => {
                write!(f, "unaligned {width}-byte access at {address:#010x}")
            }
            Self::UnsupportedInstruction { pc, instruction } => {
                write!(
                    f,
                    "unsupported instruction {instruction:#010x} at pc {pc:#010x}"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
