pub const DMA_CHANNEL_COUNT: usize = 7;

const DMA_CHANNEL_STRIDE: u32 = 0x10;
const DMA_BASE_REGISTER_OFFSET: u32 = 0x00;
const DMA_BLOCK_CONTROL_REGISTER_OFFSET: u32 = 0x04;
const DMA_CHANNEL_CONTROL_REGISTER_OFFSET: u32 = 0x08;
const DMA_GLOBAL_CONTROL_OFFSET: u32 = 0x70;
const DMA_INTERRUPT_OFFSET: u32 = 0x74;
const DMA_ADDRESS_MASK: u32 = 0x00ff_ffff;
const DMA_WORD_COUNT_MASK: u32 = 0x0000_ffff;
const DMA_BLOCK_COUNT_SHIFT: u32 = 16;
const DMA_SYNC_SHIFT: u32 = 9;
const DMA_SYNC_MASK: u32 = 0x3;
const DMA_DIRECTION_BIT: u32 = 1 << 0;
const DMA_STEP_BIT: u32 = 1 << 1;
const DMA_ENABLE_BIT: u32 = 1 << 24;
const DMA_TRIGGER_BIT: u32 = 1 << 28;
const DMA_INTERRUPT_FLAG_SHIFT: usize = 24;
const DMA_MAX_WORDS_PER_BLOCK: usize = 0x1_0000;
const DMA_BASE_ADDRESS_ALIGNMENT_MASK: u32 = 0x001f_fffc;
const DMA_DEFAULT_CONTROL: u32 = 0x0765_4321;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaChannel {
    MdecIn,
    MdecOut,
    Gpu,
    CdRom,
    Spu,
    Pio,
    Otc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaDirection {
    ToRam,
    FromRam,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaStep {
    Increment,
    Decrement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaSyncMode {
    Manual,
    Request,
    LinkedList,
    Reserved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaControl {
    pub direction: DmaDirection,
    pub step: DmaStep,
    pub sync: DmaSyncMode,
    pub trigger: bool,
    pub enable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaTransfer {
    pub channel: DmaChannel,
    pub base_address: u32,
    pub words: usize,
    pub control: DmaControl,
}

#[derive(Clone, Copy, Debug, Default)]
struct ChannelRegisters {
    base_address: u32,
    block_control: u32,
    control: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaChannelSnapshot {
    pub base_address: u32,
    pub block_control: u32,
    pub control: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaDebugState {
    pub control: u32,
    pub interrupt: u32,
    pub channels: [DmaChannelSnapshot; DMA_CHANNEL_COUNT],
}

pub struct DmaController {
    channels: [ChannelRegisters; DMA_CHANNEL_COUNT],
    control: u32,
    interrupt: u32,
}

impl DmaChannel {
    pub const fn index(self) -> usize {
        match self {
            Self::MdecIn => 0,
            Self::MdecOut => 1,
            Self::Gpu => 2,
            Self::CdRom => 3,
            Self::Spu => 4,
            Self::Pio => 5,
            Self::Otc => 6,
        }
    }

    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::MdecIn),
            1 => Some(Self::MdecOut),
            2 => Some(Self::Gpu),
            3 => Some(Self::CdRom),
            4 => Some(Self::Spu),
            5 => Some(Self::Pio),
            6 => Some(Self::Otc),
            _ => None,
        }
    }
}

impl DmaControl {
    fn decode(value: u32) -> Self {
        let sync = match (value >> DMA_SYNC_SHIFT) & DMA_SYNC_MASK {
            0 => DmaSyncMode::Manual,
            1 => DmaSyncMode::Request,
            2 => DmaSyncMode::LinkedList,
            _ => DmaSyncMode::Reserved,
        };

        Self {
            direction: if value & DMA_DIRECTION_BIT == 0 {
                DmaDirection::ToRam
            } else {
                DmaDirection::FromRam
            },
            step: if value & DMA_STEP_BIT == 0 {
                DmaStep::Increment
            } else {
                DmaStep::Decrement
            },
            sync,
            trigger: value & DMA_TRIGGER_BIT != 0,
            enable: value & DMA_ENABLE_BIT != 0,
        }
    }

    fn active(self) -> bool {
        self.enable && (self.sync != DmaSyncMode::Manual || self.trigger)
    }
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            channels: [ChannelRegisters::default(); DMA_CHANNEL_COUNT],
            control: DMA_DEFAULT_CONTROL,
            interrupt: 0,
        }
    }

    pub fn read32(&self, offset: u32) -> u32 {
        match offset {
            DMA_GLOBAL_CONTROL_OFFSET => self.control,
            DMA_INTERRUPT_OFFSET => self.interrupt,
            _ => self.channel_register(offset).unwrap_or_default(),
        }
    }

    pub fn debug_state(&self) -> DmaDebugState {
        DmaDebugState {
            control: self.control,
            interrupt: self.interrupt,
            channels: self.channels.map(|channel| DmaChannelSnapshot {
                base_address: channel.base_address,
                block_control: channel.block_control,
                control: channel.control,
            }),
        }
    }

    pub fn write32(&mut self, offset: u32, value: u32) -> Option<DmaTransfer> {
        match offset {
            DMA_GLOBAL_CONTROL_OFFSET => {
                self.control = value;
                None
            }
            DMA_INTERRUPT_OFFSET => {
                self.interrupt = value;
                None
            }
            _ => {
                let channel = DmaChannel::from_index((offset / DMA_CHANNEL_STRIDE) as usize)?;
                let register = offset & (DMA_CHANNEL_STRIDE - 1);
                let channel_registers = &mut self.channels[channel.index()];
                match register {
                    DMA_BASE_REGISTER_OFFSET => {
                        channel_registers.base_address = value & DMA_ADDRESS_MASK;
                    }
                    DMA_BLOCK_CONTROL_REGISTER_OFFSET => channel_registers.block_control = value,
                    DMA_CHANNEL_CONTROL_REGISTER_OFFSET => {
                        channel_registers.control = value;
                        let control = DmaControl::decode(value);
                        if control.active() {
                            let transfer = self.transfer(channel, control);
                            log::info!(
                                "DMA transfer: {channel:?} {direction:?} sync={sync:?} addr={addr:#08x} words={words}",
                                direction = control.direction,
                                sync = control.sync,
                                addr = transfer.base_address,
                                words = transfer.words
                            );
                            return Some(transfer);
                        }
                    }
                    _ => {}
                }
                None
            }
        }
    }

    pub fn complete(&mut self, channel: DmaChannel) {
        log::debug!("DMA transfer complete: {channel:?}");
        let registers = &mut self.channels[channel.index()];
        registers.control &= !DMA_ENABLE_BIT;
        registers.control &= !DMA_TRIGGER_BIT;
        self.interrupt |= 1 << (DMA_INTERRUPT_FLAG_SHIFT + channel.index());
    }

    fn channel_register(&self, offset: u32) -> Option<u32> {
        let channel = DmaChannel::from_index((offset / DMA_CHANNEL_STRIDE) as usize)?;
        let registers = &self.channels[channel.index()];
        match offset & (DMA_CHANNEL_STRIDE - 1) {
            DMA_BASE_REGISTER_OFFSET => Some(registers.base_address),
            DMA_BLOCK_CONTROL_REGISTER_OFFSET => Some(registers.block_control),
            DMA_CHANNEL_CONTROL_REGISTER_OFFSET => Some(registers.control),
            _ => Some(0),
        }
    }

    fn transfer(&self, channel: DmaChannel, control: DmaControl) -> DmaTransfer {
        let registers = &self.channels[channel.index()];
        DmaTransfer {
            channel,
            base_address: registers.base_address & DMA_BASE_ADDRESS_ALIGNMENT_MASK,
            words: transfer_words(registers.block_control, control.sync),
            control,
        }
    }
}

impl Default for DmaController {
    fn default() -> Self {
        Self::new()
    }
}

fn transfer_words(block_control: u32, sync: DmaSyncMode) -> usize {
    let block_size = (block_control & DMA_WORD_COUNT_MASK) as usize;
    let block_count = (block_control >> DMA_BLOCK_COUNT_SHIFT) as usize;

    match sync {
        DmaSyncMode::Manual => {
            if block_size == 0 {
                DMA_MAX_WORDS_PER_BLOCK
            } else {
                block_size
            }
        }
        DmaSyncMode::Request => {
            let block_size = if block_size == 0 {
                DMA_MAX_WORDS_PER_BLOCK
            } else {
                block_size
            };
            let block_count = if block_count == 0 {
                DMA_MAX_WORDS_PER_BLOCK
            } else {
                block_count
            };
            block_size * block_count
        }
        DmaSyncMode::LinkedList | DmaSyncMode::Reserved => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_cdrom_request_transfer() {
        let mut dma = DmaController::new();

        dma.write32(0x30, 0x0000_2000);
        dma.write32(0x34, 0x0001_0004);
        let transfer = dma.write32(0x38, 0x0100_0200).unwrap();

        assert_eq!(transfer.channel, DmaChannel::CdRom);
        assert_eq!(transfer.base_address, 0x2000);
        assert_eq!(transfer.words, 4);
        assert_eq!(transfer.control.direction, DmaDirection::ToRam);
        assert_eq!(transfer.control.sync, DmaSyncMode::Request);
    }
}
