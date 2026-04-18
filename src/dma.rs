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

pub struct DmaController {
    channels: [ChannelRegisters; 7],
    control: u32,
    interrupt: u32,
}

impl DmaChannel {
    fn index(self) -> usize {
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
        let sync = match (value >> 9) & 3 {
            0 => DmaSyncMode::Manual,
            1 => DmaSyncMode::Request,
            2 => DmaSyncMode::LinkedList,
            _ => DmaSyncMode::Reserved,
        };

        Self {
            direction: if value & 1 == 0 {
                DmaDirection::ToRam
            } else {
                DmaDirection::FromRam
            },
            step: if value & (1 << 1) == 0 {
                DmaStep::Increment
            } else {
                DmaStep::Decrement
            },
            sync,
            trigger: value & (1 << 28) != 0,
            enable: value & (1 << 24) != 0,
        }
    }

    fn active(self) -> bool {
        self.enable && (self.sync != DmaSyncMode::Manual || self.trigger)
    }
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            channels: [ChannelRegisters::default(); 7],
            control: 0x0765_4321,
            interrupt: 0,
        }
    }

    pub fn read32(&self, offset: u32) -> u32 {
        match offset {
            0x70 => self.control,
            0x74 => self.interrupt,
            _ => self.channel_register(offset).unwrap_or_default(),
        }
    }

    pub fn write32(&mut self, offset: u32, value: u32) -> Option<DmaTransfer> {
        match offset {
            0x70 => {
                self.control = value;
                None
            }
            0x74 => {
                self.interrupt = value;
                None
            }
            _ => {
                let channel = DmaChannel::from_index((offset / 0x10) as usize)?;
                let register = offset & 0x0f;
                let channel_registers = &mut self.channels[channel.index()];
                match register {
                    0x00 => channel_registers.base_address = value & 0x00ff_ffff,
                    0x04 => channel_registers.block_control = value,
                    0x08 => {
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
        registers.control &= !(1 << 24);
        registers.control &= !(1 << 28);
        self.interrupt |= 1 << (24 + channel.index());
    }

    fn channel_register(&self, offset: u32) -> Option<u32> {
        let channel = DmaChannel::from_index((offset / 0x10) as usize)?;
        let registers = &self.channels[channel.index()];
        match offset & 0x0f {
            0x00 => Some(registers.base_address),
            0x04 => Some(registers.block_control),
            0x08 => Some(registers.control),
            _ => Some(0),
        }
    }

    fn transfer(&self, channel: DmaChannel, control: DmaControl) -> DmaTransfer {
        let registers = &self.channels[channel.index()];
        DmaTransfer {
            channel,
            base_address: registers.base_address & 0x001f_fffc,
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
    let block_size = (block_control & 0xffff) as usize;
    let block_count = (block_control >> 16) as usize;

    match sync {
        DmaSyncMode::Manual => {
            if block_size == 0 {
                0x1_0000
            } else {
                block_size
            }
        }
        DmaSyncMode::Request => {
            let block_size = if block_size == 0 {
                0x1_0000
            } else {
                block_size
            };
            let block_count = if block_count == 0 {
                0x1_0000
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
