use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::exe::PsxExe;

pub const RAW_SECTOR_SIZE: usize = 2352;
pub const DATA_SECTOR_SIZE: usize = 2048;
pub const CDROM_INDEX_ADDRESS: u32 = 0x1f80_1800;
pub const CDROM_RESPONSE_ADDRESS: u32 = CDROM_INDEX_ADDRESS + 1;
pub const CDROM_PARAMETER_ADDRESS: u32 = CDROM_INDEX_ADDRESS + 2;
pub const CDROM_INTERRUPT_ADDRESS: u32 = CDROM_INDEX_ADDRESS + 3;

const CDROM_REGISTER_COUNT: u32 = 4;
const ISO_PRIMARY_VOLUME_DESCRIPTOR_SECTOR: usize = 16;
const ISO_VOLUME_DESCRIPTOR_STANDARD_ID: &[u8; 5] = b"CD001";
const ISO_ROOT_DIRECTORY_RECORD_OFFSET: usize = 156;
const ISO_DIRECTORY_FLAG: u8 = 1 << 1;
const ISO_DIRECTORY_RECORD_MIN_SIZE: usize = 34;
const ISO_NAME_LENGTH_OFFSET: usize = 32;
const ISO_NAME_START_OFFSET: usize = 33;
const ISO_EXTENT_OFFSET: usize = 2;
const ISO_SIZE_OFFSET: usize = 10;
const ISO_FLAGS_OFFSET: usize = 25;
const ISO_CURRENT_DIRECTORY_MARKER: &[u8] = &[0];
const ISO_PARENT_DIRECTORY_MARKER: &[u8] = &[1];
const CDROM_INDEX_MASK: u8 = 0x03;
const CDROM_INTERRUPT_MASK: u8 = 0x1f;
const CDROM_CLEAR_PARAMETER_FIFO_BIT: u8 = 0x80;
const CDROM_STATUS_PARAMETER_FIFO_EMPTY_BIT: u8 = 1 << 3;
const CDROM_STATUS_PARAMETER_FIFO_READY_BIT: u8 = 1 << 4;
const CDROM_STATUS_RESPONSE_FIFO_HAS_DATA_BIT: u8 = 1 << 5;
const CDROM_STATUS_DATA_FIFO_HAS_DATA_BIT: u8 = 1 << 6;
const CDROM_INDEX_INTERRUPT_PORT_SELECT_BIT: u8 = 1;
const CDROM_STATUS_STANDBY: u8 = 0x02;
const CDROM_STATUS_READING: u8 = 0x20;
const CDROM_STATUS_ERROR: u8 = 0x01;
const CDROM_IRQ_DATA_READY: u8 = 0x01;
const CDROM_IRQ_COMPLETE: u8 = 0x02;
const CDROM_IRQ_ACK: u8 = 0x03;
const CDROM_ASYNC_DELAY_TICKS: u32 = 50_000;
const CDROM_GET_TN_FIRST_TRACK: u8 = 1;
const CDROM_GET_TN_LAST_TRACK: u8 = 1;
const CDROM_LEAD_OUT_TRACK: u8 = 0;
const CDROM_TEST_VERSION_SUBCOMMAND: u8 = 0x20;
const CDROM_TEST_VERSION_RESPONSE: [u8; 4] = [0x94, 0x09, 0x19, 0xc0];
const CDROM_GET_ID_RESPONSE: [u8; 8] = [
    CDROM_STATUS_STANDBY,
    0x00,
    0x20,
    0x00,
    b'S',
    b'C',
    b'E',
    b'A',
];
const CDROM_MSF_PREGAP_FRAMES: usize = 150;
const CDROM_MSF_SECONDS_PER_MINUTE: usize = 60;
const CDROM_MSF_FRAMES_PER_SECOND: usize = 75;

