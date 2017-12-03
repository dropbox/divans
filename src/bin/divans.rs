// Copyright 2017 Dropbox, Inc
//
//   Licensed under the Apache License, Version 2.0 (the "License");
//   you may not use this file except in compliance with the License.
//   You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
//   Unless required by applicable law or agreed to in writing, software
//   distributed under the License is distributed on an "AS IS" BASIS,
//   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//   See the License for the specific language governing permissions and
//   limitations under the License.

extern crate core;
extern crate divans;
extern crate brotli;
extern crate alloc_no_stdlib as alloc;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

#[cfg(test)]
extern crate brotli as brotli_decompressor;

mod integration_test;
mod util;

pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
use std::env;


use core::convert::From;
use std::vec::Vec;
use divans::BlockSwitch;
use divans::FeatureFlagSliceType;
use divans::CopyCommand;
use divans::LiteralBlockSwitch;
use divans::LiteralCommand;
use divans::LiteralPredictionModeNibble;
use divans::PredictionModeContextMap;
use divans::Command;
use divans::DictCommand;
use divans::BrotliResult;
use divans::Compressor;
use divans::Decompressor;
use divans::Speed;
use divans::CMD_BUFFER_SIZE;
use divans::free_cmd;
use divans::DivansCompressor;
use divans::DivansCompressorFactoryStruct;
use divans::DivansCompressorFactory;
use divans::DivansDecompressorFactory;
use divans::DivansDecompressorFactoryStruct;
use divans::interface::{ArithmeticEncoderOrDecoder, NewWithAllocator, StrideSelection};
use divans::Nop;
use std::fs::File;
use std::error::Error;
use std::io::{self,Write, Seek, SeekFrom, BufReader};

macro_rules! println_stderr(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);


use std::path::Path;
fn hex_string_to_vec(s: &str) -> Result<Vec<u8>, io::Error> {
    let mut output = Vec::with_capacity(s.len() >> 1);
    let mut rem = 0;
    let mut buf : u8 = 0;
    for byte in s.bytes() {
        if byte >= b'A' && byte <= b'F' {
            buf <<= 4;
            buf |= byte - b'A' + 10;
        } else if byte >= b'a' && byte <= b'f' {
            buf <<= 4;
            buf |= byte - b'a' + 10;
        } else if byte >= b'0' && byte <= b'9' {
            buf <<= 4;
            buf |= byte - b'0';
        } else if byte == b'\n'|| byte == b'\t'|| byte == b'\r' {
                continue;
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, s));
        }
        rem += 1;
        if rem == 2 {
            rem = 0;
            output.push(buf);
        }
    }
    if rem != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                  "String must have an even number of digits"));
    }
    Ok(output)
}
#[derive(Debug)]
pub struct ItemVec<Item:Sized+Default>(Vec<Item>);
impl<Item:Sized+Default> Default for ItemVec<Item> {
    fn default() -> Self {
        ItemVec(Vec::<Item>::new())
    }
}
impl<Item:Sized+Default> alloc::SliceWrapper<Item> for ItemVec<Item> {
    fn slice(&self) -> &[Item] {
        &self.0[..]
    }
}

impl<Item:Sized+Default> alloc::SliceWrapperMut<Item> for ItemVec<Item> {
    fn slice_mut(&mut self) -> &mut [Item] {
        &mut self.0[..]
    }
}

impl<Item:Sized+Default> core::ops::Index<usize> for ItemVec<Item> {
    type Output = Item;
    fn index(&self, index:usize) -> &Item {
        &self.0[index]
    }
}

impl<Item:Sized+Default> core::ops::IndexMut<usize> for ItemVec<Item> {

    fn index_mut(&mut self, index:usize) -> &mut Item {
        &mut self.0[index]
    }
}

#[derive(Default)]
struct ItemVecAllocator<Item:Sized+Default> {
    _item: core::marker::PhantomData<Item>,
}
impl<Item:Sized+Default+Clone> alloc::Allocator<Item> for ItemVecAllocator<Item> {
    type AllocatedMemory = ItemVec<Item>;
    fn alloc_cell(&mut self, size:usize) ->ItemVec<Item>{
        ItemVec(vec![Item::default();size])
    }
    fn free_cell(&mut self, _bv:ItemVec<Item>) {

    }
}
fn window_parse(s : &str) -> Result<i32, io::Error> {
    let window_vec : Vec<String> = s.split(' ').map(|s| s.to_string()).collect();
    if window_vec.is_empty() {
        panic!("Unexpected");    }
    if window_vec.len() < 2 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                       "window needs 1 argument"));
    }
    if window_vec[0] != "window" {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                       "first arg must be window followed by log window size"));
    }
    let expected_window_size = match window_vec[1].parse::<i32>() {
        Ok(el) => el,
        Err(msg) => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      msg.description()));
        }
    };
    Ok(expected_window_size)
}

#[cfg(not(feature="external-literal-probability"))]
fn deserialize_external_probabilities(probs: &std::vec::Vec<u8>) -> Result<FeatureFlagSliceType<ItemVec<u8>>, io::Error> {
    if !probs.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "To parse nonzero external probabiltiy flags, compile with feature flag external-literal-probability"));
    }
    Ok(FeatureFlagSliceType::<ItemVec<u8>>::default())
}
#[cfg(feature="external-literal-probability")]
fn deserialize_external_probabilities(probs: &std::vec::Vec<u8>) -> Result<FeatureFlagSliceType<ItemVec<u8>>, io::Error> {
    Ok(FeatureFlagSliceType::<ItemVec<u8>>(ItemVec(probs)))
}




