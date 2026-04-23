use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use crate::error::{Error, Result};

const SECTOR_SIZE: usize = 2352;

struct EccEdc {
    ecc_f_lut: [u8; 256],
    ecc_b_lut: [u8; 256],
    edc_lut: [u32; 256],
}

pub fn decode_ecm_file(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<u64> {
    let mut decoder = EcmDecoder::new();
    let input = File::open(input)?;
    let output = File::create(output)?;
    decoder.decode(BufReader::new(input), BufWriter::new(output))
}

struct EcmDecoder {
    tables: EccEdc,
}

impl EcmDecoder {
    fn new() -> Self {
        Self {
            tables: EccEdc::new(),
        }
    }

    fn decode<R: Read, W: Write>(&mut self, mut input: R, mut output: W) -> Result<u64> {
        let mut magic = [0; 4];
        input.read_exact(&mut magic)?;
        if magic != *b"ECM\0" {
            return Err(Error::InvalidEcm("missing ECM header".into()));
        }

        let mut checkedc = 0;
        let mut written = 0;
        let mut sector = [0; SECTOR_SIZE];

        loop {
            let (sector_type, mut count) = read_command(&mut input)?;
            if count == u32::MAX {
                break;
            }
            count += 1;

            if sector_type == 0 {
                let mut remaining = count as usize;
                while remaining > 0 {
                    let chunk_len = remaining.min(SECTOR_SIZE);
                    input.read_exact(&mut sector[..chunk_len])?;
                    checkedc = self.tables.edc_partial(checkedc, &sector[..chunk_len]);
                    output.write_all(&sector[..chunk_len])?;
                    written += chunk_len as u64;
                    remaining -= chunk_len;
                }
                continue;
            }

            for _ in 0..count {
                sector.fill(0);
                sector[1..11].fill(0xff);

                match sector_type {
                    1 => {
                        sector[0x0f] = 0x01;
                        input.read_exact(&mut sector[0x00c..0x00f])?;
                        input.read_exact(&mut sector[0x010..0x810])?;
                        self.tables.generate(&mut sector, 1);
                        checkedc = self.tables.edc_partial(checkedc, &sector);
                        output.write_all(&sector)?;
                        written += SECTOR_SIZE as u64;
                    }
                    2 => {
                        sector[0x0f] = 0x02;
                        input.read_exact(&mut sector[0x014..0x818])?;
                        copy_subheader(&mut sector);
                        self.tables.generate(&mut sector, 2);
                        checkedc = self.tables.edc_partial(checkedc, &sector[0x10..0x930]);
                        output.write_all(&sector[0x10..0x930])?;
                        written += 2336;
                    }
                    3 => {
                        sector[0x0f] = 0x02;
                        input.read_exact(&mut sector[0x014..0x92c])?;
                        copy_subheader(&mut sector);
                        self.tables.generate(&mut sector, 3);
                        checkedc = self.tables.edc_partial(checkedc, &sector[0x10..0x930]);
                        output.write_all(&sector[0x10..0x930])?;
                        written += 2336;
                    }
                    _ => unreachable!(),
                }
            }
        }

        let mut expected = [0; 4];
        input.read_exact(&mut expected)?;
        if checkedc.to_le_bytes() != expected {
            return Err(Error::InvalidEcm(format!(
                "EDC mismatch: calculated {checkedc:08x}, expected {:02x}{:02x}{:02x}{:02x}",
                expected[3], expected[2], expected[1], expected[0]
            )));
        }

        output.flush()?;
        Ok(written)
    }
}

impl EccEdc {
    fn new() -> Self {
        let mut ecc_f_lut = [0; 256];
        let mut ecc_b_lut = [0; 256];
        let mut edc_lut = [0; 256];

        for i in 0..256 {
            let j = ((i << 1) ^ if i & 0x80 != 0 { 0x11d } else { 0 }) as u8;
            ecc_f_lut[i] = j;
            ecc_b_lut[(i as u8 ^ j) as usize] = i as u8;

            let mut edc = i as u32;
            for _ in 0..8 {
                edc = (edc >> 1) ^ if edc & 1 != 0 { 0xd801_8001 } else { 0 };
            }
            edc_lut[i] = edc;
        }

        Self {
            ecc_f_lut,
            ecc_b_lut,
            edc_lut,
        }
    }

    fn edc_partial(&self, mut edc: u32, src: &[u8]) -> u32 {
        for byte in src {
            edc = (edc >> 8) ^ self.edc_lut[((edc ^ u32::from(*byte)) & 0xff) as usize];
        }
        edc
    }

    fn edc_block(&self, src: &[u8], dest: &mut [u8]) {
        let edc = self.edc_partial(0, src);
        dest[..4].copy_from_slice(&edc.to_le_bytes());
    }

    fn generate(&self, sector: &mut [u8; SECTOR_SIZE], sector_type: u8) {
        match sector_type {
            1 => {
                let (src, dest) = sector.split_at_mut(0x810);
                self.edc_block(&src[..0x810], &mut dest[..4]);
                sector[0x814..0x81c].fill(0);
                self.ecc_generate(sector, false);
            }
            2 => {
                let (_, rest) = sector.split_at_mut(0x10);
                let (src, dest) = rest.split_at_mut(0x808);
                self.edc_block(src, &mut dest[..4]);
                self.ecc_generate(sector, true);
            }
            3 => {
                let (_, rest) = sector.split_at_mut(0x10);
                let (src, dest) = rest.split_at_mut(0x91c);
                self.edc_block(src, &mut dest[..4]);
            }
            _ => unreachable!(),
        }
    }

    fn ecc_generate(&self, sector: &mut [u8; SECTOR_SIZE], zero_address: bool) {
        let address = [sector[12], sector[13], sector[14], sector[15]];
        if zero_address {
            sector[12..16].fill(0);
        }

        self.ecc_block(sector, 86, 24, 2, 86, 0x81c);
        self.ecc_block(sector, 52, 43, 86, 88, 0x8c8);

        if zero_address {
            sector[12..16].copy_from_slice(&address);
        }
    }

    fn ecc_block(
        &self,
        sector: &mut [u8; SECTOR_SIZE],
        major_count: usize,
        minor_count: usize,
        major_mult: usize,
        minor_inc: usize,
        dest_offset: usize,
    ) {
        let size = major_count * minor_count;
        for major in 0..major_count {
            let mut index = (major >> 1) * major_mult + (major & 1);
            let mut ecc_a = 0;
            let mut ecc_b = 0;

            for _ in 0..minor_count {
                let temp = sector[0x0c + index];
                index += minor_inc;
                if index >= size {
                    index -= size;
                }
                ecc_a ^= temp;
                ecc_b ^= temp;
                ecc_a = self.ecc_f_lut[ecc_a as usize];
            }

            ecc_a = self.ecc_b_lut[(self.ecc_f_lut[ecc_a as usize] ^ ecc_b) as usize];
            sector[dest_offset + major] = ecc_a;
            sector[dest_offset + major + major_count] = ecc_a ^ ecc_b;
        }
    }
}

fn read_command(input: &mut impl Read) -> Result<(u8, u32)> {
    let mut byte = [0; 1];
    input.read_exact(&mut byte)?;

    let mut c = byte[0];
    let sector_type = c & 3;
    let mut count = u32::from((c >> 2) & 0x1f);
    let mut bits = 5;

    while c & 0x80 != 0 {
        input.read_exact(&mut byte)?;
        c = byte[0];
        count |= u32::from(c & 0x7f) << bits;
        bits += 7;
    }

    Ok((sector_type, count))
}

fn copy_subheader(sector: &mut [u8; SECTOR_SIZE]) {
    sector[0x10] = sector[0x14];
    sector[0x11] = sector[0x15];
    sector[0x12] = sector[0x16];
    sector[0x13] = sector[0x17];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_magic() {
        let mut decoder = EcmDecoder::new();
        let mut output = Vec::new();

        assert!(matches!(
            decoder.decode(&b"bad!"[..], &mut output),
            Err(Error::InvalidEcm(_))
        ));
    }
}
