use std::fs;
use std::path::Path;

use crate::error::{Error, Result};

const HEADER_SIZE: usize = 0x800;
const EXE_MAGIC: &[u8; 8] = b"PS-X EXE";
const INITIAL_PC_OFFSET: usize = 0x10;
const INITIAL_GP_OFFSET: usize = 0x14;
const LOAD_ADDRESS_OFFSET: usize = 0x18;
const PAYLOAD_SIZE_OFFSET: usize = 0x1c;
const STACK_POINTER_OFFSET: usize = 0x30;
const STACK_SIZE_OFFSET: usize = 0x34;

#[cfg(test)]
const TEST_ENTRY_ADDRESS: u32 = 0x8001_0000;
#[cfg(test)]
const TEST_PAYLOAD_SIZE: u32 = 4;
#[cfg(test)]
const TEST_PAYLOAD_WORD: u32 = 0x1234_5678;

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

        if &bytes[..EXE_MAGIC.len()] != EXE_MAGIC {
            return Err(Error::InvalidExe("missing PS-X EXE magic"));
        }

        let payload_size = read_u32(&bytes, PAYLOAD_SIZE_OFFSET);
        let payload_end = HEADER_SIZE
            .checked_add(payload_size as usize)
            .ok_or(Error::InvalidExe("payload size overflows usize"))?;

        if payload_end > bytes.len() {
            return Err(Error::InvalidExe("payload extends past end of file"));
        }

        Ok(Self {
            initial_pc: read_u32(&bytes, INITIAL_PC_OFFSET),
            initial_gp: read_u32(&bytes, INITIAL_GP_OFFSET),
            load_address: read_u32(&bytes, LOAD_ADDRESS_OFFSET),
            payload_size,
            stack_pointer: read_u32(&bytes, STACK_POINTER_OFFSET),
            stack_size: read_u32(&bytes, STACK_SIZE_OFFSET),
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
        bytes[..EXE_MAGIC.len()].copy_from_slice(EXE_MAGIC);
        bytes[INITIAL_PC_OFFSET..INITIAL_PC_OFFSET + 4]
            .copy_from_slice(&TEST_ENTRY_ADDRESS.to_le_bytes());
        bytes[LOAD_ADDRESS_OFFSET..LOAD_ADDRESS_OFFSET + 4]
            .copy_from_slice(&TEST_ENTRY_ADDRESS.to_le_bytes());
        bytes[PAYLOAD_SIZE_OFFSET..PAYLOAD_SIZE_OFFSET + 4]
            .copy_from_slice(&TEST_PAYLOAD_SIZE.to_le_bytes());
        bytes[HEADER_SIZE..HEADER_SIZE + TEST_PAYLOAD_SIZE as usize]
            .copy_from_slice(&TEST_PAYLOAD_WORD.to_le_bytes());

        let exe = PsxExe::from_bytes(bytes).unwrap();

        assert_eq!(exe.initial_pc, TEST_ENTRY_ADDRESS);
        assert_eq!(exe.load_address, TEST_ENTRY_ADDRESS);
        assert_eq!(exe.payload(), &[0x78, 0x56, 0x34, 0x12]);
    }
}