fn command_parse(s : &str) -> Result<Option<Command<ItemVec<u8>>>, io::Error> {
    let command_vec : Vec<&str>= s.split(' ').collect();
    if command_vec.is_empty() {
        panic!("Unexpected");
    }
    let cmd = command_vec[0];
    if cmd == "window" {
            // FIXME validate
            return Ok(None);
    } else if cmd == "prediction" {
        if command_vec.len() < 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      "prediction needs 1 argument"));
        }
        let pmode = match command_vec[1] {
          "utf8" => LiteralPredictionModeNibble::utf8(),
          "sign" => LiteralPredictionModeNibble::signed(),
          "lsb6" => LiteralPredictionModeNibble::lsb6(),
          "msb6" => LiteralPredictionModeNibble::msb6(),
          _ => return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                         "invalid prediction mode; not {utf8,sign,lsb6,msb6}")),
        };
        let mut ret = PredictionModeContextMap::<ItemVec<u8> > {
            literal_prediction_mode: pmode,
            literal_context_map: ItemVec::<u8>::default(),
            distance_context_map: ItemVec::<u8>::default(),
        };
        if let Some((index, _)) = command_vec.iter().enumerate().find(|r| *r.1 == "lcontextmap") {
            for literal_context_map_val in command_vec.split_at(index + 1).1.iter() {
                match literal_context_map_val.parse::<i64>() {
                    Ok(el) => {
                        if el <= 255 && el >= 0 {
                            ret.literal_context_map.0.push(el as u8);
                        } else {
                            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                      literal_context_map_val.to_string() +
                                                      " literal context mp val must be u8"));
                        }
                    },
                    Err(_) => {
                        break;
                    },
                }
            }
        }
        if let Some((index, _)) = command_vec.iter().enumerate().find(|r| *r.1 == "dcontextmap") {
            for distance_context_map_val in command_vec.split_at(index + 1).1.iter() {
                match distance_context_map_val.parse::<i64>() {
                    Ok(el) => {
                        if el <= 255 && el >= 0 {
                            ret.distance_context_map.0.push(el as u8);
                        } else {
                            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                      distance_context_map_val.to_string() +
                                                      " distance context map val must be u8"));
                        }
                    },
                    Err(_) => {
                        break;
                    },
                }
            }
        }
        return Ok(Some(Command::PredictionMode(ret)));
    } else if cmd == "ctype" || cmd == "ltype" || cmd == "dtype" {
        if command_vec.len() != 2 && (command_vec.len() != 3 || cmd != "ltype") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      "*type needs 1 argument"));
        }
        let block_type = match command_vec[1].parse::<u32>() {
            Ok(el) => el as u8,
            Err(msg) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          msg.description()));
            }
        };
        return Ok(Some(match cmd.chars().next().unwrap() {
            'c' => Command::BlockSwitchCommand(BlockSwitch::new(block_type)),
            'd' => Command::BlockSwitchDistance(BlockSwitch::new(block_type)),
            'l' => Command::BlockSwitchLiteral(LiteralBlockSwitch::new(block_type,
                  if command_vec.len() < 2 {
                     0
                  } else {
                     match command_vec[2].parse::<u32>() {
                         Ok(stride) => {
                             if stride > 8 {
                                 return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                           "Strude must be <= 8"));
                                 
                             }
                             stride as u8
                         },
                         Err(msg) => {
                             return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                       msg.description()));
                         }
                     }
                  }
            )),
            _ => panic!("Logic error: already checked for valid command"),
        }));
    } else if cmd == "copy" {
        if command_vec.len() < 4 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      "copy needs 4 arguments"));
        }
        let expected_len = match command_vec[1].parse::<u32>() {
            Ok(el) => el,
            Err(msg) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          msg.description()));
            }
        };
        if command_vec[2] != "from" {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      s.to_string() + "needs a from statement in the 2nd arg"));
        }
        let distance = match command_vec[3].parse::<u32>() {
            Ok(el) => el,
            Err(msg) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          msg.description()));
            }
        };
        if expected_len == 0 {
           return Ok(None);
        }
        return Ok(Some(Command::Copy(CopyCommand{distance:distance, num_bytes:expected_len})));
    } else if cmd == "dict" {
        if command_vec.len() < 6 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      "dict needs 6+ arguments"));
        }
        let expected_len = match command_vec[1].parse::<u32>() {
            Ok(el) => el,
            Err(msg) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          msg.description()));
            }
        } as u8;
        if command_vec[2] != "word" {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      s.to_string() + " needs a word after the expected len"));
        }
        let word_id : Vec<String> = command_vec[3].split(',').map(|s| s.to_string()).collect();
        if word_id.len() != 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      s.to_string() + " needs a comma separated word value"));
        }
        let word_len = match word_id[0].parse::<u32>() {
            Ok(el) => el,
            Err(msg) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          msg.description()));
            }
        } as u8;
        let word_index = match word_id[1].parse::<u32>() {
            Ok(el) => el,
            Err(msg) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          msg.description()));
            }
        };
        for index in 5..command_vec.len() {
            if command_vec[index - 1] == "func" {
                let transform = match command_vec[index].parse::<u32>() {
                    Ok(el) => el,
                    Err(msg) => {
                        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                  msg.description()));
                    }
                } as u8;
                return Ok(Some(Command::Dict(DictCommand{
                    word_size:word_len,
                    word_id:word_index,
                    empty:0,
                    final_size:expected_len,
                    transform:transform
                })));
            }
        }
    } else if cmd == "insert"{
        if command_vec.len() < 3 {
            if command_vec.len() == 2 && command_vec[1] == "0" {
                return Ok(None);
            }
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          String::from("insert needs 3 arguments, not (") + s + ")"));
        }
        let expected_len = match command_vec[1].parse::<usize>() {
            Ok(el) => el,
            Err(msg) => {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                              msg.description()));
            }
        };
        if expected_len ==  0 {
            return Ok(None);
        }
        let data = try!(hex_string_to_vec(command_vec[2]));
        let probs = if command_vec.len() > 3 {
            let prob = try!(hex_string_to_vec(command_vec[3]));
            assert!(prob.len() == expected_len * 8);
            prob
        } else {
            Vec::<u8>::new()
        };

        if data.len() != expected_len {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      String::from("Length does not match ") + s))
        }
        match deserialize_external_probabilities(&probs) {
            Ok(external_probs) => {
                return Ok(Some(Command::Literal(LiteralCommand{
                        data:ItemVec(data),
                        prob:external_probs,
                         })));
            },
            Err(external_probs_err) => {
                return Err(external_probs_err);
            },
        }
    }
    Err(io::Error::new(io::ErrorKind::InvalidInput,
                       String::from("Unknown ") + s))
}

