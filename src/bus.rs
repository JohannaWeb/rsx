use crate::bios::Bios;
use crate::cdrom::{CdImage, CdRomController, CdRomDebugState};
use crate::dma::{DmaChannel, DmaController, DmaDirection, DmaStep, DmaSyncMode, DmaTransfer};
use crate::error::{Error, Result};
use crate::gpu::{Gpu, GpuDebugState};

pub const RAM_SIZE: usize = 2 * 1024 * 1024;
const SCRATCHPAD_SIZE: usize = 1024;

const RAM_BASE: u32 = 0x0000_0000;
const RAM_END: u32 = 0x0080_0000;
const EXPANSION_1_BASE: u32 = 0x1f00_0000;
const EXPANSION_1_END: u32 = 0x1f80_0000;
const SCRATCHPAD_BASE: u32 = 0x1f80_0000;
const SCRATCHPAD_END: u32 = SCRATCHPAD_BASE + SCRATCHPAD_SIZE as u32;
const IO_BASE: u32 = 0x1f80_1000;
const IO_END: u32 = 0x1f80_3000;
const ROOT_COUNTER_BASE_OFFSET: usize = 0x100;
const ROOT_COUNTER_STRIDE: usize = 0x10;
const ROOT_COUNTER_COUNT: usize = 3;
const DMA_BASE: u32 = 0x1f80_1080;
const DMA_END: u32 = 0x1f80_1100;
const CDROM_BASE: u32 = 0x1f80_1800;
const CDROM_END: u32 = CDROM_BASE + 4;
const GPU_GP0: u32 = 0x1f80_1810;
const GPU_GP1: u32 = 0x1f80_1814;
const GPU_END: u32 = GPU_GP1 + 4;
const INTERRUPT_STATUS_OFFSET: usize = 0x70;
const INTERRUPT_MASK_OFFSET: usize = 0x74;
const VBLANK_INTERRUPT_BIT: u32 = 1;
const CDROM_INTERRUPT_BIT: u32 = 1 << 2;
pub(crate) const VBLANK_INTERVAL_TICKS: u32 = 33_868;
const PSYQ_CD_SYNC_FLAG_ADDRESS: u32 = 0x8008_9d9c;
const CACHE_CONTROL: u32 = 0xfffe_0130;
const CACHE_CONTROL_END: u32 = CACHE_CONTROL + 4;
const BIOS_BASE: u32 = 0x1fc0_0000;
const BIOS_END: u32 = BIOS_BASE + crate::bios::BIOS_SIZE as u32;

pub struct Bus {
    ram: Box<[u8; RAM_SIZE]>,
    scratchpad: [u8; SCRATCHPAD_SIZE],
    io: Box<[u8; (IO_END - IO_BASE) as usize]>,
    root_counters: [u16; ROOT_COUNTER_COUNT],
    vblank_ticks: u32,
    cdrom: CdRomController,
    dma: DmaController,
    gpu: Gpu,
    cache_control: u32,
    bios: Bios,
}

impl Bus {
    pub fn new(bios: Bios) -> Self {
        let ram = vec![0; RAM_SIZE]
            .into_boxed_slice()
            .try_into()
            .expect("RAM allocation must match RAM_SIZE");

        Self {
            ram,
            scratchpad: [0; SCRATCHPAD_SIZE],
            io: Box::new([0; (IO_END - IO_BASE) as usize]),
            root_counters: [0; ROOT_COUNTER_COUNT],
            vblank_ticks: 0,
            cdrom: CdRomController::new(),
            dma: DmaController::new(),
            gpu: Gpu::new(),
            cache_control: 0,
            bios,
        }
    }

    pub fn load_cd_image(&mut self, image: CdImage) {
        self.cdrom.load_image(image);
    }

    pub fn cd_image(&self) -> Option<&CdImage> {
        self.cdrom.image()
    }

    pub fn cdrom_command_count(&self) -> u64 {
        self.cdrom.command_count()
    }

    pub fn cdrom_dma_read_bytes(&self) -> u64 {
        self.cdrom.dma_read_bytes()
    }

    pub fn cdrom_debug_state(&self) -> CdRomDebugState {
        self.cdrom.debug_state()
    }

    pub fn framebuffer_rgb(&self) -> Vec<u8> {
        self.gpu.framebuffer_rgb()
    }

    pub fn display_width(&self) -> usize {
        self.gpu.display_width()
    }