fn trace_flag(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

#[cfg(test)]
const TEST_ENTRY_ADDRESS: u32 = 0x8001_0000;
#[cfg(test)]
const TEST_GLOBAL_POINTER: u32 = 0x0000_1234;
#[cfg(test)]
const TEST_STACK_POINTER: u32 = 0x801f_ff00;
#[cfg(test)]
const TEST_PAYLOAD_WORD: u32 = 0x1234_5678;
#[cfg(test)]
const TEST_EXE_SECTOR: usize = 22;
#[cfg(test)]
const TEST_ROOT_DIRECTORY_SECTOR: usize = 20;
#[cfg(test)]
const TEST_DEFAULT_SECTOR_COUNT: usize = 23;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackMode {
    Mode1Raw,
    Mode2Raw,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdRomCommand {
    GetStat = 0x01,
    Setloc = 0x02,
    ReadN = 0x06,
    MotorOn = 0x07,
    Stop = 0x08,
    Pause = 0x09,
    Init = 0x0a,
    Mute = 0x0b,
    Demute = 0x0c,
    Setfilter = 0x0d,
    Setmode = 0x0e,
    Getparam = 0x0f,
    GetTN = 0x13,
    GetTD = 0x14,
    SeekL = 0x15,
    SeekP = 0x16,
    Test = 0x19,
    GetID = 0x1a,
    ReadS = 0x1b,
}

pub struct CdImage {
    path: PathBuf,
    mode: TrackMode,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IsoDirectoryRecord {
    name: String,
    extent: usize,
    size: usize,
    is_directory: bool,
}

pub struct CdRomController {
    image: Option<CdImage>,
    index: u8,
    status: u8,
    interrupt_enable: u8,
    interrupt_flag: u8,
    mode: u8,
    trace_reads: bool,
    trace_writes: bool,
    parameter_fifo: Vec<u8>,
    response_fifo: Vec<u8>,
    data_fifo: Vec<u8>,
    pending_lba: usize,
    command_count: u64,
    dma_reads: u64,
    last_command: Option<CdRomCommand>,
    // Queued second response delivered after the current interrupt is acknowledged
    queued_irq: u8,
    queued_response: Vec<u8>,
    queued_data: Vec<u8>,
    tick_counter: u32,
    mirrored_cd_command_count: u64,
}

impl CdRomCommand {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            value if value == Self::GetStat as u8 => Some(Self::GetStat),
            value if value == Self::Setloc as u8 => Some(Self::Setloc),
            value if value == Self::ReadN as u8 => Some(Self::ReadN),
            value if value == Self::MotorOn as u8 => Some(Self::MotorOn),
            value if value == Self::Stop as u8 => Some(Self::Stop),
            value if value == Self::Pause as u8 => Some(Self::Pause),
            value if value == Self::Init as u8 => Some(Self::Init),
            value if value == Self::Mute as u8 => Some(Self::Mute),
            value if value == Self::Demute as u8 => Some(Self::Demute),
            value if value == Self::Setfilter as u8 => Some(Self::Setfilter),
            value if value == Self::Setmode as u8 => Some(Self::Setmode),
            value if value == Self::Getparam as u8 => Some(Self::Getparam),
            value if value == Self::GetTN as u8 => Some(Self::GetTN),
            value if value == Self::GetTD as u8 => Some(Self::GetTD),
            value if value == Self::SeekL as u8 => Some(Self::SeekL),
            value if value == Self::SeekP as u8 => Some(Self::SeekP),
            value if value == Self::Test as u8 => Some(Self::Test),
            value if value == Self::GetID as u8 => Some(Self::GetID),
            value if value == Self::ReadS as u8 => Some(Self::ReadS),
            _ => None,
        }
    }

    pub fn code(self) -> u8 {
        self as u8
    }
}

impl CdImage {
    #[cfg(test)]
    pub fn from_raw_for_test(bytes: Vec<u8>) -> Self {
        Self {
            path: PathBuf::from("test.bin"),
            mode: TrackMode::Mode2Raw,
            bytes,
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("cue") => Self::from_cue(path),
            Some("bin") => Self::from_bin(path, TrackMode::Mode2Raw),
            Some("ecm") => Err(Error::InvalidCue(
                "ECM-compressed images must be decoded to .bin before loading".into(),
            )),
            _ => Err(Error::InvalidCue(
                "expected a .cue sheet or raw .bin image".into(),
            )),
        }
    }

    pub fn from_cue(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let cue = fs::read_to_string(path)?;
        let (bin_name, mode) = parse_cue(&cue)?;
        let bin_path = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(bin_name);
        Self::from_bin(bin_path, mode)
    }

    pub fn from_bin(path: impl AsRef<Path>, mode: TrackMode) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path)?;
        if bytes.len() % RAW_SECTOR_SIZE != 0 {
            return Err(Error::InvalidCue(format!(
                "{} size is not a multiple of {RAW_SECTOR_SIZE} bytes",
                path.display()
            )));
        }

        Ok(Self { path, mode, bytes })
    }

    pub fn sector_count(&self) -> usize {
        self.bytes.len() / RAW_SECTOR_SIZE
    }

    pub fn mode(&self) -> TrackMode {
        self.mode
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read_data_sector(&self, sector: usize) -> Result<&[u8]> {
        if sector >= self.sector_count() {
            return Err(Error::InvalidCue(format!(
                "sector {sector} is out of range"
            )));
        }

        let raw_start = sector * RAW_SECTOR_SIZE;
        let data_start = match self.mode {
            TrackMode::Mode1Raw | TrackMode::Mode2Raw => raw_start + 24,
        };
        Ok(&self.bytes[data_start..data_start + DATA_SECTOR_SIZE])
    }

    pub fn boot_exe(&self) -> Result<PsxExe> {
        let system_cnf = self.read_iso_file("SYSTEM.CNF")?;
        let system_cnf = String::from_utf8_lossy(&system_cnf);
        let boot_path = parse_boot_path(&system_cnf)
            .ok_or_else(|| Error::InvalidCue("SYSTEM.CNF has no BOOT entry".into()))?;
        PsxExe::from_bytes(self.read_iso_file(&boot_path)?)
    }

    pub fn read_iso_file(&self, path: &str) -> Result<Vec<u8>> {
        let root = self.root_directory_record()?;
        let mut current = root;
        let components = path
            .trim_matches(['\\', '/'])
            .split(['\\', '/'])
            .filter(|component| !component.is_empty());

        for component in components {
            let entry = self
                .find_directory_entry(&current, component)?
                .ok_or_else(|| Error::InvalidCue(format!("ISO file not found: {path}")))?;
            current = entry;
        }

        if current.is_directory {
            return Err(Error::InvalidCue(format!(
                "ISO path is a directory: {path}"
            )));
        }

        self.read_extent(current.extent, current.size)
    }

    fn root_directory_record(&self) -> Result<IsoDirectoryRecord> {
        let descriptor = self.read_data_sector(ISO_PRIMARY_VOLUME_DESCRIPTOR_SECTOR)?;
        if descriptor.first() != Some(&1) || &descriptor[1..6] != ISO_VOLUME_DESCRIPTOR_STANDARD_ID
        {
            return Err(Error::InvalidCue(
                "missing ISO9660 primary volume descriptor".into(),
            ));
        }

        parse_directory_record(
            &descriptor[ISO_ROOT_DIRECTORY_RECORD_OFFSET..],
            IsoRecordName::KeepSpecial,
        )
        .ok_or_else(|| Error::InvalidCue("missing ISO9660 root directory record".into()))
    }

    fn find_directory_entry(
        &self,
        directory: &IsoDirectoryRecord,
        name: &str,
    ) -> Result<Option<IsoDirectoryRecord>> {
        if !directory.is_directory {
            return Err(Error::InvalidCue(format!(
                "ISO path component is not a directory: {}",
                directory.name
            )));
        }

        let bytes = self.read_extent(directory.extent, directory.size)?;
        let wanted = normalize_iso_name(name);
        let mut offset = 0;

        while offset < bytes.len() {
            let length = bytes[offset] as usize;
            if length == 0 {
                offset = align_up_usize(offset + 1, DATA_SECTOR_SIZE);
                continue;
            }
            if offset + length > bytes.len() {
                return Err(Error::InvalidCue("truncated ISO directory record".into()));
            }

            if let Some(record) =
                parse_directory_record(&bytes[offset..offset + length], IsoRecordName::Normalize)
                && normalize_iso_name(&record.name) == wanted
            {
                return Ok(Some(record));
            }

            offset += length;
        }

        Ok(None)
    }

    fn read_extent(&self, extent: usize, size: usize) -> Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(size);
        let first_sector = extent;
        let sector_count = size.div_ceil(DATA_SECTOR_SIZE);

        for sector in first_sector..first_sector + sector_count {
            bytes.extend_from_slice(self.read_data_sector(sector)?);
        }

        bytes.truncate(size);
        Ok(bytes)
    }
}