fn recode_cmd_buffer<RState:divans::interface::Compressor,
                     Writer:std::io::Write,>(state: &mut RState,
                                             cmd_buffer:&[Command<ItemVec<u8>>],
                                             w: &mut Writer,
                                             output_scratch:&mut [u8]) -> Result<usize, io::Error> {
    let mut i_processed_index = 0usize;
    let mut o_processed_index = 0usize;
    let mut ret = 0usize;
    while i_processed_index < cmd_buffer.len() {
        match state.encode_commands(cmd_buffer,
                           &mut i_processed_index,
                           output_scratch,
                           &mut o_processed_index) {
            BrotliResult::ResultSuccess => {
                assert_eq!(i_processed_index, cmd_buffer.len());
                break;
            },
            BrotliResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                if let Err(x) = w.write_all(output_scratch.split_at(o_processed_index).0) {
                    return Err(x);
                }
                ret += o_processed_index;
                o_processed_index = 0;
            }
            BrotliResult::NeedsMoreInput => {
                assert_eq!(i_processed_index, cmd_buffer.len());
                break;
//                return Err(io::Error::new(io::ErrorKind::InvalidInput,
//                               "Unknown Error Type: Needs more input (Partial command?)"));
            }
            BrotliResult::ResultFailure => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                               "Brotli Failure to recode file"));
            }
        }
    }
    if let Err(x) = w.write_all(output_scratch.split_at(o_processed_index).0) {
        return Err(x);
    }
    ret += o_processed_index;
    Ok(ret)
}

fn recode_inner<Reader:std::io::BufRead,
                Writer:std::io::Write,
                RingBuffer:core::default::Default+SliceWrapper<u8>+SliceWrapperMut<u8>>(
    r:&mut Reader,
    w:&mut Writer) -> io::Result<()> {
    let mut buffer = String::new();
    let mut obuffer = [0u8;65_536];
    let mut ibuffer:[Command<ItemVec<u8>>; CMD_BUFFER_SIZE] = [Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop()];

    let mut i_read_index = 0usize;
    let mut state = divans::DivansRecodeState::<RingBuffer>::default();
    loop {
        buffer.clear();
        match r.read_line(&mut buffer) {
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e)
            },
            Ok(count) => {
                if i_read_index == ibuffer.len() || count == 0 {
                    recode_cmd_buffer(&mut state, ibuffer.split_at(i_read_index).0, w,
                                      &mut obuffer[..]).unwrap();
                    i_read_index = 0
                }
                if count == 0 {
                    break;
                }
                let line = buffer.trim().to_string();
                match command_parse(&line).unwrap() {
                    None => {},
                    Some(c) => {
                        ibuffer[i_read_index] = c;
                        i_read_index += 1;
                    }
                }
            }
        }
    }
    loop {
        let mut o_processed_index = 0;
        match state.flush(&mut obuffer[..],
                          &mut o_processed_index) {
            BrotliResult::ResultSuccess => {
                if o_processed_index != 0 {
                    if let Err(x) = w.write_all(obuffer.split_at(o_processed_index).0) {
                        return Err(x);
                    }
                }
                break;
            },
            BrotliResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                if let Err(x) = w.write_all(obuffer.split_at(o_processed_index).0) {
                    return Err(x);
                }
            }
            BrotliResult::NeedsMoreInput => {
                panic!("Unreasonable demand: no input avail in this code path");
            }
            BrotliResult::ResultFailure => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                               "Brotli Failure to recode file"));
            }
        }
    }

    Ok(())
}

