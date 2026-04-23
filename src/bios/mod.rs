use std::fs;
use std::path::Path;

use crate::error::{Error, Result};

pub const BIOS_SIZE: usize = 512 * 1024;

#[derive(Clone)]
pub struct Bios {
    bytes: Box<[u8; BIOS_SIZE]>,
}

impl Bios {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_bytes(fs::read(path)?)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != BIOS_SIZE {
            return Err(Error::InvalidBiosSize {
                expected: BIOS_SIZE,
                actual: bytes.len(),
            });
        }

        let bytes = bytes
            .into_boxed_slice()
            .try_into()
            .map_err(|boxed: Box<[u8]>| Error::InvalidBiosSize {
                expected: BIOS_SIZE,
                actual: boxed.len(),
            })?;

        Ok(Self { bytes })
    }

    pub fn read8(&self, offset: u32) -> u8 {
        self.bytes[offset as usize % BIOS_SIZE]
    }

    pub fn read16(&self, offset: u32) -> u16 {
        let offset = offset as usize % BIOS_SIZE;
        u16::from_le_bytes([self.bytes[offset], self.bytes[offset + 1]])
    }

    pub fn read32(&self, offset: u32) -> u32 {
        let offset = offset as usize % BIOS_SIZE;
        u32::from_le_bytes([
            self.bytes[offset],
            self.bytes[offset + 1],
            self.bytes[offset + 2],
            self.bytes[offset + 3],
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_size() {
        assert!(matches!(
            Bios::from_bytes(vec![0; 4]),
            Err(Error::InvalidBiosSize {
                expected: BIOS_SIZE,
                actual: 4
            })
        ));
    }
}