    pub fn display_height(&self) -> usize {
        self.gpu.display_height()
    }

    pub fn gpu_debug_state(&self) -> GpuDebugState {
        self.gpu.debug_state()
    }

    pub fn interrupt_pending(&self) -> bool {
        self.interrupt_pending_bits() != 0
    }

    pub fn interrupt_pending_bits(&self) -> u32 {
        let status = self.io_word(INTERRUPT_STATUS_OFFSET);
        let mask = self.io_word(INTERRUPT_MASK_OFFSET);
        status & mask
    }

    pub fn tick(&mut self) {
        self.tick_root_counters();
        self.tick_vblank();
        self.sync_cdrom_interrupt();
    }

    pub fn load_ram(&mut self, address: u32, bytes: &[u8]) -> Result<()> {
        for (index, byte) in bytes.iter().copied().enumerate() {
            self.write8(address.wrapping_add(index as u32), byte)?;
        }
        Ok(())
    }

    pub fn read8(&mut self, address: u32) -> Result<u8> {
        let physical = mask_region(address);
        match physical {
            RAM_BASE..RAM_END => Ok(self.ram[ram_offset(physical)]),
            EXPANSION_1_BASE..EXPANSION_1_END => Ok(0xff),
            SCRATCHPAD_BASE..SCRATCHPAD_END => {
                Ok(self.scratchpad[(physical - SCRATCHPAD_BASE) as usize])
            }
            CDROM_BASE..CDROM_END => Ok(self.cdrom.read8(physical)),
            DMA_BASE..DMA_END => {
                let offset = physical - DMA_BASE;
                Ok(self.dma.read32(offset & !3).to_le_bytes()[(offset & 3) as usize])
            }
            GPU_GP1..GPU_END => {
                Ok(self.gpu.read_status().to_le_bytes()[(physical - GPU_GP1) as usize])
            }
            IO_BASE..IO_END => Ok(self.io[(physical - IO_BASE) as usize]),
            CACHE_CONTROL..CACHE_CONTROL_END => {
                Ok(self.cache_control.to_le_bytes()[(physical - CACHE_CONTROL) as usize])
            }
            BIOS_BASE..BIOS_END => Ok(self.bios.read8(physical - BIOS_BASE)),
            _ => {
                log::warn!("unhandled address at {address:#010x} (physical {physical:#010x})");
                Err(Error::AddressOutOfRange(address))
            }
        }
    }

    pub fn read16(&mut self, address: u32) -> Result<u16> {
        require_aligned(address, 2)?;
        Ok(u16::from_le_bytes([
            self.read8(address)?,
            self.read8(address.wrapping_add(1))?,
        ]))
    }

    pub fn read32(&mut self, address: u32) -> Result<u32> {
        require_aligned(address, 4)?;
        Ok(u32::from_le_bytes([
            self.read8(address)?,
            self.read8(address.wrapping_add(1))?,
            self.read8(address.wrapping_add(2))?,
            self.read8(address.wrapping_add(3))?,
        ]))
    }

    pub fn peek32(&self, address: u32) -> Result<u32> {
        require_aligned(address, 4)?;
        Ok(u32::from_le_bytes([
            self.peek8(address)?,
            self.peek8(address.wrapping_add(1))?,
            self.peek8(address.wrapping_add(2))?,
            self.peek8(address.wrapping_add(3))?,
        ]))
    }

    pub fn write8(&mut self, address: u32, value: u8) -> Result<()> {
        let physical = mask_region(address);
        match physical {
            RAM_BASE..RAM_END => {
                let offset = ram_offset(physical);
                self.ram[offset] = value;
                Ok(())
            }
            EXPANSION_1_BASE..EXPANSION_1_END => Ok(()),
            SCRATCHPAD_BASE..SCRATCHPAD_END => {
                self.scratchpad[(physical - SCRATCHPAD_BASE) as usize] = value;
                Ok(())
            }
            CDROM_BASE..CDROM_END => {
                self.cdrom.write8(physical, value);
                self.sync_cdrom_interrupt();
                Ok(())
            }
            DMA_BASE..DMA_END => {
                self.write_dma8(physical, value);
                Ok(())
            }
            GPU_GP0..GPU_END => {
                self.write_gpu8(physical, value);
                Ok(())
            }
            IO_BASE..IO_END => {
                self.write_io8(physical, value);
                Ok(())
            }
            CACHE_CONTROL..CACHE_CONTROL_END => {
                let mut bytes = self.cache_control.to_le_bytes();
                bytes[(physical - CACHE_CONTROL) as usize] = value;
                self.cache_control = u32::from_le_bytes(bytes);
                Ok(())
            }
            BIOS_BASE..BIOS_END => {
                log::warn!("illegal write to BIOS at {address:#010x}");
                Err(Error::AddressOutOfRange(address))
            }
            _ => {
                log::warn!("unhandled write8 at address {address:#010x} (physical {physical:#010x})");
                Err(Error::AddressOutOfRange(address))
            }
        }
    }