impl CdRomController {
    pub fn new() -> Self {
        Self {
            image: None,
            index: 0,
            status: CDROM_STATUS_STANDBY,
            interrupt_enable: 0,
            interrupt_flag: 0,
            mode: 0,
            trace_reads: trace_flag("PS1_TRACE_CDROM_READS"),
            trace_writes: trace_flag("PS1_TRACE_CDROM_WRITES"),
            parameter_fifo: Vec::new(),
            response_fifo: Vec::new(),
            data_fifo: Vec::new(),
            pending_lba: 0,
            command_count: 0,
            dma_reads: 0,
            last_command: None,
            queued_irq: 0,
            queued_response: Vec::new(),
            queued_data: Vec::new(),
            tick_counter: 0,
            mirrored_cd_command_count: 0,
        }
    }

    pub fn load_image(&mut self, image: CdImage) {
        self.image = Some(image);
        self.status = CDROM_STATUS_STANDBY;
    }

    pub fn image(&self) -> Option<&CdImage> {
        self.image.as_ref()
    }

    pub fn has_interrupt(&self) -> bool {
        self.interrupt_flag & self.interrupt_enable & CDROM_INTERRUPT_MASK != 0
    }

    pub fn has_pending_response(&self) -> bool {
        self.interrupt_flag & CDROM_INTERRUPT_MASK != 0
    }

    pub fn pending_response(&self) -> Option<(u8, u8)> {
        self.has_pending_response().then_some((
            self.interrupt_flag & CDROM_INTERRUPT_MASK,
            self.response_fifo.first().copied().unwrap_or(0),
        ))
    }

    pub fn read8(&mut self, address: u32) -> u8 {
        let value = match address & (CDROM_REGISTER_COUNT - 1) {
            0 => self.status_byte(),
            1 => pop_front(&mut self.response_fifo),
            2 => pop_front(&mut self.data_fifo),
            3 => {
                if self.index & CDROM_INDEX_INTERRUPT_PORT_SELECT_BIT == 0 {
                    self.interrupt_enable
                } else {
                    self.interrupt_flag
                }
            }
            _ => unreachable!(),
        };
        if self.trace_reads {
            eprintln!(
                "cdrom read addr={address:#010x} reg={} index={} value={value:#04x}",
                address & (CDROM_REGISTER_COUNT - 1),
                self.index
            );
        }
        value
    }

    pub fn read_data_byte(&mut self) -> u8 {
        self.dma_reads += 1;
        pop_front(&mut self.data_fifo)
    }

    pub fn command_count(&self) -> u64 {
        self.command_count
    }

    pub fn dma_read_bytes(&self) -> u64 {
        self.dma_reads
    }

    pub fn tick(&mut self, cycles: u32) {
        if self.tick_counter > 0 {
            self.tick_counter = self.tick_counter.saturating_sub(cycles);
            if self.tick_counter == 0 && self.queued_irq != 0 {
                // Only deliver if the previous interrupt was acknowledged
                if self.interrupt_flag == 0 {
                    self.response_fifo.clear();
                    self.response_fifo
                        .extend_from_slice(&self.queued_response.clone());
                    if !self.queued_data.is_empty() {
                        self.data_fifo.clear();
                        self.data_fifo.extend_from_slice(&self.queued_data.clone());
                    }
                    self.interrupt_flag = self.queued_irq;
                    self.queued_irq = 0;
                    self.queued_response.clear();
                    self.queued_data.clear();
                } else {
                    // Try again next tick if still busy
                    self.tick_counter = 1;
                }
            }
        }
    }

