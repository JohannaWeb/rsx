use std::fs;
use std::path::Path;

use crate::error::{Error, Result};

const HEADER_SIZE: usize = 0x800;

pub struct PsxExe {
    pub initial_pc: u32,
    pub initial_gp: u32,
    pub load_address: u32,
    pub payload_size: u32,
    pub stack_pointer: u32,
    pub stack_size: u32,
    payload: Vec<u8>,
}

impl PsxExe {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_bytes(fs::read(path)?)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() < HEADER_SIZE {
            return Err(Error::InvalidExe(
                "file is smaller than the 0x800-byte header",
            ));
        }

        if &bytes[0..8] != b"PS-X EXE" {
            return Err(Error::InvalidExe("missing PS-X EXE magic"));
        }

        let payload_size = read_u32(&bytes, 0x1c);
        let payload_end = HEADER_SIZE
            .checked_add(payload_size as usize)
            .ok_or(Error::InvalidExe("payload size overflows usize"))?;

        if payload_end > bytes.len() {
            return Err(Error::InvalidExe("payload extends past end of file"));
        }

        Ok(Self {
            initial_pc: read_u32(&bytes, 0x10),
            initial_gp: read_u32(&bytes, 0x14),
            load_address: read_u32(&bytes, 0x18),
            payload_size,
            stack_pointer: read_u32(&bytes, 0x30),
            stack_size: read_u32(&bytes, 0x34),
            payload: bytes[HEADER_SIZE..payload_end].to_vec(),
        })
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_exe() {
        let mut bytes = vec![0; HEADER_SIZE + 4];
        bytes[0..8].copy_from_slice(b"PS-X EXE");
        bytes[0x10..0x14].copy_from_slice(&0x8001_0000_u32.to_le_bytes());
        bytes[0x18..0x1c].copy_from_slice(&0x8001_0000_u32.to_le_bytes());
        bytes[0x1c..0x20].copy_from_slice(&4_u32.to_le_bytes());
        bytes[HEADER_SIZE..HEADER_SIZE + 4].copy_from_slice(&0x1234_5678_u32.to_le_bytes());

        let exe = PsxExe::from_bytes(bytes).unwrap();

        assert_eq!(exe.initial_pc, 0x8001_0000);
        assert_eq!(exe.load_address, 0x8001_0000);
        assert_eq!(exe.payload(), &[0x78, 0x56, 0x34, 0x12]);
    }
}