    pub fn write16(&mut self, address: u32, value: u16) -> Result<()> {
        require_aligned(address, 2)?;
        for (index, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(index as u32), byte)?;
        }
        Ok(())
    }

    pub fn write32(&mut self, address: u32, value: u32) -> Result<()> {
        let physical = mask_region(address);
        if physical == GPU_GP0 {
            self.gpu.write_gp0(value);
            return Ok(());
        }
        if physical == GPU_GP1 {
            self.gpu.write_gp1(value);
            return Ok(());
        }
        if (DMA_BASE..DMA_END).contains(&physical) {
            if let Some(transfer) = self.dma.write32(physical - DMA_BASE, value) {
                self.execute_dma_transfer(transfer)?;
            }
            return Ok(());
        }

        require_aligned(address, 4)?;
        for (index, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(index as u32), byte)?;
        }
        Ok(())
    }

    fn write_dma8(&mut self, address: u32, value: u8) {
        let offset = address - DMA_BASE;
        let aligned = offset & !3;
        let byte_index = (offset & 3) as usize;
        let mut bytes = self.dma.read32(aligned).to_le_bytes();
        bytes[byte_index] = value;
        if let Some(transfer) = self.dma.write32(aligned, u32::from_le_bytes(bytes)) {
            let _ = self.execute_dma_transfer(transfer);
        }
    }

    fn write_gpu8(&mut self, address: u32, value: u8) {
        let register = address & !3;
        let byte_index = (address & 3) as usize;
        let mut bytes = match register {
            GPU_GP0 => 0_u32.to_le_bytes(),
            GPU_GP1 => self.gpu.read_status().to_le_bytes(),
            _ => unreachable!(),
        };
        bytes[byte_index] = value;
        let word = u32::from_le_bytes(bytes);

        match register {
            GPU_GP0 => self.gpu.write_gp0(word),
            GPU_GP1 => self.gpu.write_gp1(word),
            _ => unreachable!(),
        }
    }

    fn execute_dma_transfer(&mut self, transfer: DmaTransfer) -> Result<()> {
        match transfer.channel {
            DmaChannel::Gpu if transfer.control.direction == DmaDirection::FromRam => {
                self.execute_gpu_dma(transfer)?;
                self.dma.complete(transfer.channel);
            }
            DmaChannel::CdRom if transfer.control.direction == DmaDirection::ToRam => {
                self.execute_cdrom_dma(transfer)?;
                self.dma.complete(transfer.channel);
            }
            _ => {
                self.dma.complete(transfer.channel);
            }
        }
        Ok(())
    }

    fn execute_gpu_dma(&mut self, transfer: DmaTransfer) -> Result<()> {
        match transfer.control.sync {
            DmaSyncMode::LinkedList => self.execute_gpu_linked_list_dma(transfer.base_address),
            DmaSyncMode::Manual | DmaSyncMode::Request => {
                let mut address = transfer.base_address;
                for _ in 0..transfer.words {
                    let word = self.read_ram_word(address)?;
                    self.gpu.write_gp0(word);
                    address = match transfer.control.step {
                        DmaStep::Increment => address.wrapping_add(4),
                        DmaStep::Decrement => address.wrapping_sub(4),
                    };
                }
                Ok(())
            }
            DmaSyncMode::Reserved => Ok(()),
        }
    }

    fn execute_gpu_linked_list_dma(&mut self, base_address: u32) -> Result<()> {
        let mut address = base_address & 0x001f_fffc;
        let mut packets = 0usize;

        loop {
            let header = self.read_ram_word(address)?;
            let words = (header >> 24) as usize;
            let next = header & 0x00ff_ffff;

            let mut packet_word_address = address.wrapping_add(4);
            for _ in 0..words {
                let word = self.read_ram_word(packet_word_address)?;
                self.gpu.write_gp0(word);
                packet_word_address = packet_word_address.wrapping_add(4);
            }

            packets += 1;
            if next == 0x00ff_ffff {
                break;
            }
            if packets > 4096 {
                break;
            }
            address = next & 0x001f_fffc;
        }

        Ok(())
    }

    fn execute_cdrom_dma(&mut self, transfer: DmaTransfer) -> Result<()> {
        let mut address = transfer.base_address;
        for _ in 0..transfer.words {
            let word = u32::from_le_bytes([
                self.cdrom.read_data_byte(),
                self.cdrom.read_data_byte(),
                self.cdrom.read_data_byte(),
                self.cdrom.read_data_byte(),
            ]);
            self.write_ram_word(address, word)?;
            address = match transfer.control.step {
                DmaStep::Increment => address.wrapping_add(4),
                DmaStep::Decrement => address.wrapping_sub(4),
            };
        }
        Ok(())
    }

    fn read_ram_word(&self, address: u32) -> Result<u32> {
        require_aligned(address, 4)?;
        let physical = mask_region(address);
        match physical {
            RAM_BASE..RAM_END => {
                let offset = ram_offset(physical);
                Ok(u32::from_le_bytes([
                    self.ram[offset & (RAM_SIZE - 1)],
                    self.ram[(offset + 1) & (RAM_SIZE - 1)],
                    self.ram[(offset + 2) & (RAM_SIZE - 1)],
                    self.ram[(offset + 3) & (RAM_SIZE - 1)],
                ]))
            }
            _ => Err(Error::AddressOutOfRange(address)),
        }
    }

    fn write_ram_word(&mut self, address: u32, value: u32) -> Result<()> {
        require_aligned(address, 4)?;
        let physical = mask_region(address);
        match physical {
            RAM_BASE..RAM_END => {
                let bytes = value.to_le_bytes();
                let offset = ram_offset(physical);
                for (index, byte) in bytes.into_iter().enumerate() {
                    self.ram[(offset + index) & (RAM_SIZE - 1)] = byte;
                }
                Ok(())
            }
            _ => Err(Error::AddressOutOfRange(address)),
        }
    }

    fn write_io8(&mut self, address: u32, value: u8) {
        let offset = (address - IO_BASE) as usize;

        // Interrupt status acknowledges bits by writing zeroes to them.
        if (0x70..0x74).contains(&offset) {
            let base = 0x70;
            let old = u32::from_le_bytes([
                self.io[base],
                self.io[base + 1],
                self.io[base + 2],
                self.io[base + 3],
            ]);
            let mut acknowledge = old.to_le_bytes();
            acknowledge[offset - base] = value;
            let new = old & u32::from_le_bytes(acknowledge);
            self.io[base..base + 4].copy_from_slice(&new.to_le_bytes());
            self.sync_cdrom_interrupt();
            return;
        }

        self.io[offset] = value;
        self.write_root_counter_byte(offset, value);
    }

    fn tick_root_counters(&mut self) {
        for counter in 0..ROOT_COUNTER_COUNT {
            self.root_counters[counter] = self.root_counters[counter].wrapping_add(1);
            let offset = ROOT_COUNTER_BASE_OFFSET + counter * ROOT_COUNTER_STRIDE;
            self.io[offset..offset + 2].copy_from_slice(&self.root_counters[counter].to_le_bytes());
        }
    }

    fn tick_vblank(&mut self) {
        self.vblank_ticks += 1;
        if self.vblank_ticks < VBLANK_INTERVAL_TICKS {
            return;
        }

        self.vblank_ticks = 0;
        let interrupt_status = self.io_word(INTERRUPT_STATUS_OFFSET) | VBLANK_INTERRUPT_BIT;
        self.io[INTERRUPT_STATUS_OFFSET..INTERRUPT_STATUS_OFFSET + 4]
            .copy_from_slice(&interrupt_status.to_le_bytes());
    }

    fn write_root_counter_byte(&mut self, offset: usize, value: u8) {
        for counter in 0..ROOT_COUNTER_COUNT {
            let base = ROOT_COUNTER_BASE_OFFSET + counter * ROOT_COUNTER_STRIDE;
            if offset == base || offset == base + 1 {
                let byte_index = offset - base;
                let mut bytes = self.root_counters[counter].to_le_bytes();
                bytes[byte_index] = value;
                self.root_counters[counter] = u16::from_le_bytes(bytes);
                return;
            }
        }
    }

    fn peek8(&self, address: u32) -> Result<u8> {
        let physical = mask_region(address);
        match physical {
            RAM_BASE..RAM_END => Ok(self.ram[ram_offset(physical)]),
            EXPANSION_1_BASE..EXPANSION_1_END => Ok(0xff),
            SCRATCHPAD_BASE..SCRATCHPAD_END => {
                Ok(self.scratchpad[(physical - SCRATCHPAD_BASE) as usize])
            }
            GPU_GP1..GPU_END => {
                Ok(self.gpu.read_status().to_le_bytes()[(physical - GPU_GP1) as usize])
            }
            IO_BASE..IO_END => Ok(self.io[(physical - IO_BASE) as usize]),
            CACHE_CONTROL..CACHE_CONTROL_END => {
                Ok(self.cache_control.to_le_bytes()[(physical - CACHE_CONTROL) as usize])
            }
            BIOS_BASE..BIOS_END => Ok(self.bios.read8(physical - BIOS_BASE)),
            _ => {
                log::warn!("unhandled address at {address:#010x} (physical {physical:#010x})");
                Err(Error::AddressOutOfRange(address))
            }
        }
    }

    fn sync_cdrom_interrupt(&mut self) {
        let mut interrupt_status = self.io_word(INTERRUPT_STATUS_OFFSET);

        if self.cdrom.has_interrupt() {
            interrupt_status |= CDROM_INTERRUPT_BIT;
            let _ = self.write_ram_word(PSYQ_CD_SYNC_FLAG_ADDRESS, 1);
        } else {
            interrupt_status &= !CDROM_INTERRUPT_BIT;
        }

        self.io[INTERRUPT_STATUS_OFFSET..INTERRUPT_STATUS_OFFSET + 4]
            .copy_from_slice(&interrupt_status.to_le_bytes());
    }

    fn io_word(&self, offset: usize) -> u32 {
        u32::from_le_bytes([
            self.io[offset],
            self.io[offset + 1],
            self.io[offset + 2],
            self.io[offset + 3],
        ])
    }
}