fn allowed_command(_cmd: &Command<ItemVec<u8>>, _last_literal_switch: &mut divans::LiteralBlockSwitch) -> bool {
    true
}

fn compress_inner<Reader:std::io::BufRead,
                  Writer:std::io::Write,
                  Encoder:ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                  AllocU8:alloc::Allocator<u8>,
                  AllocU32:alloc::Allocator<u32>,
                  AllocCDF2:alloc::Allocator<divans::CDF2>,
                  AllocCDF16:alloc::Allocator<divans::DefaultCDF16>>(
    mut state: DivansCompressor<Encoder,
                                AllocU8,
                                AllocU32,
                                AllocCDF2,
                                AllocCDF16>,
    r:&mut Reader,
    w:&mut Writer) -> io::Result<()> {
    let mut buffer = String::new();
    let mut obuffer = [0u8;65_536];
    let mut ibuffer:[Command<ItemVec<u8>>; CMD_BUFFER_SIZE] = [Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop(),
                                                           Command::<ItemVec<u8>>::nop()];

    let mut i_read_index = 0usize;
    let mut last_literal_switch = LiteralBlockSwitch::new(0, 0);
    let mut m8 = ItemVecAllocator::<u8>::default();
    loop {
        buffer.clear();
        match r.read_line(&mut buffer) {
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e)
            },
            Ok(count) => {
                if i_read_index == ibuffer.len() || count == 0 {
                    try!(recode_cmd_buffer(&mut state, ibuffer.split_at(i_read_index).0, w,
                                               &mut obuffer[..]));

                    for item in &mut ibuffer {
                       free_cmd(item, &mut m8);
                    }
                    i_read_index = 0
                }
                if count == 0 {
                    break;
                }
                let line = buffer.trim().to_string();
                match try!(command_parse(&line)) {
                    None => {},
                    Some(c) => {
                        if allowed_command(&c,
                                           &mut last_literal_switch) {
                            ibuffer[i_read_index] = c;
                            i_read_index += 1;
                        }
                    }
                }
            }
        }
    }
    loop {
        let mut o_processed_index = 0;
        match state.flush(&mut obuffer[..],
                          &mut o_processed_index) {
            BrotliResult::ResultSuccess => {
                if o_processed_index != 0 {
                    if let Err(x) = w.write_all(obuffer.split_at(o_processed_index).0) {
                        return Err(x);
                    }
                }
                break;
            },
            BrotliResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                if let Err(x) = w.write_all(obuffer.split_at(o_processed_index).0) {
                    return Err(x);
                }
            }
            BrotliResult::NeedsMoreInput => {
                panic!("Unreasonable demand: no input avail in this code path");
            }
            BrotliResult::ResultFailure => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                               "Brotli Failure to recode file"));
            }
        }
    }
    Ok(())
}
fn compress_raw_inner<Compressor: divans::interface::Compressor,
                      Reader:std::io::Read,
                      Writer:std::io::Write>(r:&mut Reader,
                                             w:&mut Writer,
                                             mut ibuffer: <ItemVecAllocator<u8> as Allocator<u8>>::AllocatedMemory,
                                             mut obuffer: <ItemVecAllocator<u8> as Allocator<u8>>::AllocatedMemory,
                                             mut compress_state: Compressor,
                                             free_state: &mut Fn(Compressor)->ItemVecAllocator<u8>) -> io::Result<()> {
    let mut ilim = 0usize;
    let mut idec_index = 0usize;
    let mut olim = 0usize;
    let mut oenc_index = 0usize;
    loop {
        if idec_index == ilim {
            idec_index = 0;
            match r.read(ibuffer.slice_mut()) {
                Ok(count) => {
                    ilim = count;
                    if ilim == 0 {
                        break; // we're done reading the input
                    }
                },
                Err(e) => {
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    let mut m8 = free_state(compress_state);
                    m8.free_cell(ibuffer);
                    m8.free_cell(obuffer);
                    return Err(e);
                }
            }
        }
        if idec_index != ilim {
            match compress_state.encode(ibuffer.slice().split_at(ilim).0,
                               &mut idec_index,
                               obuffer.slice_mut().split_at_mut(oenc_index).1,
                               &mut olim) {
                BrotliResult::ResultSuccess => continue,
                BrotliResult::ResultFailure => {
                    let mut m8 = free_state(compress_state);
                    m8.free_cell(ibuffer);
                    m8.free_cell(obuffer);
                    return Err(io::Error::new(io::ErrorKind::Other,
                               "Failure encoding brotli"));
                },
                BrotliResult::NeedsMoreInput | BrotliResult::NeedsMoreOutput => {},
            }
        }
        while oenc_index != olim {
            match w.write(&obuffer.slice()[oenc_index..olim]) {
                Ok(count) => oenc_index += count,
                Err(e) => {
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    let mut m8 = free_state(compress_state);
                    m8.free_cell(ibuffer);
                    m8.free_cell(obuffer);
                    return Err(e);
                }
            }
        }
        olim = 0;
        oenc_index = 0;
    }
    let mut done = false;
    while !done {
        match compress_state.flush(obuffer.slice_mut().split_at_mut(oenc_index).1,
                          &mut olim) {
            BrotliResult::ResultSuccess => done = true,
            BrotliResult::ResultFailure => {
                let mut m8 = free_state(compress_state);
                m8.free_cell(ibuffer);
                m8.free_cell(obuffer);
                return Err(io::Error::new(io::ErrorKind::Other,
                                          "Failure encoding brotli"));
            },
            BrotliResult::NeedsMoreInput => {
                done = true; // should we assert here?
                assert!(false);// we are flushing--should be success here
            }
            BrotliResult::NeedsMoreOutput => {
            }
        }
        while oenc_index != olim {
            match w.write(&obuffer.slice()[oenc_index..olim]) {
                Ok(count) => oenc_index += count,
                Err(e) => {
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    let mut m8 = free_state(compress_state);
                    m8.free_cell(ibuffer);
                    m8.free_cell(obuffer);
                    return Err(e);
                }
            }
        }
        oenc_index = 0;
        olim = 0;
    }
    let mut m8 = free_state(compress_state);
    m8.free_cell(ibuffer);
    m8.free_cell(obuffer);
    Ok(())
}