    pub fn sync_psyq_state(&mut self, ram: &mut [u8]) {
        if !self.has_pending_response() {
            return;
        }

        if self.mirrored_cd_command_count == self.command_count {
            return;
        }

        const PSYQ_CD_SYNC_FLAG_ADDRESS: u32 = 0x8008_9d9c;
        const PSYQ_CD_REG0: u32 = CDROM_INDEX_ADDRESS;
        const PSYQ_CD_REG1: u32 = CDROM_RESPONSE_ADDRESS;
        const PSYQ_CD_REG2: u32 = CDROM_PARAMETER_ADDRESS;
        const PSYQ_CD_REG3: u32 = CDROM_INTERRUPT_ADDRESS;
        const PSYQ_CD_TABLE_STRIDE_BYTES: usize = 4;
        const PSYQ_CD_TABLE_SIZE_BYTES: usize = 16;
        const PSYQ_CD_BUFFER_POINTER_BACK_OFFSET: usize = 0x28;
        const PSYQ_CD_BUFFER_POINTER_FORWARD_OFFSET: usize = 0x1c;
        const PSYQ_CD_BUFFER_MIN_PHYSICAL_ADDRESS: u32 = 0x0001_0000;
        const PSYQ_CD_BUFFER_STATUS_READY: u8 = 1;
        const RAM_SIZE: usize = 2 * 1024 * 1024;

        // Write sync flag
        let sync_offset = (PSYQ_CD_SYNC_FLAG_ADDRESS as usize) & (RAM_SIZE - 1);
        if sync_offset + 3 < RAM_SIZE {
            ram[sync_offset..sync_offset + 4].copy_from_slice(&1_u32.to_le_bytes());
        }

        let (irq, status) = self.pending_response().unwrap();
        let mut found = false;

        for offset in (0..RAM_SIZE.saturating_sub(PSYQ_CD_TABLE_SIZE_BYTES))
            .step_by(PSYQ_CD_TABLE_STRIDE_BYTES)
        {
            let reg0 = u32::from_le_bytes([
                ram[offset],
                ram[offset + 1],
                ram[offset + 2],
                ram[offset + 3],
            ]);
            let reg1 = u32::from_le_bytes([
                ram[offset + 4],
                ram[offset + 5],
                ram[offset + 6],
                ram[offset + 7],
            ]);
            let reg2 = u32::from_le_bytes([
                ram[offset + 8],
                ram[offset + 9],
                ram[offset + 10],
                ram[offset + 11],
            ]);
            let reg3 = u32::from_le_bytes([
                ram[offset + 12],
                ram[offset + 13],
                ram[offset + 14],
                ram[offset + 15],
            ]);

            if reg0 == PSYQ_CD_REG0
                && reg1 == PSYQ_CD_REG1
                && reg2 == PSYQ_CD_REG2
                && reg3 == PSYQ_CD_REG3
            {
                let mut candidates = Vec::with_capacity(2);
                if offset >= PSYQ_CD_BUFFER_POINTER_BACK_OFFSET {
                    candidates.push(offset - PSYQ_CD_BUFFER_POINTER_BACK_OFFSET);
                }
                candidates.push(offset + PSYQ_CD_BUFFER_POINTER_FORWARD_OFFSET);

                for table_offset in candidates {
                    if table_offset + 4 > RAM_SIZE {
                        continue;
                    }
                    let buffer = u32::from_le_bytes([
                        ram[table_offset],
                        ram[table_offset + 1],
                        ram[table_offset + 2],
                        ram[table_offset + 3],
                    ]);
                    let physical = buffer & 0x1f_ffff;
                    if physical >= PSYQ_CD_BUFFER_MIN_PHYSICAL_ADDRESS
                        && (physical as usize) + 3 < RAM_SIZE
                    {
                        let buffer_offset = physical as usize;
                        ram[buffer_offset] = PSYQ_CD_BUFFER_STATUS_READY;
                        ram[buffer_offset + 1] = status;
                        ram[buffer_offset + 2] = irq;
                    }
                }
                found = true;
            }
        }

        if found {
            self.mirrored_cd_command_count = self.command_count;
        }
    }

    pub fn debug_state(&self) -> CdRomDebugState {
        CdRomDebugState {
            last_command: self.last_command,
            response_len: self.response_fifo.len(),
            data_len: self.data_fifo.len(),
            interrupt_enable: self.interrupt_enable,
            interrupt_flag: self.interrupt_flag,
            status: self.status,
            status_byte: self.status_byte(),
            mode: self.mode,
        }
    }

    pub fn write8(&mut self, address: u32, value: u8) {
        if self.trace_writes {
            eprintln!(
                "cdrom write addr={address:#010x} reg={} index={} value={value:#04x}",
                address & (CDROM_REGISTER_COUNT - 1),
                self.index
            );
        }
        match address & (CDROM_REGISTER_COUNT - 1) {
            0 => self.index = value & CDROM_INDEX_MASK,
            1 => {
                if self.index == 0 {
                    self.execute_command(value);
                }
            }
            2 => match self.index {
                0 => self.parameter_fifo.push(value),
                1 => self.interrupt_enable = value & CDROM_INTERRUPT_MASK,
                _ => {}
            },
            3 => match self.index {
                0 => {
                    if value & CDROM_CLEAR_PARAMETER_FIFO_BIT != 0 {
                        self.parameter_fifo.clear();
                    }
                }
                1 => {
                    self.interrupt_flag &= !(value & CDROM_INTERRUPT_MASK);
                }
                _ => {}
            },
            _ => unreachable!(),
        }
    }