fn mask_region(address: u32) -> u32 {
    match address {
        0x8000_0000..=0x9fff_ffff => address - 0x8000_0000,
        0xa000_0000..=0xbfff_ffff => address - 0xa000_0000,
        _ => address,
    }
}

fn ram_offset(address: u32) -> usize {
    (address as usize) & (RAM_SIZE - 1)
}

fn require_aligned(address: u32, width: usize) -> Result<()> {
    if address as usize % width == 0 {
        Ok(())
    } else {
        Err(Error::UnalignedAccess { address, width })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CdRomCommand;

    fn bus() -> Bus {
        Bus::new(Bios::from_bytes(vec![0; crate::bios::BIOS_SIZE]).unwrap())
    }

    #[test]
    fn mirrors_kuseg_kseg0_and_kseg1_ram() {
        let mut bus = bus();
        bus.write32(0x8000_1000, 0x1234_5678).unwrap();

        assert_eq!(bus.read32(0x0000_1000).unwrap(), 0x1234_5678);
        assert_eq!(bus.read32(0xa000_1000).unwrap(), 0x1234_5678);
    }

    #[test]
    fn rejects_unaligned_word_reads() {
        assert!(matches!(
            bus().read32(3),
            Err(Error::UnalignedAccess {
                address: 3,
                width: 4
            })
        ));
    }

    #[test]
    fn stores_common_io_registers() {
        let mut bus = bus();

        bus.write32(0xfffe_0130, 0x0000_0804).unwrap();

        assert_eq!(bus.read32(0xfffe_0130).unwrap(), 0x0000_0804);
    }

    #[test]
    fn gpu_status_reports_ready() {
        let mut bus = bus();

        assert_eq!(bus.read32(GPU_GP1).unwrap(), 0x1f00_0000);
    }

    #[test]
    fn gpu_gp0_writes_update_framebuffer() {
        let mut bus = bus();

        bus.write32(GPU_GP0, 0x02_00_ff_00).unwrap();
        bus.write32(GPU_GP0, 3 | (4 << 16)).unwrap();
        bus.write32(GPU_GP0, 1 | (1 << 16)).unwrap();

        let rgb = bus.framebuffer_rgb();
        let offset = (4 * crate::gpu::VRAM_WIDTH + 3) * 3;

        assert_eq!(&rgb[offset..offset + 3], &[0x00, 0xff, 0x00]);
    }

    #[test]
    fn gpu_dma_linked_list_sends_packets_to_gp0() {
        let mut bus = bus();

        bus.write32(0x0000_2000, 0x03ff_ffff).unwrap();
        bus.write32(0x0000_2004, 0x02_00_00_ff).unwrap();
        bus.write32(0x0000_2008, 7 | (9 << 16)).unwrap();
        bus.write32(0x0000_200c, 1 | (1 << 16)).unwrap();

        bus.write32(0x1f80_10a0, 0x0000_2000).unwrap();
        bus.write32(0x1f80_10a4, 0).unwrap();
        bus.write32(0x1f80_10a8, 0x0100_0401).unwrap();

        let rgb = bus.framebuffer_rgb();
        let offset = (9 * crate::gpu::VRAM_WIDTH + 7) * 3;

        assert_eq!(&rgb[offset..offset + 3], &[0xff, 0x00, 0x00]);
    }

    #[test]
    fn stubs_expansion_region_reads_and_writes() {
        let mut bus = bus();

        bus.write8(0x1f00_0084, 0x12).unwrap();

        assert_eq!(bus.read8(0x1f00_0084).unwrap(), 0xff);
    }

    #[test]
    fn cdrom_interrupts_raise_main_interrupt_status() {
        let mut bus = bus();

        bus.write8(0x1f80_1800, 1).unwrap();
        bus.write8(0x1f80_1802, 0x1f).unwrap();
        bus.write8(0x1f80_1800, 0).unwrap();
        bus.write8(0x1f80_1801, CdRomCommand::GetStat.code())
            .unwrap();

        assert_ne!(bus.read32(0x1f80_1070).unwrap() & CDROM_INTERRUPT_BIT, 0);
        assert_eq!(bus.read32(PSYQ_CD_SYNC_FLAG_ADDRESS).unwrap(), 1);

        bus.write8(0x1f80_1800, 1).unwrap();
        bus.write8(0x1f80_1803, 0x03).unwrap();

        assert_eq!(bus.read32(0x1f80_1070).unwrap() & CDROM_INTERRUPT_BIT, 0);
    }

    #[test]
    fn interrupt_status_writes_do_not_create_interrupts() {
        let mut bus = bus();

        bus.write32(0x1f80_1070, 0xffff_ffff).unwrap();

        assert_eq!(bus.read32(0x1f80_1070).unwrap(), 0);
    }

    #[test]
    fn root_counters_advance_on_tick() {
        let mut bus = bus();

        bus.tick();
        bus.tick();

        assert_eq!(bus.read16(0x1f80_1110).unwrap(), 2);
    }

    #[test]
    fn vblank_interrupt_is_raised_periodically() {
        let mut bus = bus();

        for _ in 0..VBLANK_INTERVAL_TICKS {
            bus.tick();
        }

        assert_ne!(bus.read32(0x1f80_1070).unwrap() & VBLANK_INTERRUPT_BIT, 0);
    }

    #[test]
    fn cdrom_dma_copies_sector_data_to_ram() {
        let mut bus = bus();
        let mut raw = vec![0; crate::cdrom::RAW_SECTOR_SIZE];
        raw[24] = 0x11;
        raw[25] = 0x22;
        raw[26] = 0x33;
        raw[27] = 0x44;
        raw[28] = 0x55;
        raw[29] = 0x66;
        raw[30] = 0x77;
        raw[31] = 0x88;

        bus.load_cd_image(CdImage::from_raw_for_test(raw));
        bus.write8(0x1f80_1800, 0).unwrap();
        bus.write8(0x1f80_1801, CdRomCommand::ReadN.code()).unwrap();
        assert_eq!(bus.read8(0x1f80_1801).unwrap(), 0x22);

        bus.write32(0x1f80_10b0, 0x0000_2000).unwrap();
        bus.write32(0x1f80_10b4, 0x0001_0002).unwrap();
        bus.write32(0x1f80_10b8, 0x0100_0200).unwrap();

        assert_eq!(bus.read32(0x0000_2000).unwrap(), 0x4433_2211);
        assert_eq!(bus.read32(0x0000_2004).unwrap(), 0x8877_6655);
    }
}
