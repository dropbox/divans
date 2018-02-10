extern crate core;
extern crate md4;
extern crate alloc_no_stdlib as alloc;
use alloc::{SliceWrapper, SliceWrapperMut, Allocator};
use std::collections::HashMap;
mod fixed_buffer;
pub use fixed_buffer::{
    CryptoSigTrait,
    FixedBuffer2,
    FixedBuffer3,
    FixedBuffer4,
    FixedBuffer5,
    FixedBuffer6,
    FixedBuffer7,
    FixedBuffer8,
    FixedBuffer12,
    FixedBuffer16,
    FixedBuffer24,
    FixedBuffer32,
};
const MD4_MAGIC: [u8;4] = [0x72, 0x73, 0x01, 0x36];
const BLAKE5_MAGIC: [u8;4] = [0x72, 0x73, 0x01, 0x37];
const HEADER_SIZE: usize = 12;
#[derive(Default, Copy, Clone)]
pub struct Sig<SigBuffer:CryptoSigTrait> {
    crc32: u32,
    crypto_sig: SigBuffer,
}

pub struct SigFile<SigBuffer:CryptoSigTrait, AllocSig: Allocator<Sig<SigBuffer>>> {
    block_size: u32,
    signatures: AllocSig::AllocatedMemory,
    blake5: bool,
}

fn le_to_u32(data:&[u8]) -> u32 {
    u32::from(data[3]) + u32::from(data[2]) * 256 + u32::from(data[1]) * 65536 + u32::from(data[0]) * 65536 * 256
}
fn u32_to_le(val: u32) -> [u8;4] {
    [((val >> 24) & 0xff) as u8,
     ((val >> 16) & 0xff) as u8,
     ((val >> 8) & 0xff) as u8,
     (val & 0xff) as u8]
}
fn full_serialize<SigBuffer:CryptoSigTrait>(item: Sig<SigBuffer>, output: &mut [u8]) -> usize {
    let mut first_split = output.split_at_mut(4);
    first_split.0.clone_from_slice(&u32_to_le(item.crc32)[..]);
    first_split.1.split_at_mut(SigBuffer::SIZE).0.clone_from_slice(item.crypto_sig.slice());
    4 + SigBuffer::SIZE
}

fn partial_serialize<SigBuffer:CryptoSigTrait>(item: Sig<SigBuffer>, input_offset : &mut usize, output: &mut [u8], output_offset: &mut usize) -> bool {
    let mut buffer = [0u8; 36];
    assert!(buffer.len() >= 4 + SigBuffer::SIZE);
    full_serialize(item, &mut buffer[..]);
    let buffer_offset = *input_offset % (4 + SigBuffer::SIZE);
    let to_copy = core::cmp::min(buffer.len() - buffer_offset, output.len() - *output_offset);
    output.split_at_mut(*output_offset).1.split_at_mut(to_copy).0.clone_from_slice(buffer.split_at(buffer_offset).1.split_at(to_copy).0);
    *input_offset += to_copy;
    *output_offset += to_copy;
    to_copy == buffer.len()
}

impl <SigBuffer:CryptoSigTrait, AllocSig: Allocator<Sig<SigBuffer>>> SigFile<SigBuffer,AllocSig> {
    pub fn new(m_sig:&mut AllocSig, buf: &[u8]) -> Self {
        unimplemented!();
    }
    pub fn serialize(&self, input_offset: &mut usize, output: &mut [u8], output_offset: &mut usize) -> bool {
        while *input_offset < 12 && *output_offset < output.len() {
            let mut header_buffer = [0u8;12];
            if self.blake5 {
                header_buffer[0..4].clone_from_slice(&BLAKE5_MAGIC);
            } else {
                header_buffer[0..4].clone_from_slice(&MD4_MAGIC);
            }
            header_buffer[4..8].clone_from_slice(&u32_to_le(self.block_size));
            header_buffer[8..12].clone_from_slice(&u32_to_le(SigBuffer::SIZE as u32));
            let to_copy = core::cmp::min(HEADER_SIZE - *input_offset, output.len() - *output_offset);
            output.split_at_mut(*output_offset).1.split_at_mut(to_copy).0.clone_from_slice(
                header_buffer.split_at(*input_offset).1.split_at(to_copy).0);
            *input_offset += to_copy;
            *output_offset += to_copy;
        }
        let stride = SigBuffer::SIZE + 4;
        let start_index = (*input_offset - HEADER_SIZE) / stride;
        let stop_index = core::cmp::min(
            self.signatures.slice().len(),
            (*input_offset - HEADER_SIZE) / stride + (output.len() - *output_offset + stride - 1) / stride);
        if start_index != stop_index {
            debug_assert!(*output_offset != output.len());  // otherwise we wouldn't have gotten here
            partial_serialize(self.signatures.slice()[start_index], input_offset, output, output_offset);
        }
        if start_index + 1 < stop_index {
            debug_assert!(*output_offset + stride <= output.len());  // otherwise we wouldn't have gotten here
            for item in self.signatures.slice()[start_index + 1..stop_index - 1].iter() {
                let delta = full_serialize(*item, output.split_at_mut(*output_offset).1);
                *output_offset += delta;
                *input_offset += delta;
            }
        }
        if start_index + 1 != stop_index {
            debug_assert!(*output_offset != output.len());  // otherwise we wouldn't have gotten here
            partial_serialize(self.signatures.slice()[stop_index - 1], input_offset, output, output_offset)
        } else {
            false
        }
    }
    pub fn deserialize(m_sig:&mut AllocSig, on_disk_format: &[u8]) -> core::result::Result<Self, usize> {
        if on_disk_format.len() < 12 {
            return Err(0);
        }
        let is_md4 = &MD4_MAGIC[..] == &on_disk_format[..4];
        let is_blake5 = &BLAKE5_MAGIC[..] == &on_disk_format[..4];
        if !(is_md4 || is_blake5) {
            return Err(0);
        }
        let desired_buffer_size = le_to_u32(&on_disk_format[8..HEADER_SIZE]);
        if desired_buffer_size != SigBuffer::SIZE as u32 {
            return Err(desired_buffer_size as usize);
        }
        let stride = 4 + core::mem::size_of::<Sig<SigBuffer>>();
        let mut sigs = m_sig.alloc_cell((on_disk_format.len() - HEADER_SIZE) / stride);
        for (index, item) in sigs.slice_mut().iter_mut().enumerate() {
            let record_start = on_disk_format.split_at(index * stride + HEADER_SIZE).1;
            item.crypto_sig.slice_mut().clone_from_slice(&on_disk_format[4..(4 + SigBuffer::SIZE)]);
            item.crc32 = le_to_u32(record_start);
        }
        Ok(SigFile::<SigBuffer,AllocSig> {
            block_size: le_to_u32(&on_disk_format[4..8]),
            signatures: sigs,
            blake5: is_blake5,
        })
    }
    pub fn free(&mut self, m_sig: &mut AllocSig) {
        m_sig.free_cell(core::mem::replace(&mut self.signatures, AllocSig::AllocatedMemory::default()))
    }
    pub fn create_sig_hint(&self) -> SigHint {
        let mut hint = SigHint {
            crc32_to_sig_index: HashMap::<u32, usize>::with_capacity(self.signatures.slice().len()),
        };
        let mut file_offset = 0usize;
        for (index, item) in self.signatures.slice().iter().enumerate() {
            hint.crc32_to_sig_index.insert(item.crc32, index);
        }
        hint
    }
}

pub struct SigHint {
    crc32_to_sig_index: HashMap<u32, usize>,
}