    fn status_byte(&self) -> u8 {
        let mut status = self.index & CDROM_INDEX_MASK;
        if self.parameter_fifo.is_empty() {
            status |= CDROM_STATUS_PARAMETER_FIFO_EMPTY_BIT;
        }
        status |= CDROM_STATUS_PARAMETER_FIFO_READY_BIT;
        if !self.response_fifo.is_empty() {
            status |= CDROM_STATUS_RESPONSE_FIFO_HAS_DATA_BIT;
        }
        if !self.data_fifo.is_empty() {
            status |= CDROM_STATUS_DATA_FIFO_HAS_DATA_BIT;
        }
        status
    }

    fn execute_command(&mut self, command: u8) {
        self.command_count += 1;
        if self.interrupt_enable == 0 {
            self.interrupt_enable = CDROM_INTERRUPT_MASK;
        }

        let Some(command) = CdRomCommand::from_byte(command) else {
            log::warn!("unknown CD-ROM command: {command:#04x}");
            self.last_command = None;
            self.push_response(&[self.status | CDROM_STATUS_ERROR]);
            return;
        };

        log::debug!(
            "CD-ROM command: {command:?} params={:02x?}",
            self.parameter_fifo
        );
        self.last_command = Some(command);
        match command {
            CdRomCommand::GetStat => self.complete_with_status(),
            CdRomCommand::Setloc => self.setloc(),
            CdRomCommand::ReadN | CdRomCommand::ReadS => self.read_sector(),
            // These commands send INT3 (ack) then INT2 (complete)
            CdRomCommand::MotorOn | CdRomCommand::SeekL | CdRomCommand::SeekP => {
                let stat = self.status;
                self.push_ack_then(&[stat], CDROM_IRQ_COMPLETE);
            }
            CdRomCommand::Init => {
                self.status = CDROM_STATUS_STANDBY;
                let stat = self.status;
                self.push_ack_then(&[stat], CDROM_IRQ_COMPLETE);
            }
            CdRomCommand::Stop => {
                self.status = 0x00;
                let stat = self.status;
                self.push_ack_then(&[stat], CDROM_IRQ_COMPLETE);
            }
            CdRomCommand::Pause => {
                self.status &= !CDROM_STATUS_READING;
                let stat = self.status;
                self.push_ack_then(&[stat], CDROM_IRQ_COMPLETE);
            }
            CdRomCommand::Mute | CdRomCommand::Demute => self.complete_with_status(),
            CdRomCommand::Setfilter => {
                self.parameter_fifo.clear();
                self.complete_with_status();
            }
            CdRomCommand::Setmode => {
                if let Some(mode) = self.parameter_fifo.first().copied() {
                    self.mode = mode;
                }
                self.parameter_fifo.clear();
                self.complete_with_status();
            }
            CdRomCommand::Getparam => {
                self.parameter_fifo.clear();
                self.push_response(&[self.status, self.mode, 0, 0, 0]);
            }
            CdRomCommand::GetTN => self.push_response(&[
                self.status,
                CDROM_GET_TN_FIRST_TRACK,
                CDROM_GET_TN_LAST_TRACK,
            ]),
            CdRomCommand::GetTD => self.get_td(),
            CdRomCommand::Test => self.test_command(),
            CdRomCommand::GetID => self.get_id(),
        }
    }

    fn setloc(&mut self) {
        if self.parameter_fifo.len() >= 3 {
            let minute = bcd_to_binary(self.parameter_fifo[0]) as usize;
            let second = bcd_to_binary(self.parameter_fifo[1]) as usize;
            let frame = bcd_to_binary(self.parameter_fifo[2]) as usize;
            self.pending_lba = msf_to_lba(minute, second, frame);
        }
        self.parameter_fifo.clear();
        self.complete_with_status();
    }

    fn read_sector(&mut self) {
        self.data_fifo.clear();
        if let Some(image) = &self.image {
            if let Ok(sector) = image.read_data_sector(self.pending_lba) {
                self.data_fifo.extend_from_slice(sector);
                self.pending_lba += 1;
                self.status = CDROM_STATUS_STANDBY | CDROM_STATUS_READING;
            }
        }
        // INT3 (ack) first, then INT1 (data ready) — data is pre-loaded into data_fifo
        let stat = self.status;
        self.response_fifo.clear();
        self.response_fifo.push(stat);
        self.interrupt_flag = CDROM_IRQ_ACK;
        self.queued_irq = CDROM_IRQ_DATA_READY;
        self.queued_response = vec![stat];
        self.tick_counter = CDROM_ASYNC_DELAY_TICKS;
    }