type BrotliFactory = divans::BrotliDivansHybridCompressorFactory<ItemVecAllocator<u8>,
                                                         ItemVecAllocator<u16>,
                                                         ItemVecAllocator<u32>,
                                                         ItemVecAllocator<i32>,
                                                         ItemVecAllocator<brotli::enc::command::Command>,
                                                         ItemVecAllocator<divans::CDF2>,
                                                         ItemVecAllocator<divans::DefaultCDF16>,
                                                         ItemVecAllocator<brotli::enc::util::floatX>,
                                                         ItemVecAllocator<brotli::enc::vectorization::Mem256f>,
                                                         ItemVecAllocator<brotli::enc::histogram::HistogramLiteral>,
                                                         ItemVecAllocator<brotli::enc::histogram::HistogramCommand>,
                                                         ItemVecAllocator<brotli::enc::histogram::HistogramDistance>,
                                                         ItemVecAllocator<brotli::enc::cluster::HistogramPair>,
                                                         ItemVecAllocator<brotli::enc::histogram::ContextType>,
                                                         ItemVecAllocator<brotli::enc::entropy_encode::HuffmanTree>>;

fn compress_raw<Reader:std::io::Read,
                Writer:std::io::Write>(
    r:&mut Reader,
    w:&mut Writer,
    dynamic_context_mixing: Option<u8>,
    literal_adaptation_speed: Option<Speed>,
    do_context_map: bool,
    force_stride_value:StrideSelection,
    opt_window_size:Option<i32>,
    buffer_size: usize,
    use_brotli: bool) -> io::Result<()> {
    let window_size = opt_window_size.unwrap_or(21);
    let mut m8 = ItemVecAllocator::<u8>::default();
    let ibuffer = m8.alloc_cell(buffer_size);
    let obuffer = m8.alloc_cell(buffer_size);
    if use_brotli {
        let state =BrotliFactory::new(
            m8,
            ItemVecAllocator::<u32>::default(),
            ItemVecAllocator::<divans::CDF2>::default(),
            ItemVecAllocator::<divans::DefaultCDF16>::default(),
            window_size as usize,
            dynamic_context_mixing.unwrap_or(0),
            literal_adaptation_speed,
            do_context_map,
            force_stride_value,
            (ItemVecAllocator::<u8>::default(),
             ItemVecAllocator::<u16>::default(),
             ItemVecAllocator::<i32>::default(),
             ItemVecAllocator::<brotli::enc::command::Command>::default(),
             ItemVecAllocator::<brotli::enc::util::floatX>::default(),
             ItemVecAllocator::<brotli::enc::vectorization::Mem256f>::default(),
             ItemVecAllocator::<brotli::enc::histogram::HistogramLiteral>::default(),
             ItemVecAllocator::<brotli::enc::histogram::HistogramCommand>::default(),
             ItemVecAllocator::<brotli::enc::histogram::HistogramDistance>::default(),
             ItemVecAllocator::<brotli::enc::cluster::HistogramPair>::default(),
             ItemVecAllocator::<brotli::enc::histogram::ContextType>::default(),
             ItemVecAllocator::<brotli::enc::entropy_encode::HuffmanTree>::default()),
        );
        let mut free_closure = |state_to_free:<BrotliFactory as DivansCompressorFactory<ItemVecAllocator<u8>, ItemVecAllocator<u32>, ItemVecAllocator<divans::CDF2>, ItemVecAllocator<divans::DefaultCDF16>>>::ConstructedCompressor| ->ItemVecAllocator<u8> {state_to_free.free().0};
        compress_raw_inner(r, w,
                           ibuffer, obuffer,
                           state,
                           &mut free_closure)
    } else {
        type Factory = DivansCompressorFactoryStruct<
                ItemVecAllocator<u8>,
                ItemVecAllocator<divans::CDF2>,
                ItemVecAllocator<divans::DefaultCDF16>>;
        let state =Factory::new(
            m8,
            ItemVecAllocator::<u32>::default(),
            ItemVecAllocator::<divans::CDF2>::default(),
            ItemVecAllocator::<divans::DefaultCDF16>::default(),
            window_size as usize,
            dynamic_context_mixing.unwrap_or(0),
            literal_adaptation_speed, do_context_map, force_stride_value, (),
        );
        let mut free_closure = |state_to_free:<Factory as DivansCompressorFactory<ItemVecAllocator<u8>, ItemVecAllocator<u32>, ItemVecAllocator<divans::CDF2>, ItemVecAllocator<divans::DefaultCDF16>>>::ConstructedCompressor| ->ItemVecAllocator<u8> {state_to_free.free().0};
        compress_raw_inner(r, w,
                           ibuffer, obuffer,
                           state,
                           &mut free_closure)
    }
}
fn compress_ir<Reader:std::io::BufRead,
            Writer:std::io::Write>(
    r:&mut Reader,
    w:&mut Writer,
    dynamic_context_mixing: Option<u8>,
    literal_adaptation_speed: Option<Speed>,
    do_context_map: bool,
    force_stride_value:StrideSelection) -> io::Result<()> {
    let window_size : i32;
    let mut buffer = String::new();
    loop {
        match r.read_line(&mut buffer) {
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            },
            Ok(_) => {
                let line = buffer.trim().to_string();
                window_size = try!(window_parse(&line));
                break;
            }
        }
    }
    let state =DivansCompressorFactoryStruct::<ItemVecAllocator<u8>,
                                  ItemVecAllocator<divans::CDF2>,
                                  ItemVecAllocator<divans::DefaultCDF16>>::new(
        ItemVecAllocator::<u8>::default(),
        ItemVecAllocator::<u32>::default(),
        ItemVecAllocator::<divans::CDF2>::default(),
        ItemVecAllocator::<divans::DefaultCDF16>::default(),
        window_size as usize,
        dynamic_context_mixing.unwrap_or(0),
        literal_adaptation_speed,
        do_context_map,
        force_stride_value,
        (),
    );
    compress_inner(state, r, w)
}

