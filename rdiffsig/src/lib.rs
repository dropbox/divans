extern crate core;
extern crate md4;
extern crate alloc_no_stdlib as alloc;
mod crc32;
use crc32::{crc_rollout, crc_rotate, crc_update};

use md4::Digest;
use core::cmp::min;
use alloc::{SliceWrapper, SliceWrapperMut, Allocator};
use std::collections::HashMap;
mod fixed_buffer;
pub use fixed_buffer::{
    CryptoSigTrait,
    FixedBuffer1,
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


#[derive(Copy,Clone)]
pub struct SigFileStat {
    file_size: usize,
    block_size: u32,
    #[allow(dead_code)]
    crypto_hash_size: u32,
    #[allow(dead_code)]
    blake5: bool,
}

impl SigFileStat {
    pub fn new(on_disk_format: &[u8]) -> Result<Self, ()> {
        let is_md4 = &MD4_MAGIC[..] == &on_disk_format[..4];
        let is_blake5 = &BLAKE5_MAGIC[..] == &on_disk_format[..4];
        if is_md4 == false && is_blake5 == false {
            return Err(());
        }
        let crypto_hash_size = le_to_u32(&on_disk_format[8..HEADER_SIZE]);
        let stride = 4 + crypto_hash_size as usize;
        let data_len = on_disk_format.len() - HEADER_SIZE;
        if data_len % stride != 0 {
            return Err(());
        }
        let block_size = le_to_u32(&on_disk_format[4..8]);
        Ok(SigFileStat {
            block_size: block_size,
            file_size: block_size as usize * (data_len / stride),
            crypto_hash_size: crypto_hash_size,
            blake5: is_blake5,
        })
    }
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
    let first_split = output.split_at_mut(4);
    first_split.0.clone_from_slice(&u32_to_le(item.crc32)[..]);
    first_split.1.split_at_mut(SigBuffer::SIZE).0.clone_from_slice(item.crypto_sig.slice());
    4 + SigBuffer::SIZE
}

fn partial_serialize<SigBuffer:CryptoSigTrait>(item: Sig<SigBuffer>, input_offset : &mut usize, output: &mut [u8], output_offset: &mut usize) -> bool {
    let mut buffer = [0u8; 36];
    assert!(buffer.len() >= 4 + SigBuffer::SIZE);
    full_serialize(item, &mut buffer[..]);
    let buffer_offset = *input_offset % (4 + SigBuffer::SIZE);
    let to_copy = min(4 + SigBuffer::SIZE - buffer_offset, output.len() - *output_offset);
    output.split_at_mut(*output_offset).1.split_at_mut(to_copy).0.clone_from_slice(buffer.split_at(buffer_offset).1.split_at(to_copy).0);
    *input_offset += to_copy;
    *output_offset += to_copy;
    to_copy == buffer.len()
}


impl <SigBuffer:CryptoSigTrait, AllocSig: Allocator<Sig<SigBuffer>>> SigFile<SigBuffer,AllocSig> {
    pub fn new(m_sig:&mut AllocSig, block_size: u32, buf: &[u8]) -> Self {
        let num_signatures = (buf.len() + block_size as usize - 1) / block_size as usize;
        let mut sig = m_sig.alloc_cell(num_signatures);
        for (index, item) in sig.slice_mut().iter_mut().enumerate() {
            let slice = &buf[index * block_size as usize .. min((index + 1) * block_size as usize, buf.len())];
            let mut md4_hasher = md4::Md4::default();
            md4_hasher.input(slice);
            item.crypto_sig.slice_mut().clone_from_slice(&md4_hasher.result()[..SigBuffer::SIZE]);
            item.crc32 = crc_update(item.crc32, slice);
        }
        SigFile::<SigBuffer, AllocSig> {
            block_size: block_size,
            signatures: sig,
            blake5: false,
        }
    }
    pub fn signatures(&self) -> &[Sig<SigBuffer>] {
        self.signatures.slice()
    }
    pub fn block_size(&self) -> u32 {
        self.block_size
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
            let to_copy = min(HEADER_SIZE - *input_offset, output.len() - *output_offset);
            output.split_at_mut(*output_offset).1.split_at_mut(to_copy).0.clone_from_slice(
                header_buffer.split_at(*input_offset).1.split_at(to_copy).0);
            *input_offset += to_copy;
            *output_offset += to_copy;
        }
        if *output_offset == output.len() {
            return false;
        }
        let stride = SigBuffer::SIZE + 4;
        let start_index = (*input_offset - HEADER_SIZE) / stride;
        let stop_index = min(self.signatures.slice().len(),
                             (*input_offset - HEADER_SIZE) / stride + (output.len() - *output_offset + stride - 1) / stride);
        if start_index < stop_index {
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
        if start_index + 1 < stop_index {
            debug_assert!(*output_offset != output.len());  // otherwise we wouldn't have gotten here
            partial_serialize(self.signatures.slice()[stop_index - 1], input_offset, output, output_offset)
        } else {
            start_index == stop_index
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
        let desired_crypto_hash_size = le_to_u32(&on_disk_format[8..HEADER_SIZE]);
        if desired_crypto_hash_size != SigBuffer::SIZE as u32 {
            return Err(desired_crypto_hash_size as usize);
        }
        let stride = 4 + desired_crypto_hash_size as usize;
        if (on_disk_format.len() - HEADER_SIZE) % stride != 0 {
            return Err(on_disk_format.len() - HEADER_SIZE)
        }
        let mut sigs = m_sig.alloc_cell((on_disk_format.len() - HEADER_SIZE) / stride);
        for (index, item) in sigs.slice_mut().iter_mut().enumerate() {
            let record_start = on_disk_format.split_at(index * stride + HEADER_SIZE).1;
            item.crypto_sig.slice_mut().clone_from_slice(&record_start[4..(4 + desired_crypto_hash_size as usize)]);
            item.crc32 = le_to_u32(record_start);
        }
        Ok(SigFile::<SigBuffer,AllocSig> {
            block_size: le_to_u32(&on_disk_format[4..8]),
            signatures: sigs,
            blake5: is_blake5,
        })
    }
    pub fn stat(&self) -> SigFileStat {
        SigFileStat {
            block_size: self.block_size(),
            file_size: self.block_size() as usize * self.signatures.slice().len(),
            crypto_hash_size: SigBuffer::SIZE as u32,
            blake5: self.blake5,
        }
    }
    pub fn free(&mut self, m_sig: &mut AllocSig) {
        m_sig.free_cell(core::mem::replace(&mut self.signatures, AllocSig::AllocatedMemory::default()))
    }
    pub fn create_sig_hint(&self) -> SigHint {
        let mut hint = SigHint {
            crc32_to_sig_index: HashMap::<u32, usize>::with_capacity(self.signatures.slice().len()),
        };
        for (index, item) in self.signatures.slice().iter().enumerate() {
            hint.crc32_to_sig_index.insert(item.crc32, index);
        }
        hint
    }
}

pub struct SigHint {
    crc32_to_sig_index: HashMap<u32, usize>,
}

pub struct CustomDictionary<AllocU8:Allocator<u8>> {
    data:AllocU8::AllocatedMemory,
    invalid:AllocU8::AllocatedMemory,
    ring_buffer:AllocU8::AllocatedMemory,
    ring_buffer_offset: u32,
    rolling_crc32:u32,
    rolling_count:u32,
    file_offset: usize,
}
const MIN_DICT_VALID: usize = 8;
impl<AllocU8:Allocator<u8>> CustomDictionary<AllocU8> {
    pub fn new(m8: &mut AllocU8,
               sig_file: SigFileStat) -> Self{
        let d = m8.alloc_cell(sig_file.file_size + MIN_DICT_VALID);
        let mut invalid = m8.alloc_cell(d.slice().len());
        for i in invalid.slice_mut()[..sig_file.file_size].iter_mut() {
            *i = 0x78; // last 8 zeros are considered 'valid'
        }
        let ring_buffer = m8.alloc_cell(sig_file.block_size as usize);
        CustomDictionary::<AllocU8> {
            data: d,
            invalid: invalid,
            ring_buffer: ring_buffer,
            rolling_count: 0,
            ring_buffer_offset: 0,
            rolling_crc32: 0,
            file_offset: 0,
        }
    }
    pub fn dict(&self) -> &[u8]{
        self.data.slice()
    }
    pub fn dict_mask(&self) -> &[u8]{
        self.invalid.slice()
    }
    pub fn speculative_add_helper<SigBuffer:CryptoSigTrait,
                   AllocSig: Allocator<Sig<SigBuffer>>>(sig_offset: usize,
                                                        sig_file: &SigFile<SigBuffer,
                                                                           AllocSig>,
                                                        length: u32,
                                                        ring_buffer: &[u8],
                                                        ring_buffer_offset: usize,
                                                        dict: &mut[u8],
                                                        invalid: &mut [u8]) -> bool {
         let mut md4_hasher = md4::Md4::default();
         let ring_buffer_pair = ring_buffer.split_at(ring_buffer_offset as usize);
         let first_ring_copy_len = min(length as usize, ring_buffer_pair.1.len());
         md4_hasher.input(&ring_buffer_pair.1[..first_ring_copy_len]);
         let second_ring_copy_len = min(length as usize - first_ring_copy_len, ring_buffer_pair.0.len());
         md4_hasher.input(&ring_buffer_pair.0[..second_ring_copy_len]);
         let md4_sum = &md4_hasher.result()[..SigBuffer::SIZE];
         if sig_file.signatures.slice()[sig_offset].crypto_sig.slice() == md4_sum {
             let dict_target = sig_offset * sig_file.block_size() as usize;
             dict.split_at_mut(dict_target).1.split_at_mut(
                 first_ring_copy_len).0.clone_from_slice(&ring_buffer_pair.1[..first_ring_copy_len]);
             dict.split_at_mut(dict_target + first_ring_copy_len).1.split_at_mut(
                 second_ring_copy_len).0.clone_from_slice(&ring_buffer_pair.0[..second_ring_copy_len]);
             for item in invalid.split_at_mut(dict_target).1.split_at_mut(
                 first_ring_copy_len + second_ring_copy_len).0.iter_mut() {
                 *item = 0;
             }
             true
         } else {
             false
         }
    }
    pub fn speculative_add<SigBuffer:CryptoSigTrait,
                   AllocSig: Allocator<Sig<SigBuffer>>>(&mut self,
                   sig_offset: usize,
                   sig_file: &SigFile<SigBuffer,
                                      AllocSig>,
                   length: u32) -> bool {
        Self::speculative_add_helper(sig_offset, sig_file, length,
                                     self.ring_buffer.slice(), self.ring_buffer_offset as usize,
                                     self.data.slice_mut(), self.invalid.slice_mut())
    }
    pub fn write<SigBuffer:CryptoSigTrait,
                   AllocSig: Allocator<Sig<SigBuffer>>>(&mut self,
                   mut input: &[u8],
                   hint: &SigHint,
                   sig_file: &SigFile<SigBuffer,
                                      AllocSig>) {
        while input.len() != 0 {
            while (self.rolling_count as usize) < self.ring_buffer.slice().len() {
                let to_copy = min(self.ring_buffer.slice().len() - self.rolling_count as usize, input.len());
                let input_split = input.split_at(to_copy);
                self.ring_buffer.slice_mut().split_at_mut(self.rolling_count as usize).1.split_at_mut(to_copy).0.clone_from_slice(input_split.0);
                self.rolling_count += to_copy as u32;
                input = input_split.1;
                if self.rolling_count as usize == self.ring_buffer.slice().len() {
                    self.rolling_crc32 = crc_update(0, self.ring_buffer.slice());
                    //print!("Checking offset {:?} {:x} off:0x{:x}\n", self.rolling_count, self.rolling_crc32, self.file_offset);
                    if let Some(dict_offset) = hint.crc32_to_sig_index.get(&self.rolling_crc32) {
                        self.file_offset += self.rolling_count as usize;
                        //print!("Found offset {:?} {:?} {:x}\n", self.rolling_count, dict_offset, self.rolling_crc32);
                        let rc = self.rolling_count;
                        if self.speculative_add(*dict_offset, sig_file, rc) {
                            self.rolling_count = 0; //match!
                            continue; // we assume that there are no nontrivial overlapping sections, so we start over
                        }
                    }else {
                        self.file_offset += 1;
                    }
                } else {
                    return
                }
            }
            assert_eq!(self.rolling_count, sig_file.block_size());
            let mut early_exit = false;
            for (index,item) in input.iter().enumerate() { // ring buffer is fully populated here
                {
                    let ring_buffer_mfd_byte = &mut self.ring_buffer.slice_mut()[self.ring_buffer_offset as usize];
                    self.rolling_crc32 = crc_rotate(self.rolling_crc32, sig_file.block_size(), *ring_buffer_mfd_byte, *item);                
                    *ring_buffer_mfd_byte = *item;
                }
                let dict_offset = hint.crc32_to_sig_index.get(&self.rolling_crc32);
                let rc = self.rolling_count;
                //print!("Dhecking offset {:?} {:x} off:0x{:x}\n", self.rolling_count, self.rolling_crc32, self.file_offset);
                self.file_offset += 1;
                if dict_offset.is_some() && self.speculative_add(*dict_offset.unwrap(), sig_file, rc) {
                    //print!("Found offset {:?} {:?} {:x}\n", self.rolling_count, dict_offset, self.rolling_crc32);
                    self.rolling_count = 0; //match!
                    self.ring_buffer_offset = 0;
                    input = &input[index..];
                    early_exit = true;
                    break; // we assume that there are no nontrivial overlapping sections, so we start over with the fast loop
                } else {
                    self.ring_buffer_offset += 1;
                    if self.ring_buffer_offset == sig_file.block_size() {
                        self.ring_buffer_offset = 0;
                    }
                }
            }
            if !early_exit {
                break;
            }
        }
    }
    pub fn flush<SigBuffer:CryptoSigTrait,
                   AllocSig: Allocator<Sig<SigBuffer>>>(&mut self,
                   hint: &SigHint,
                   sig_file: &SigFile<SigBuffer,
                                      AllocSig>) {
        if self.rolling_count as usize != self.ring_buffer.slice().len() { // we havent' computed a crc32 yet: do that now
            let rc = self.rolling_count as usize;
            self.rolling_crc32 = crc_update(0, &self.ring_buffer.slice()[..rc]);
            //print!("Fhecking offset {:?} {:x} off:0x{:x}\n", self.rolling_count, self.rolling_crc32, self.file_offset);
            self.file_offset += self.rolling_count as usize;
            if let Some(dict_offset) = hint.crc32_to_sig_index.get(&self.rolling_crc32) {
                //print!("Found offset {:?} {:?} {:x}\n", self.rolling_count, dict_offset, self.rolling_crc32);
                if self.speculative_add(*dict_offset, sig_file, rc as u32) {
                    return
                }
            }
        }
        {
            let ring_buffer_pair = self.ring_buffer.slice().split_at(self.ring_buffer_offset as usize);
            let ring_buffer_seg = min(self.rolling_count as usize, ring_buffer_pair.1.len());
            let slices_to_iter = [ring_buffer_pair.1.split_at(ring_buffer_seg).0,
                                  ring_buffer_pair.0.split_at(min(self.rolling_count as usize - ring_buffer_seg,
                                                                  ring_buffer_pair.0.len())).0];
            for slice in slices_to_iter.iter() {
                for (roll_mod, item) in slice.iter().enumerate() {
                    crc_rollout(self.rolling_crc32, self.rolling_count - roll_mod as u32, *item);
                    //print!("Ehecking offset {:?} {:x} off:0x{:x}\n", self.rolling_count - roll_mod as u32 , self.rolling_crc32, self.file_offset);
                    self.file_offset += 1;
                    let dict_offset = hint.crc32_to_sig_index.get(&self.rolling_crc32);
                    //print!("VERIFYINGoffset {:?} {:?} {:x}\n", self.rolling_count - roll_mod as u32, dict_offset, self.rolling_crc32);
                    if dict_offset.is_some() &&
                        // call helper to avoid angering the borrow checker
                        Self::speculative_add_helper(*dict_offset.unwrap(), sig_file, self.rolling_count - roll_mod as u32,
                                                     self.ring_buffer.slice(), self.ring_buffer_offset as usize,
                                                     self.data.slice_mut(), self.invalid.slice_mut()) {
                        self.rolling_count = 0;
                        self.ring_buffer_offset = 0;
                        return;
                    }
                }
                self.rolling_count -= slice.len() as u32;
            }
        }
    }
    pub fn free(&mut self, m8: &mut AllocU8) {
        m8.free_cell(core::mem::replace(&mut self.data, AllocU8::AllocatedMemory::default()));
        m8.free_cell(core::mem::replace(&mut self.invalid, AllocU8::AllocatedMemory::default()));
        m8.free_cell(core::mem::replace(&mut self.ring_buffer, AllocU8::AllocatedMemory::default()));
    }
}