    fn get_td(&mut self) {
        let track = self.parameter_fifo.first().copied().unwrap_or(0);
        self.parameter_fifo.clear();
        if track == CDROM_LEAD_OUT_TRACK {
            let sectors = self
                .image
                .as_ref()
                .map(CdImage::sector_count)
                .unwrap_or_default();
            let (minute, second, frame) = lba_to_msf(sectors);
            self.push_response(&[
                self.status,
                binary_to_bcd(minute as u8),
                binary_to_bcd(second as u8),
                binary_to_bcd(frame as u8),
            ]);
        } else {
            self.push_response(&[self.status, 0, CDROM_STATUS_STANDBY, 0]);
        }
    }

    fn test_command(&mut self) {
        let subcommand = self.parameter_fifo.first().copied().unwrap_or(0);
        self.parameter_fifo.clear();
        match subcommand {
            CDROM_TEST_VERSION_SUBCOMMAND => self.push_response(&CDROM_TEST_VERSION_RESPONSE),
            _ => self.push_response(&[self.status]),
        }
    }

    fn get_id(&mut self) {
        // INT3 (ack) first with status only, then INT2 with the full 8-byte response
        let stat = self.status;
        self.response_fifo.clear();
        self.response_fifo.push(stat);
        self.interrupt_flag = CDROM_IRQ_ACK;

        self.queued_irq = CDROM_IRQ_COMPLETE;
        self.queued_response = CDROM_GET_ID_RESPONSE.to_vec();
        self.queued_response[0] = stat;
        self.tick_counter = CDROM_ASYNC_DELAY_TICKS;
    }

    fn complete_with_status(&mut self) {
        self.push_ack_then(&[self.status], 0);
    }

    // INT3 ack with [stat], then optionally a second response with irq2/bytes2
    fn push_ack_then(&mut self, second_bytes: &[u8], second_irq: u8) {
        log::debug!("CD-ROM ack INT3, queuing IRQ {second_irq} response: {second_bytes:02x?}");
        self.response_fifo.clear();
        self.response_fifo.push(self.status);
        self.interrupt_flag = CDROM_IRQ_ACK;
        if second_irq != 0 {
            self.queued_irq = second_irq;
            self.queued_response = second_bytes.to_vec();
            self.tick_counter = CDROM_ASYNC_DELAY_TICKS;
        }
    }