fn zero_slice(sl: &mut [u8]) -> usize {
    for v in sl.iter_mut() {
        *v = 0u8;
    }
    sl.len()
}

fn decompress<Reader:std::io::Read,
              Writer:std::io::Write> (r:&mut Reader,
                                      w:&mut Writer,
                                      buffer_size: usize) -> io::Result<()> {
    let mut m8 = ItemVecAllocator::<u8>::default();
    let mut ibuffer = m8.alloc_cell(buffer_size);
    let mut obuffer = m8.alloc_cell(buffer_size);
    let mut state = DivansDecompressorFactoryStruct::<ItemVecAllocator<u8>,
                                         ItemVecAllocator<divans::CDF2>,
                                         ItemVecAllocator<divans::DefaultCDF16>>::new(m8,
                                                                ItemVecAllocator::<divans::CDF2>::default(),
    ItemVecAllocator::<divans::DefaultCDF16>::default());
    let mut input_offset = 0usize;
    let mut input_end = 0usize;
    let mut output_offset = 0usize;

    loop {
        match state.decode(ibuffer.slice().split_at(input_end).0,
                           &mut input_offset,
                           obuffer.slice_mut(),
                           &mut output_offset) {
            BrotliResult::ResultSuccess => {
                break
            },
            BrotliResult::ResultFailure => {
                let mut m8 = state.free().0;
                m8.free_cell(ibuffer);
                m8.free_cell(obuffer);
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          "Error within Divans File"));
            },
            BrotliResult::NeedsMoreOutput => {
                let mut output_written = 0;
                while output_written != output_offset {
                    // flush buffer, if any
                    match w.write(obuffer.slice().split_at(output_written).1.split_at(output_offset - output_written).0) {
                        Ok(count) => output_written += count,
                        Err(e) => {
                            if e.kind() == io::ErrorKind::Interrupted {
                                continue;
                            }
                            let mut m8 = state.free().0;
                            m8.free_cell(ibuffer);
                            m8.free_cell(obuffer);
                            return Err(e);
                        }
                    }
                }
                output_offset = 0; // reset buffer
            },
            BrotliResult::NeedsMoreInput => {
                if input_offset == input_end {
                    // We have exhausted all the available input, so we can reset the cursors.
                    input_offset = 0;
                    input_end = 0;
                }
                loop {
                    match r.read(ibuffer.slice_mut().split_at_mut(input_end).1) {
                        Ok(size) => {
                            if size == 0 {
                                //println_stderr!("End of file.  Feeding zero's.\n");
                                let len = zero_slice(ibuffer.slice_mut().split_at_mut(input_end).1);
                                input_end += len;
                                //return Err(io::Error::new(
                                //    io::ErrorKind::UnexpectedEof,
                                //    "Divans file invalid: didn't have a terminator marker"));
                            } else {
                                input_end += size;
                            }
                            break
                        },
                        Err(e) => {
                            if e.kind() == io::ErrorKind::Interrupted {
                                continue;
                            }
                            let mut m8 = state.free().0;
                            m8.free_cell(ibuffer);
                            m8.free_cell(obuffer);
                            return Err(e);
                        },
                    }
                }
            },
        }
    }
    let mut output_written = 0;
    while output_written != output_offset {
        // flush buffer, if any
        match w.write(obuffer.slice().split_at(output_written).1.split_at(output_offset - output_written).0) {
            Ok(count) => output_written += count,
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                let mut m8 = state.free().0;
                m8.free_cell(ibuffer);
                m8.free_cell(obuffer);
                return Err(e);
            }
        }
    }
    let mut m8 = state.free().0;
    m8.free_cell(ibuffer);
    m8.free_cell(obuffer);
    Ok(())
}

fn recode<Reader:std::io::BufRead,
          Writer:std::io::Write>(
    mut r:&mut Reader,
    mut w:&mut Writer) -> io::Result<()> {
    let window_size : i32;
    let mut buffer = String::new();
    loop {
        match r.read_line(&mut buffer) {
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            },
            Ok(_) => {
                let line = buffer.trim().to_string();
                window_size = window_parse(&line).unwrap();
                break;
            }
        }
    }
    match window_size {
        10 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer10>(&mut r,
                                               &mut w),
        11 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer11>(&mut r,
                                               &mut w),
        12 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer12>(&mut r,
                                         &mut w),
        13 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer13>(&mut r,
                                         &mut w),
        14 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer14>(&mut r,
                                         &mut w),
        15 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer15>(&mut r,
                                         &mut w),
        16 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer16>(&mut r,
                                         &mut w),
        17 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer17>(&mut r,
                                         &mut w),
        18 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer18>(&mut r,
                                         &mut w),
        19 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer19>(&mut r,
                                         &mut w),
        20 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer20>(&mut r,
                                         &mut w),
        21 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer21>(&mut r,
                                         &mut w),
        22 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer22>(&mut r,
                                         &mut w),
        23 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer23>(&mut r,
                                         &mut w),
        24 => recode_inner::<Reader,
                     Writer,
                     util::StaticHeapBuffer24>(&mut r,
                                         &mut w),
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "Window size must be <=24 >= 10")),
    }
}
fn main() {
    let mut do_compress = false;
    let mut raw_compress = false;
    let mut do_recode = false;
    let mut filenames = [std::string::String::new(), std::string::String::new()];
    let mut num_benchmarks = 1;
    let mut use_context_map = false;
    let mut use_brotli = true;
    let mut force_stride_value = StrideSelection::PriorDisabled;
    let mut literal_adaptation: Option<Speed> = None;
    let mut window_size: Option<i32> = None;
    let mut dynamic_context_mixing: Option<u8> = None;
    let mut buffer_size:usize = 65_536;
    if env::args_os().len() > 1 {
        let mut first = true;
        for argument in env::args() {
            if first {
                first = false;
                continue;
            }
            if argument == "-d" {
                continue;
            }
            if argument.starts_with("-bs") {
                buffer_size = argument.trim_matches(
                    '-').trim_matches(
                    'b').trim_matches('s').parse::<usize>().unwrap();
                continue;
            }
            if argument.starts_with("-b") {
                num_benchmarks = argument.trim_matches(
                    '-').trim_matches(
                    'b').parse::<usize>().unwrap();
                continue;
            }
            if argument == "--recode" {
                do_recode = true;
                continue;
            }
            if argument.starts_with("-w") || argument.starts_with("-window=") {
                let fs = argument.trim_matches(
                    '-').trim_matches(
                    'w').trim_matches(
                    'i').trim_matches(
                    'n').trim_matches(
                    'd').trim_matches(
                    'o').trim_matches(
                    'w').trim_matches(
                    '=').parse::<i32>().unwrap();
                window_size=Some(fs);
                continue;
            }
            if argument.starts_with("-stride") || argument == "-s" {
                if argument.starts_with("-stride=") {
                    let fs = argument.trim_matches(
                        '-').trim_matches(
                        's').trim_matches(
                        't').trim_matches(
                        'r').trim_matches(
                        'i').trim_matches(
                        'd').trim_matches(
                        'e').trim_matches(
                        '=').parse::<u32>().unwrap();
                    force_stride_value = match fs {
                        0 => panic!("Omit -s to set avoid stride=0"),
                        1 => StrideSelection::Stride1,
                        2 => StrideSelection::Stride2,
                        3 => StrideSelection::Stride3,
                        4 => StrideSelection::Stride4,
                        5 => StrideSelection::Stride5,
                        6 => StrideSelection::Stride6,
                        7 => StrideSelection::Stride7,
                        8 => StrideSelection::Stride8,
                        _ => panic!("Force stride must be <= 8"),
                    }
                } else {
                    match force_stride_value {
                        StrideSelection::PriorDisabled => force_stride_value = StrideSelection::UseBrotliRec,
                        _ => {}, // already set
                    }
                }
                continue;
            }
            if argument == "-cm" || argument == "-contextmap" {
                use_context_map = true;
                continue;
            }
            if argument == "-i" {
                do_compress = true;
                continue;
            }
            if argument == "-c" {
                do_compress = true;
                raw_compress = true;
                continue;
            }
            if argument == "-nobrotli" {
                do_compress = true;
                raw_compress = true;
                use_brotli = false;
                continue;
            }
            if argument.starts_with("-mixing=") {
                dynamic_context_mixing = Some(argument.trim_matches(
                    '-').trim_matches(
                    'm').trim_matches(
                    'i').trim_matches(
                    'x').trim_matches(
                    'i').trim_matches(
                    'n').trim_matches(
                    'g').trim_matches(
                    '=').parse::<i32>().unwrap() as u8);
                continue
            }
            if argument.starts_with("-speed=") {
                literal_adaptation = Some(argument.trim_matches(
                    '-').trim_matches(
                    's').trim_matches(
                    'p').trim_matches(
                    'e').trim_matches(
                    'e').trim_matches(
                    'd').trim_matches(
                    '=').parse::<Speed>().unwrap());
                continue

            }
            if argument == "-h" || argument == "-help" || argument == "--help" {
                println_stderr!("Compression: divans {{-c [raw_input_file] | -i [ir_file]}} [output_file]");
                println_stderr!("Decompression: divans [input_file] [output_file]");
                return;
            }
            if argument == "-v" || argument == "-version" || argument == "--version" {
                println_stderr!("Divans {}", sha());
                return;
            }
            if filenames[0] == "" {
                filenames[0] = argument.clone();
                continue;
            }
            if filenames[1] == "" {
                filenames[1] = argument.clone();
                continue;
            }
            panic!("Unknown Argument {:}", argument);
        }
        if filenames[0] != "" {
            let mut input = match File::open(&Path::new(&filenames[0])) {
                Err(why) => panic!("couldn't open {:}\n{:}", filenames[0], why),
                Ok(file) => file,
            };
            if filenames[1] != "" {
                let mut output = match File::create(&Path::new(&filenames[1])) {
                    Err(why) => panic!("couldn't open file for writing: {:}\n{:}", filenames[1], why),
                    Ok(file) => file,
                };
                for i in 0..num_benchmarks {
                    if do_compress && !raw_compress {
                        let mut buffered_input = BufReader::new(input);
                        match compress_ir(&mut buffered_input, &mut output, dynamic_context_mixing.clone(), literal_adaptation.clone(), use_context_map, force_stride_value) {
                            Ok(_) => {}
                            Err(e) => panic!("Error {:?}", e),
                        }
                        input = buffered_input.into_inner();
                    } else if do_compress {
                        match compress_raw(&mut input, &mut output, dynamic_context_mixing.clone(), literal_adaptation.clone(), use_context_map, force_stride_value, window_size, buffer_size, use_brotli) {
                            Ok(_) => {}
                            Err(e) => panic!("Error {:?}", e),
                        }
                    } else if do_recode {
                        let mut buffered_input = BufReader::new(input);
                        recode(&mut buffered_input,
                               &mut output).unwrap();
                        input = buffered_input.into_inner();
                    } else {
                        match decompress(&mut input, &mut output, buffer_size) {
                            Ok(_) => {}
                            Err(e) => panic!("Error {:?}", e),
                        }
                    }
                    if i + 1 != num_benchmarks {
                        input.seek(SeekFrom::Start(0)).unwrap();
                        output.seek(SeekFrom::Start(0)).unwrap();
                    }
                }
                drop(output);
            } else {
                assert_eq!(num_benchmarks, 1);
                if do_compress && !raw_compress {
                    let mut buffered_input = BufReader::new(input);
                    match compress_ir (&mut buffered_input, &mut io::stdout(), dynamic_context_mixing.clone(), literal_adaptation, use_context_map, force_stride_value) {
                        Ok(_) => {}
                        Err(e) => panic!("Error {:?}", e),
                    }
                } else if do_compress {
                    match compress_raw (&mut input, &mut io::stdout(), dynamic_context_mixing.clone(), literal_adaptation, use_context_map, force_stride_value, window_size, buffer_size, use_brotli) {
                        Ok(_) => {}
                        Err(e) => panic!("Error {:?}", e),
                    }
                } else if do_recode {
                    let mut buffered_input = BufReader::new(input);
                    recode(&mut buffered_input,
                           &mut io::stdout()).unwrap()
                } else {
                    match decompress(&mut input, &mut io::stdout(), buffer_size) {
                        Ok(_) => {}
                        Err(e) => panic!("Error {:?}", e),
                    }
                }
            }
        } else {
            assert_eq!(num_benchmarks, 1);
            if do_compress && !raw_compress {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                match compress_ir(&mut stdin, &mut io::stdout(), dynamic_context_mixing.clone(), literal_adaptation, use_context_map, force_stride_value) {
                    Ok(_) => return,
                    Err(e) => panic!("Error {:?}", e),
                }
            } else if do_compress {
                match compress_raw(&mut std::io::stdin(), &mut io::stdout(), dynamic_context_mixing.clone(), literal_adaptation, use_context_map, force_stride_value, window_size, buffer_size, use_brotli) {
                    Ok(_) => return,
                    Err(e) => panic!("Error {:?}", e),
                }
            } else if do_recode {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                recode(&mut stdin,
                       &mut io::stdout()).unwrap()
            } else {
                match decompress(&mut io::stdin(), &mut io::stdout(), buffer_size) {
                    Ok(_) => return,
                    Err(e) => panic!("Error {:?}", e),
                }
            }
        }
    } else {
        assert_eq!(num_benchmarks, 1);
        match decompress(&mut io::stdin(), &mut io::stdout(), buffer_size) {
            Ok(_) => return,
            Err(e) => panic!("Error {:?}", e),
        }
    }
}