    fn push_response(&mut self, bytes: &[u8]) {
        log::debug!(
            "CD-ROM response: {bytes:02x?} (interrupt {:#04x})",
            CDROM_IRQ_ACK
        );
        self.response_fifo.clear();
        self.response_fifo.extend_from_slice(bytes);
        self.interrupt_flag = CDROM_IRQ_ACK;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CdRomDebugState {
    pub last_command: Option<CdRomCommand>,
    pub response_len: usize,
    pub data_len: usize,
    pub interrupt_enable: u8,
    pub interrupt_flag: u8,
    pub status: u8,
    pub status_byte: u8,
    pub mode: u8,
}

impl Default for CdRomController {
    fn default() -> Self {
        Self::new()
    }
}

fn pop_front(fifo: &mut Vec<u8>) -> u8 {
    if fifo.is_empty() { 0 } else { fifo.remove(0) }
}

fn bcd_to_binary(value: u8) -> u8 {
    ((value >> 4) * 10) + (value & 0x0f)
}

fn binary_to_bcd(value: u8) -> u8 {
    ((value / 10) << 4) | (value % 10)
}

fn msf_to_lba(minute: usize, second: usize, frame: usize) -> usize {
    let absolute = minute * CDROM_MSF_SECONDS_PER_MINUTE * CDROM_MSF_FRAMES_PER_SECOND
        + second * CDROM_MSF_FRAMES_PER_SECOND
        + frame;
    absolute.saturating_sub(CDROM_MSF_PREGAP_FRAMES)
}

fn lba_to_msf(lba: usize) -> (usize, usize, usize) {
    let lba = lba + CDROM_MSF_PREGAP_FRAMES;
    let minute = lba / (CDROM_MSF_SECONDS_PER_MINUTE * CDROM_MSF_FRAMES_PER_SECOND);
    let second = (lba / CDROM_MSF_FRAMES_PER_SECOND) % CDROM_MSF_SECONDS_PER_MINUTE;
    let frame = lba % CDROM_MSF_FRAMES_PER_SECOND;
    (minute, second, frame)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IsoRecordName {
    KeepSpecial,
    Normalize,
}

fn parse_directory_record(bytes: &[u8], name_mode: IsoRecordName) -> Option<IsoDirectoryRecord> {
    let length = *bytes.first()? as usize;
    if length == 0 || bytes.len() < length || length < ISO_DIRECTORY_RECORD_MIN_SIZE {
        return None;
    }

    let name_length = bytes[ISO_NAME_LENGTH_OFFSET] as usize;
    let name_start = ISO_NAME_START_OFFSET;
    let name_end = name_start + name_length;
    if name_end > length {
        return None;
    }

    let name = match &bytes[name_start..name_end] {
        ISO_CURRENT_DIRECTORY_MARKER if name_mode == IsoRecordName::KeepSpecial => ".".to_owned(),
        ISO_PARENT_DIRECTORY_MARKER if name_mode == IsoRecordName::KeepSpecial => "..".to_owned(),
        ISO_CURRENT_DIRECTORY_MARKER | ISO_PARENT_DIRECTORY_MARKER => return None,
        raw => String::from_utf8_lossy(raw).into_owned(),
    };

    Some(IsoDirectoryRecord {
        name: normalize_iso_name(&name),
        extent: read_le_u32(bytes, ISO_EXTENT_OFFSET) as usize,
        size: read_le_u32(bytes, ISO_SIZE_OFFSET) as usize,
        is_directory: bytes[ISO_FLAGS_OFFSET] & ISO_DIRECTORY_FLAG != 0,
    })
}

fn normalize_iso_name(name: &str) -> String {
    let name = name.trim_end_matches('.');
    let name = name.split_once(';').map_or(name, |(base, _version)| base);
    name.to_ascii_uppercase()
}

fn parse_boot_path(system_cnf: &str) -> Option<String> {
    for line in system_cnf.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("BOOT") {
            continue;
        }

        let value = value.trim();
        let value = value
            .strip_prefix("cdrom:")
            .or_else(|| value.strip_prefix("CDROM:"))
            .unwrap_or(value)
            .trim_start_matches(['\\', '/']);
        return Some(value.to_owned());
    }

    None
}

fn align_up_usize(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn read_le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn parse_cue(cue: &str) -> Result<(String, TrackMode)> {
    let mut current_file = None;

    for line in cue.lines().map(str::trim) {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("FILE ") {
            current_file = Some(parse_quoted_file(line)?);
        } else if upper.starts_with("TRACK ") {
            let mode = if upper.contains("MODE1/2352") {
                Some(TrackMode::Mode1Raw)
            } else if upper.contains("MODE2/2352") {
                Some(TrackMode::Mode2Raw)
            } else {
                None
            };

            if let Some(mode) = mode {
                let file = current_file
                    .ok_or_else(|| Error::InvalidCue("TRACK entry has no FILE entry".into()))?;
                return Ok((file, mode));
            }
        }
    }

    Err(Error::InvalidCue(
        "missing MODE1/2352 or MODE2/2352 track".into(),
    ))
}

fn parse_quoted_file(line: &str) -> Result<String> {
    let start = line
        .find('"')
        .ok_or_else(|| Error::InvalidCue("FILE entry must quote the binary path".into()))?
        + 1;
    let end = line[start..]
        .find('"')
        .ok_or_else(|| Error::InvalidCue("FILE entry has no closing quote".into()))?
        + start;
    Ok(line[start..end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_cue() {
        let cue = "FILE \"game.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n";

        let (file, mode) = parse_cue(cue).unwrap();

        assert_eq!(file, "game.bin");
        assert_eq!(mode, TrackMode::Mode2Raw);
    }

    #[test]
    fn parses_first_data_track_from_multifile_cue() {
        let cue = "FILE \"data.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"audio.bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n";

        let (file, mode) = parse_cue(cue).unwrap();

        assert_eq!(file, "data.bin");
        assert_eq!(mode, TrackMode::Mode2Raw);
    }

    #[test]
    fn reads_boot_exe_from_iso9660_filesystem() {
        let exe_size = 0x804;
        let mut exe = vec![0; exe_size];
        exe[0..8].copy_from_slice(b"PS-X EXE");
        exe[0x10..0x14].copy_from_slice(&TEST_ENTRY_ADDRESS.to_le_bytes());
        exe[0x14..0x18].copy_from_slice(&TEST_GLOBAL_POINTER.to_le_bytes());
        exe[0x18..0x1c].copy_from_slice(&TEST_ENTRY_ADDRESS.to_le_bytes());
        exe[0x1c..0x20].copy_from_slice(&4_u32.to_le_bytes());
        exe[0x30..0x34].copy_from_slice(&TEST_STACK_POINTER.to_le_bytes());
        exe[0x800..0x804].copy_from_slice(&TEST_PAYLOAD_WORD.to_le_bytes());

        let image = test_iso_image(&[
            TestIsoFile {
                name: "SYSTEM.CNF;1",
                sector: 21,
                bytes: b"BOOT = cdrom:\\SLUS_004.04;1\r\n".to_vec(),
            },
            TestIsoFile {
                name: "SLUS_004.04;1",
                sector: TEST_EXE_SECTOR,
                bytes: exe,
            },
        ]);

        let exe = image.boot_exe().unwrap();

        assert_eq!(exe.initial_pc, TEST_ENTRY_ADDRESS);
        assert_eq!(exe.initial_gp, TEST_GLOBAL_POINTER);
        assert_eq!(exe.stack_pointer, TEST_STACK_POINTER);
        assert_eq!(exe.payload(), &[0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn controller_returns_status_response() {
        let mut cdrom = CdRomController::new();

        cdrom.write8(CDROM_INDEX_ADDRESS, 0);
        cdrom.write8(CDROM_RESPONSE_ADDRESS, CdRomCommand::GetStat.code());

        assert_ne!(
            cdrom.read8(CDROM_INDEX_ADDRESS) & CDROM_STATUS_RESPONSE_FIFO_HAS_DATA_BIT,
            0
        );
        assert_eq!(cdrom.read8(CDROM_RESPONSE_ADDRESS), CDROM_STATUS_STANDBY);
        cdrom.write8(CDROM_INDEX_ADDRESS, 1);
        assert_eq!(cdrom.read8(CDROM_INTERRUPT_ADDRESS), CDROM_IRQ_ACK);
        cdrom.write8(CDROM_INTERRUPT_ADDRESS, CDROM_IRQ_ACK);
        assert_eq!(cdrom.read8(CDROM_INTERRUPT_ADDRESS), 0x00);
    }

    #[test]
    fn controller_reads_data_sector_after_setloc() {
        let mut raw = vec![0; RAW_SECTOR_SIZE * 2];
        raw[24] = 0xaa;
        raw[25] = 0xbb;
        raw[RAW_SECTOR_SIZE + 24] = 0xcc;

        let image = CdImage {
            path: PathBuf::from("test.bin"),
            mode: TrackMode::Mode2Raw,
            bytes: raw,
        };
        let mut cdrom = CdRomController::new();
        cdrom.load_image(image);

        cdrom.write8(CDROM_INDEX_ADDRESS, 0);
        cdrom.write8(CDROM_PARAMETER_ADDRESS, 0x00);
        cdrom.write8(CDROM_PARAMETER_ADDRESS, CDROM_STATUS_STANDBY);
        cdrom.write8(CDROM_PARAMETER_ADDRESS, CDROM_IRQ_DATA_READY);
        cdrom.write8(CDROM_RESPONSE_ADDRESS, CdRomCommand::Setloc.code());
        assert_eq!(cdrom.read8(CDROM_RESPONSE_ADDRESS), CDROM_STATUS_STANDBY);

        cdrom.write8(CDROM_RESPONSE_ADDRESS, CdRomCommand::ReadN.code());
        assert_eq!(
            cdrom.read8(CDROM_RESPONSE_ADDRESS),
            CDROM_STATUS_STANDBY | CDROM_STATUS_READING
        );
        assert_eq!(cdrom.read8(CDROM_PARAMETER_ADDRESS), 0xcc);
    }

    struct TestIsoFile {
        name: &'static str,
        sector: usize,
        bytes: Vec<u8>,
    }

    fn test_iso_image(files: &[TestIsoFile]) -> CdImage {
        let sector_count = files
            .iter()
            .map(|file| file.sector + file.bytes.len().div_ceil(DATA_SECTOR_SIZE))
            .max()
            .unwrap_or(0)
            .max(TEST_DEFAULT_SECTOR_COUNT);
        let mut raw = vec![0; RAW_SECTOR_SIZE * sector_count];
        let mut root_directory = Vec::new();

        push_directory_record(
            &mut root_directory,
            "\0",
            TEST_ROOT_DIRECTORY_SECTOR,
            DATA_SECTOR_SIZE,
            true,
        );
        push_directory_record(
            &mut root_directory,
            "\x01",
            TEST_ROOT_DIRECTORY_SECTOR,
            DATA_SECTOR_SIZE,
            true,
        );
        for file in files {
            push_directory_record(
                &mut root_directory,
                file.name,
                file.sector,
                file.bytes.len(),
                false,
            );
            write_data_extent(&mut raw, file.sector, &file.bytes);
        }

        let mut pvd = vec![0; DATA_SECTOR_SIZE];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        write_directory_record(
            &mut pvd[ISO_ROOT_DIRECTORY_RECORD_OFFSET..],
            "\0",
            TEST_ROOT_DIRECTORY_SECTOR,
            DATA_SECTOR_SIZE,
            true,
        );

        write_data_sector(&mut raw, ISO_PRIMARY_VOLUME_DESCRIPTOR_SECTOR, &pvd);
        write_data_sector(&mut raw, TEST_ROOT_DIRECTORY_SECTOR, &root_directory);

        CdImage {
            path: PathBuf::from("test.iso"),
            mode: TrackMode::Mode2Raw,
            bytes: raw,
        }
    }

    fn write_data_sector(raw: &mut [u8], sector: usize, data: &[u8]) {
        let start = sector * RAW_SECTOR_SIZE + 24;
        raw[start..start + data.len()].copy_from_slice(data);
    }

    fn write_data_extent(raw: &mut [u8], first_sector: usize, data: &[u8]) {
        for (sector_offset, chunk) in data.chunks(DATA_SECTOR_SIZE).enumerate() {
            write_data_sector(raw, first_sector + sector_offset, chunk);
        }
    }

    fn push_directory_record(
        directory: &mut Vec<u8>,
        name: &str,
        sector: usize,
        size: usize,
        is_directory: bool,
    ) {
        let record_start = directory.len();
        let record_len = directory.len() + directory_record_len(name);
        directory.resize(record_len, 0);
        write_directory_record(
            &mut directory[record_start..record_len],
            name,
            sector,
            size,
            is_directory,
        );
    }

    fn write_directory_record(
        record: &mut [u8],
        name: &str,
        sector: usize,
        size: usize,
        is_directory: bool,
    ) {
        let name_bytes = name.as_bytes();
        let record_len = directory_record_len(name);
        record[0] = record_len as u8;
        record[2..6].copy_from_slice(&(sector as u32).to_le_bytes());
        record[6..10].copy_from_slice(&(sector as u32).to_be_bytes());
        record[10..14].copy_from_slice(&(size as u32).to_le_bytes());
        record[14..18].copy_from_slice(&(size as u32).to_be_bytes());
        record[25] = if is_directory { ISO_DIRECTORY_FLAG } else { 0 };
        record[28..30].copy_from_slice(&1_u16.to_le_bytes());
        record[30..32].copy_from_slice(&1_u16.to_be_bytes());
        record[32] = name_bytes.len() as u8;
        record[33..33 + name_bytes.len()].copy_from_slice(name_bytes);
    }

    fn directory_record_len(name: &str) -> usize {
        let len = 33 + name.len();
        if len % 2 == 0 { len } else { len + 1 }
    }
}
