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
#![cfg_attr(feature="benchmark", feature(test))]

extern crate core;
#[cfg(feature="no-stdlib-rlib")]
extern crate divans_no_stdlib as divans;
#[cfg(not(feature="no-stdlib-rlib"))]
extern crate divans;
extern crate brotli;
extern crate alloc_no_stdlib as alloc;
use brotli::TransformDictionaryWord;
use brotli::dictionary::{kBrotliMaxDictionaryWordLength, kBrotliDictionary,
                                      kBrotliDictionaryOffsetsByLength};

include!(concat!(env!("OUT_DIR"), "/version.rs"));


#[cfg(test)]
extern crate brotli as brotli_decompressor;

mod integration_test;
mod benchmark;
mod util;

pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
use std::env;
use std::error;

use core::convert::From;
use std::vec::Vec;
use divans::StaticCommand;
use divans::BlockSwitch;
use divans::FeatureFlagSliceType;
use divans::CopyCommand;
use divans::LiteralBlockSwitch;
use divans::LiteralCommand;
use divans::LiteralPredictionModeNibble;
use divans::PredictionModeContextMap;
use divans::Command;
use divans::DictCommand;
use divans::DivansResult;
use divans::DivansOutputResult;
use divans::Compressor;
use divans::Decompressor;
use divans::Speed;
use divans::ir_optimize;
use divans::CMD_BUFFER_SIZE;

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

fn is_divans(header:&[u8]) -> bool {
    if header.len() < divans::MAGIC_NUMBER.len() {
        return false;
    }
    for (a, b) in header[..divans::MAGIC_NUMBER.len()].iter().zip(divans::MAGIC_NUMBER[..].iter()) {
        if a != b {
            return false;
        }
    }
    return true;
}

use std::path::Path;



#[derive(Copy,Clone,Debug)]
struct DivansErrMsg(pub divans::ErrMsg);
impl core::fmt::Display for DivansErrMsg {
    fn fmt(&self, f:&mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        <divans::ErrMsg as core::fmt::Debug>::fmt(&self.0, f)
    }
}

impl error::Error for DivansErrMsg {
    fn description(&self) -> &str {
        "Divans error"
    }
    fn cause(&self) -> Option<&error::Error> {None}
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
        //eprint!("A:{}\n", size);
        ItemVec(vec![Item::default();size])
    }
    fn free_cell(&mut self, _bv:ItemVec<Item>) {
        //eprint!("F:{}\n", _bv.slice().len());
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



#[cfg(not(feature="external-literal-probability"))]
fn deserialize_ref_probabilities(probs: brotli::SliceOffset) -> Result<FeatureFlagSliceType<brotli::SliceOffset>, io::Error> {
    if probs.len() != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "To parse nonzero external probabiltiy flags, compile with feature flag external-literal-probability"));
    }
    Ok(FeatureFlagSliceType::<brotli::SliceOffset>::default())
}
#[cfg(feature="external-literal-probability")]
fn deserialize_ref_probabilities(probs: brotli::SliceOffset) -> Result<FeatureFlagSliceType<brotli::SliceOffset>, io::Error> {
    Ok(FeatureFlagSliceType::<brotli::SliceOffset>(ItemVec(probs)))
}

fn expand_or_extend(output: &mut Vec<u8>, sl: &[u8], cursor:&mut usize) {
    if *cursor == output.len() {
        output.extend(sl);
    } else {
        if *cursor + sl.len() > output.len() {
            output.resize(*cursor + sl.len(), 0);
        }
        output.split_at_mut(*cursor).1.split_at_mut(sl.len()).0.clone_from_slice(sl);
    }
    *cursor += sl.len();
}

fn is_pred_mode(s:&str) -> bool {
    s.starts_with("prediction ")
}

fn command_parse(s : &str, literal_buffer: &mut Vec<u8>, cursor: &mut usize,
                 strict_ring_buffer: bool) -> Result<Option<Command<brotli::SliceOffset>>, io::Error> {
    let command_vec : Vec<&str>= s.split(' ').collect();
    if command_vec.is_empty() {
        panic!("Unexpected");
    }
    let cmd = command_vec[0];
    if cmd.starts_with("#") {
        return Ok(None);
    }
    if cmd.starts_with("//") {
        return Ok(None);
    }
    if cmd == "window" {
        assert!(!is_pred_mode(s));
        // FIXME validate
        return Ok(None);
    } else if is_pred_mode(s) {
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
            literal_context_map: ItemVec::<u8>::default(),
            predmode_speed_and_distance_context_map: ItemVec::<u8>::default(),
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
        ret.predmode_speed_and_distance_context_map.0.resize(PredictionModeContextMap::<ItemVec<u8> >::size_of_combined_array(0), 0u8);
        ret.set_literal_prediction_mode(pmode);
        if let Some((index, _)) = command_vec.iter().enumerate().find(|r| *r.1 == "dcontextmap") {
            for distance_context_map_val in command_vec.split_at(index + 1).1.iter() {
                match distance_context_map_val.parse::<i64>() {
                    Ok(el) => {
                        if el <= 255 && el >= 0 {
                            ret.predmode_speed_and_distance_context_map.0.push(el as u8);
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
        let mut mixing_values = [0;8192];
        if let Some((index, _)) = command_vec.iter().enumerate().find(|r| *r.1 == "mixingvalues") {
            let mut offset = 0usize;
            for mixing_val in command_vec.split_at(index + 1).1.iter() {
                match mixing_val.parse::<i64>() {
                    Ok(el) => {
                        if offset >= 8192 {
                            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                      mixing_val.to_string() +
                                                      "Must have no more than 512 mixing values"));
                        }
                        if el <= 8 && el >= 0 {
                            mixing_values[offset] = el as u8;
                            offset += 1;
                        } else {
                            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                      mixing_val.to_string() +
                                                      "Prior Strategy Mixing val must be 0 or 1"));
                        }
                    },
                    Err(_) => {
                        break;
                    },
                }
            }
        }
        ret.set_mixing_values(&mixing_values);
        
        let mut cm_stride_mix_speed = [[(0u16,0u16);2];3];
        let keys = [["cmspeedinc", "cmspeedmax"],
                    ["stspeedinc", "stspeedmax"],
                    ["mxspeedinc", "mxspeedmax"]];
        for (which_type, keypair) in keys.iter().enumerate() {
            for (incmx, key) in keypair.iter().enumerate() {
                if let Some((index, _)) = command_vec.iter().enumerate().find(|r| *r.1 == *key) {
                    for (index, speed_inc_val) in command_vec.split_at(index + 1).1.iter().enumerate() {
                        if index >= 2 {
                            break;
                        }
                        match speed_inc_val.parse::<u16>() {
                            Ok(el) => {
                                if el <= 16384 {
                                    if incmx == 0 {
                                        cm_stride_mix_speed[which_type][index].0 = el;
                                    } else {
                                        cm_stride_mix_speed[which_type][index].1 = el;
                                    }
                                } else {
                                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                              speed_inc_val.to_string() +
                                                              " speed inc val must be u16 <= 16384"));
                                }
                            },
                            Err(_) => {
                                break;
                            },
                        }
                    }
                }
            }
        }
        ret.set_context_map_speed(cm_stride_mix_speed[0]);
        ret.set_stride_context_speed(cm_stride_mix_speed[1]);
        ret.set_combined_stride_context_speed(cm_stride_mix_speed[2]);
        let first_marker = *cursor;
        expand_or_extend(literal_buffer, ret.literal_context_map.slice(), cursor);
        let second_marker = *cursor;
        expand_or_extend(literal_buffer, ret.predmode_speed_and_distance_context_map.slice(), cursor);

        return Ok(Some(Command::PredictionMode(PredictionModeContextMap::<brotli::SliceOffset>{
            literal_context_map:brotli::SliceOffset(first_marker, ret.literal_context_map.len() as u32),
            predmode_speed_and_distance_context_map:brotli::SliceOffset(second_marker,
                                                                        ret.predmode_speed_and_distance_context_map.len() as u32),
        })));
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
        if *cursor + expected_len as usize > literal_buffer.len() {
            literal_buffer.resize(*cursor + expected_len as usize, 0);
        }
        if strict_ring_buffer {
            if *cursor < distance as usize {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          "Copy distance before input start"));
            }
            for (i, j) in (*cursor..(*cursor+expected_len as usize)).zip((*cursor - distance as usize)..(*cursor - distance as usize + expected_len as usize)) {
                let tmp = literal_buffer[j];
                literal_buffer[i] = tmp;
            }
            *cursor += expected_len as usize;
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
                let dict_cmd = DictCommand{
                    word_size:word_len,
                    word_id:word_index,
                    empty:0,
                    final_size:expected_len,
                    transform:transform
                };
                if strict_ring_buffer{
                    let copy_len = u32::from(dict_cmd.word_size);
                    let word_len_category_index = kBrotliDictionaryOffsetsByLength[copy_len as usize] as u32;
                    let word_index = (dict_cmd.word_id * copy_len) + word_len_category_index;
                    let dict = &kBrotliDictionary;
                    let word = &dict[(word_index as usize)..(word_index as usize + copy_len as usize)];
                    let mut transformed_word = [0u8;kBrotliMaxDictionaryWordLength as usize + 13];
                    let final_len = TransformDictionaryWord(&mut transformed_word[..],
                                                            &word[..],
                                                            copy_len as i32,
                                                            i32::from(dict_cmd.transform));
                    expand_or_extend(literal_buffer, &transformed_word[..final_len as usize], cursor);
                }
                return Ok(Some(Command::Dict(dict_cmd)));
            }
        }
    } else if cmd == "insert" || cmd == "rndins" {
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

        let data = try!(util::literal_slice_to_vec(&s.as_bytes()[command_vec[0].len() + command_vec[1].len() + 2..]));
        let probs = if command_vec.len() > 3 && command_vec[2].len() != 0 && command_vec[2].bytes().next().unwrap() != b'\"' {
            let prob = try!(util::hex_slice_to_vec(command_vec[3].as_bytes()));
            assert!(prob.len() == expected_len * 8);
            prob
        } else {
            Vec::<u8>::new()
        };
        let marker0 = *cursor;
        expand_or_extend(literal_buffer, &data[..], cursor);
        let marker1 = *cursor;
        expand_or_extend(literal_buffer, &probs[..], cursor);
        literal_buffer.extend(&probs[..]);
        if data.len() != expected_len {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      String::from("Length does not match ") + s))
        }
        match deserialize_external_probabilities(&probs) {
            Ok(external_probs) => {
                return Ok(Some(Command::Literal(LiteralCommand{
                    data:brotli::SliceOffset(marker0, data.len() as u32),
                    high_entropy:cmd == "rndins",
                    prob:deserialize_ref_probabilities(brotli::SliceOffset(marker1, external_probs.len() as u32)).unwrap(),
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
                     Writer:std::io::Write,
                     ISl:SliceWrapper<u8>+Default>(state: &mut RState,
                                             cmd_buffer:&[Command<ISl>],
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
            DivansOutputResult::Success => {
                assert_eq!(i_processed_index, cmd_buffer.len());
                break;
            },
            DivansOutputResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                if let Err(x) = w.write_all(output_scratch.split_at(o_processed_index).0) {
                    return Err(x);
                }
                ret += o_processed_index;
                o_processed_index = 0;
            }
//            DivansResult::NeedsMoreInput => {
//                assert_eq!(i_processed_index, cmd_buffer.len());
//                break;
//            }
            DivansOutputResult::Failure(m) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                               DivansErrMsg(m)));
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
    let mut obuffer = vec![0u8; 65_536];
    let mut literal_buffer = Vec::<u8>::new();

    let mut tbuffer = [Command::<brotli::SliceOffset>::nop(); CMD_BUFFER_SIZE];

    let mut i_read_index = 0usize;
    let mut state = divans::DivansRecodeState::<RingBuffer>::default();
    let mut cursor = 0usize;
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
                if i_read_index == tbuffer.len() || count == 0 {
                    let mut ibuffer:[Command<brotli::InputReference>;CMD_BUFFER_SIZE] = [Command::BlockSwitchCommand(BlockSwitch::new(0));CMD_BUFFER_SIZE];
                    for (icommand, frozen) in ibuffer[..i_read_index].iter_mut().zip(tbuffer[..i_read_index].iter()) {
                        *icommand = brotli::interface::thaw(frozen, &literal_buffer[..]);
                    }
                    recode_cmd_buffer(&mut state, ibuffer.split_at(i_read_index).0, w,
                                      &mut obuffer[..]).unwrap();
                    i_read_index = 0
                }
                if count == 0 {
                    break;
                }
                let line = buffer.trim().to_string();
                match command_parse(&line, &mut literal_buffer, &mut cursor, false).unwrap() {
                    None => {},
                    Some(c) => {
                        tbuffer[i_read_index] = c;
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
            DivansOutputResult::Success => {
                if o_processed_index != 0 {
                    if let Err(x) = w.write_all(obuffer.split_at(o_processed_index).0) {
                        return Err(x);
                    }
                }
                break;
            },
            DivansOutputResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                if let Err(x) = w.write_all(obuffer.split_at(o_processed_index).0) {
                    return Err(x);
                }
            }
            DivansOutputResult::Failure(m) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                               DivansErrMsg(m)));
            }
        }
    }

    Ok(())
}




fn compress_inner<Reader:std::io::BufRead,
                  Writer:std::io::Write,
                  Encoder:ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                  AllocU8:alloc::Allocator<u8>,
                  AllocU32:alloc::Allocator<u32>,
                  AllocCDF16:alloc::Allocator<divans::DefaultCDF16>>(
    mut state: DivansCompressor<Encoder,
                                AllocU8,
                                AllocU32,
                                AllocCDF16>,
    r:&mut Reader,
    w:&mut Writer,
    opts: &divans::DivansCompressorOptions,) -> io::Result<()> {
    let mut expanded_buffer  = ItemVec::<Command<brotli::SliceOffset>>::default();
    let mut mc = ItemVecAllocator::<Command<brotli::SliceOffset>>::default();
    let mut buffer = String::new();
    let mut obuffer = vec![0u8; 65_536];
    let backref_len = (1u64 << opts.window_size.unwrap() as u32) as usize;
    let mut predmode_buffer = Vec::<u8>::new();
    let mut predmode_cursor = 0usize;
    let mut literal_buffer = vec![0u8;backref_len + backref_len];
    let mut ibuffer = Vec::<Command<brotli::SliceOffset>>::new();
    let mut literal_cursor = backref_len;
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
                let line = buffer.trim().to_string();
                if (count == 0 || is_pred_mode(&line) || literal_cursor >= literal_buffer.len()) && ibuffer.len() != 0 {
                    {
                    let mut rest;
                    let to_write;
                    if let Command::PredictionMode(x) = ibuffer[0] {
                        let pred_mode = x;
                        rest = &mut ibuffer[1..];
                        let mut thawed_pm = match brotli::interface::thaw(&Command::PredictionMode(pred_mode), &predmode_buffer[..]) {
                            Command::PredictionMode(x) => x,
                            _ => panic!("Thawed predctionmode is not a predictionmode"),
                        };
                        let mut literal_context_map_mut = thawed_pm.literal_context_map.slice().to_vec();
                        let mut distance_context_map_mut = thawed_pm.predmode_speed_and_distance_context_map.slice().to_vec();
                        let mut pred_mode_mut = PredictionModeContextMap::<brotli::InputReferenceMut>{
                            literal_context_map:brotli::InputReferenceMut{
                                data:&mut literal_context_map_mut[..],
                                orig_offset:0,
                            },
                            predmode_speed_and_distance_context_map:brotli::InputReferenceMut{
                                data:&mut distance_context_map_mut[..],
                                orig_offset:0,
                            }
                        };
                        let final_cmd = if opts.divans_ir_optimizer != 0 {
                            match ir_optimize::ir_optimize(&mut pred_mode_mut,
                                                           &mut rest,
                                                           brotli::InputPair(brotli::InputReference{
                                                               data:&literal_buffer[..],
                                                               orig_offset:0,
                                                           },brotli::InputReference{
                                                               data:&[],
                                                               orig_offset:literal_buffer.len(),
                                                           }),
                                                           state.get_codec_mut(),
                                                           opts.window_size.unwrap() as u8,
                                                           *opts,
                                                           &mut mc, &mut expanded_buffer) {
                                Ok(buf) => buf,
                                Err(e) => {return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                                                     e));},
                            }
                        } else {
                            rest
                        };
                        try!(recode_cmd_buffer(&mut state, &[Command::PredictionMode(pred_mode_mut)], w,
                                               &mut obuffer[..]));
                        to_write = final_cmd;
                    } else {
                        to_write = &ibuffer[..]
                    }
                    let mut i_read_index = 0usize;
                    let mut thawed:[Command<brotli::InputReference>;CMD_BUFFER_SIZE] = [Command::BlockSwitchCommand(BlockSwitch::new(0));CMD_BUFFER_SIZE];
                    while i_read_index < to_write.len() {
                        let to_copy = core::cmp::min(thawed.len(), to_write.len() - i_read_index);
                        for (icommand, frozen) in thawed[..to_copy].iter_mut().zip(to_write[..].split_at(i_read_index).1.split_at(to_copy).0.iter()) {
                            *icommand = brotli::interface::thaw(frozen, &literal_buffer[..]);
                        }
                        try!(recode_cmd_buffer(&mut state, thawed.split_at(to_copy).0, w,
                                               &mut obuffer[..]));
                        i_read_index += to_copy;
                    }
                    }
                    ibuffer.clear();
                    assert!(literal_cursor >= backref_len);
                    assert!(literal_cursor <= literal_buffer.len());
                    assert!(backref_len <= literal_buffer.len());
                    // repopulate ring buffer to end at middle
                    for (i, j) in (0..backref_len).zip((literal_cursor - backref_len)..literal_cursor) {
                        let tmp = literal_buffer[j];
                        literal_buffer[i] = tmp;
                    }
                    literal_cursor = backref_len;
                }
                if count == 0 {
                    break;
                }
                if is_pred_mode(&line) {
                    if let Some(c) = try!(command_parse(&line, &mut predmode_buffer, &mut predmode_cursor, true)) {
                        ibuffer.push(c);
                    }
                } else if let Some(c) = try!(command_parse(&line, &mut literal_buffer, &mut literal_cursor, true)) {
                    ibuffer.push(c);
                }
            }
        }
    }
    loop {
        let mut o_processed_index = 0;
        match state.flush(&mut obuffer[..],
                          &mut o_processed_index) {
            DivansOutputResult::Success => {
                if o_processed_index != 0 {
                    if let Err(x) = w.write_all(obuffer.split_at(o_processed_index).0) {
                        return Err(x);
                    }
                }
                break;
            },
            DivansOutputResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                if let Err(x) = w.write_all(obuffer.split_at(o_processed_index).0) {
                    return Err(x);
                }
            }
            DivansOutputResult::Failure(m) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                               DivansErrMsg(m)));
            }
        }
    }
    let _m8 = state.free().0;
    Ok(())
}
fn compress_raw_inner<Compressor: divans::interface::Compressor,
                      Reader:std::io::Read,
                      Writer:std::io::Write>(r:&mut Reader,
                                             w:&mut Writer,
                                             mut ibuffer: <ItemVecAllocator<u8> as Allocator<u8>>::AllocatedMemory,
                                             mut obuffer: <ItemVecAllocator<u8> as Allocator<u8>>::AllocatedMemory,
                                             mut compress_state: Compressor,
                                             mut additional_input: &mut [u8],
                                             free_state: &mut Fn(Compressor)->ItemVecAllocator<u8>) -> io::Result<()> {
    let mut ilim = additional_input.len();
    let mut idec_index = 0usize;
    let mut olim = 0usize;
    let mut oenc_index = 0usize;
    let mut err: io::Result<()> = Ok(());
    // read in the header and check for divans

    while let Ok(_) = err {
        let mut borrowed_ibuffer = ibuffer.slice_mut();
        if additional_input.len() != 0 {
            if idec_index == additional_input.len() {
                additional_input = &mut []; // clear out the header
                ilim = 0;
                idec_index = 0;
            } else {
                borrowed_ibuffer = &mut additional_input[..];
                ilim = borrowed_ibuffer.len();
            }
        }
        if idec_index == ilim {
            idec_index = 0;
            match r.read(borrowed_ibuffer) {
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
                    err = Err(e);
                    break;
                }
            }
        }
        if idec_index != ilim {
            match compress_state.encode(borrowed_ibuffer.split_at(ilim).0,
                               &mut idec_index,
                               obuffer.slice_mut().split_at_mut(oenc_index).1,
                               &mut olim) {
                DivansResult::Success => continue,
                DivansResult::Failure(m) => {
                    err = Err(io::Error::new(io::ErrorKind::Other,
                               DivansErrMsg(m)));
                    break;
                },
                DivansResult::NeedsMoreInput | DivansResult::NeedsMoreOutput => {},
            }
        }
        while oenc_index != olim {
            match w.write(&obuffer.slice()[oenc_index..olim]) {
                Ok(count) => oenc_index += count,
                Err(e) => {
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    err = Err(e);
                    break;
                }
            }
        }
        olim = 0;
        oenc_index = 0;
    }
    if let Err(e) = err {
        let mut m8 = free_state(compress_state);
        m8.free_cell(ibuffer);
        m8.free_cell(obuffer);
        return Err(e);
    }
    let mut done = false;
    while !done {
        match compress_state.flush(obuffer.slice_mut().split_at_mut(oenc_index).1,
                          &mut olim) {
            DivansOutputResult::Success => done = true,
            DivansOutputResult::Failure(m) => {
                let mut m8 = free_state(compress_state);
                m8.free_cell(ibuffer);
                m8.free_cell(obuffer);
                return Err(io::Error::new(io::ErrorKind::Other,
                                          DivansErrMsg(m)));
            },
            DivansOutputResult::NeedsMoreOutput => {
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
                                                         ItemVecAllocator<u64>,
                                                         ItemVecAllocator<brotli::enc::command::Command>,
                                                         ItemVecAllocator<divans::DefaultCDF16>,
                                                         ItemVecAllocator<brotli::enc::util::floatX>,
                                                         ItemVecAllocator<brotli::enc::vectorization::Mem256f>,
                                                         ItemVecAllocator<brotli::enc::PDF>,
                                                         ItemVecAllocator<brotli::enc::StaticCommand>,
                                                         ItemVecAllocator<brotli::enc::histogram::HistogramLiteral>,
                                                         ItemVecAllocator<brotli::enc::histogram::HistogramCommand>,
                                                         ItemVecAllocator<brotli::enc::histogram::HistogramDistance>,
                                                         ItemVecAllocator<brotli::enc::cluster::HistogramPair>,
                                                         ItemVecAllocator<brotli::enc::histogram::ContextType>,
                                                         ItemVecAllocator<brotli::enc::entropy_encode::HuffmanTree>,
                                                         ItemVecAllocator<brotli::enc::ZopfliNode>>;

fn compress_raw<Reader:std::io::Read,
                Writer:std::io::Write>(r:&mut Reader,
                                       w:&mut Writer,
                                       opts: divans::DivansCompressorOptions,
                                       mut buffer_size: usize,
                                       use_brotli: bool,
                                       force_compress: bool,
                                       multithread: bool) -> io::Result<()> {
    let mut basic_buffer_backing = [0u8; 16];
    let basic_buffer: &mut[u8];
    if force_compress {
        basic_buffer = &mut[];
    } else {
        let mut ilim = 0usize;
        loop {
            match r.read(&mut basic_buffer_backing[ilim..]) {
                Ok(count) => {
                    ilim += count;
                    if count == 0 || ilim == basic_buffer_backing.len() {
                        basic_buffer = &mut basic_buffer_backing[..ilim];
                        break; // we're done reading the input
                    }
                },
                Err(e) => {
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(e);
                }
            }
        }
    }
    if force_compress == false && is_divans(basic_buffer) {
        return decompress(r, w, buffer_size, basic_buffer, false, multithread);
    }
    let mut m8 = ItemVecAllocator::<u8>::default();
    if buffer_size == 0 {
        buffer_size = 4096;
    }
    let ibuffer = m8.alloc_cell(buffer_size);
    let obuffer = m8.alloc_cell(buffer_size);
    if use_brotli {
        let state =BrotliFactory::new(
            m8,
            ItemVecAllocator::<u32>::default(),
            ItemVecAllocator::<divans::DefaultCDF16>::default(),
            opts,
            (ItemVecAllocator::<u8>::default(),
             ItemVecAllocator::<u16>::default(),
             ItemVecAllocator::<i32>::default(),
             ItemVecAllocator::<brotli::enc::command::Command>::default(),
             ItemVecAllocator::<u64>::default(),
             ItemVecAllocator::<brotli::enc::util::floatX>::default(),
             ItemVecAllocator::<brotli::enc::vectorization::Mem256f>::default(),
             ItemVecAllocator::<brotli::enc::histogram::HistogramLiteral>::default(),
             ItemVecAllocator::<brotli::enc::histogram::HistogramCommand>::default(),
             ItemVecAllocator::<brotli::enc::histogram::HistogramDistance>::default(),
             ItemVecAllocator::<brotli::enc::cluster::HistogramPair>::default(),
             ItemVecAllocator::<brotli::enc::histogram::ContextType>::default(),
             ItemVecAllocator::<brotli::enc::entropy_encode::HuffmanTree>::default(),
             ItemVecAllocator::<brotli::enc::ZopfliNode>::default(),
             ItemVecAllocator::<brotli::enc::PDF>::default(),
             ItemVecAllocator::<brotli::enc::StaticCommand>::default(),
            ), 
        );
        let mut free_closure = |state_to_free:<BrotliFactory as DivansCompressorFactory<ItemVecAllocator<u8>, ItemVecAllocator<u32>, ItemVecAllocator<divans::DefaultCDF16>>>::ConstructedCompressor| ->ItemVecAllocator<u8> {state_to_free.free().0};
        compress_raw_inner(r, w,
                           ibuffer, obuffer,
                           state,
                           basic_buffer,
                           &mut free_closure)
    } else {
        type Factory = DivansCompressorFactoryStruct<
                ItemVecAllocator<u8>,
                ItemVecAllocator<divans::DefaultCDF16>>;
        let state =Factory::new(
            m8,
            ItemVecAllocator::<u32>::default(),
            ItemVecAllocator::<divans::DefaultCDF16>::default(),
            opts, (),
        );
        let mut free_closure = |state_to_free:<Factory as DivansCompressorFactory<ItemVecAllocator<u8>, ItemVecAllocator<u32>, ItemVecAllocator<divans::DefaultCDF16>>>::ConstructedCompressor| ->ItemVecAllocator<u8> {state_to_free.free().0};
        compress_raw_inner(r, w,
                           ibuffer, obuffer,
                           state,
                           basic_buffer,
                           &mut free_closure)
    }
}
fn compress_ir<Reader:std::io::BufRead,
            Writer:std::io::Write>(
    r:&mut Reader,
    w:&mut Writer,
    mut opts: divans::DivansCompressorOptions,
) -> io::Result<()> {
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
    opts.window_size = Some(window_size);
    let state =DivansCompressorFactoryStruct::<ItemVecAllocator<u8>,
                                  ItemVecAllocator<divans::DefaultCDF16>>::new(
        ItemVecAllocator::<u8>::default(),
        ItemVecAllocator::<u32>::default(),
        ItemVecAllocator::<divans::DefaultCDF16>::default(),
        opts,
        (),
    );
    compress_inner(state, r, w, &opts)
}

fn decompress<Reader:std::io::Read, Writer:std::io::Write>(r:&mut Reader,
                                                           w:&mut Writer,
                                                           buffer_size: usize,
                                                           additional_input: &mut[u8],
                                                           skip_crc: bool,
                                                           multithread:bool,) -> io::Result<()>
{
    let ret;
    let mut state = DivansDecompressorFactoryStruct::<ItemVecAllocator<u8>, ItemVecAllocator<divans::DefaultCDF16>, ItemVecAllocator<StaticCommand>>::new(
        ItemVecAllocator::<u8>::default(),
        ItemVecAllocator::<divans::DefaultCDF16>::default(),
        ItemVecAllocator::<StaticCommand>::default(),
        skip_crc,
        multithread,
    );
    
    ret = decompress_generic(
        r,
        w,
        &mut state,
        additional_input,
        buffer_size);
    state.free();
    ret
}


#[allow(unused_assignments)]
fn decompress_generic<Reader:std::io::Read,
                      Writer:std::io::Write,
                      D:Decompressor>(r:&mut Reader,
                                      w:&mut Writer,
                                      state:&mut D,
                                      additional_input:&mut[u8],
                                      mut buffer_size: usize) -> io::Result<()> {
    if buffer_size == 0 {
        buffer_size = 4096;
    }
    let mut ibuffer = vec![0u8; core::cmp::max(buffer_size, additional_input.len())];
    ibuffer[..additional_input.len()].clone_from_slice(additional_input);
    let mut obuffer = vec![0u8; buffer_size];;
    let mut input_offset = 0usize;
    let mut input_end = additional_input.len();
    let mut output_offset = 0usize;

    loop {
        match state.decode(ibuffer[..].split_at(input_end).0,
                           &mut input_offset,
                           &mut obuffer[..],
                           &mut output_offset) {
            DivansResult::Success => {
                break
            },
            DivansResult::Failure(m) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          DivansErrMsg(m)));
            },
            DivansResult::NeedsMoreOutput => {
                let mut output_written = 0;
                while output_written != output_offset {
                    // flush buffer, if any
                    match w.write(obuffer[..].split_at(output_written).1.split_at(output_offset - output_written).0) {
                        Ok(count) => output_written += count,
                        Err(e) => {
                            if e.kind() == io::ErrorKind::Interrupted {
                                continue;
                            }
                            return Err(e);
                        }
                    }
                }
                output_offset = 0; // reset buffer
            },
            DivansResult::NeedsMoreInput => {
                if input_offset == input_end {
                    // We have exhausted all the available input, so we can reset the cursors.
                    input_offset = 0;
                    input_end = 0;
                }
                let mut any_read = false;
                loop {
                    if ibuffer[..].split_at_mut(input_end).1.len() == 0 {
                        break;
                    }
                    match r.read(ibuffer[..].split_at_mut(input_end).1) {
                        Ok(size) => {
                            if size == 0 && ! any_read {
                                //println_stderr!("End of file.  Feeding zero's.\n");
                                return Err(io::Error::new(
                                    io::ErrorKind::UnexpectedEof,
                                    "Divans file invalid: didn't have a terminator marker"));
                            } else {
                                input_end += size;
                            }
                            if size == 0 {
                                break;
                            } else {
                                any_read = true;
                            }
                            break;
                        },
                        Err(e) => {
                            if e.kind() == io::ErrorKind::Interrupted {
                                continue;
                            }
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
        match w.write(obuffer[..].split_at(output_written).1.split_at(output_offset - output_written).0) {
            Ok(count) => output_written += count,
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
        }
    }
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
#[cfg(not(feature="no-stdlib"))]
const PARALLEL_AVAILABLE: bool = true;
#[cfg(feature="no-stdlib")]
const PARALLEL_AVAILABLE: bool = false;

fn main() {
    let mut force_compress = false;
    let mut do_compress = true;
    let mut raw_compress = true;
    let mut q9_5 = false;
    let mut divans_ir_optimizer = false;
    let mut do_recode = false;
    let mut filenames = [std::string::String::new(), std::string::String::new()];
    let mut num_benchmarks = 1;
    let mut use_context_map = true;
    let mut use_brotli = true;
    let mut force_stride_value = StrideSelection::UseBrotliRec;
    let mut literal_adaptation: Option<[Speed;4]> = None;
    let mut window_size: Option<i32> = Some(22);
    let mut lgwin: Option<u32> = Some(22);
    let mut quality: Option<u16> = Some(11);
    let mut stride_detection_quality: Option<u8> = None;
    let mut speed_detection_quality: Option<u8> = None;
    let mut dynamic_context_mixing: Option<u8> = Some(1);
    let mut buffer_size:usize = 65_536;
    let mut force_prior_algorithm: Option<u16> = None;
    let mut set_low = false;
    let mut brotli_literal_byte_score: Option<u32> = None;
    let mut doubledash = false;
    let mut prior_bitmask_detection = true;
    let mut force_literal_context_mode:Option<LiteralPredictionModeNibble> = None;
    let mut skip_crc = false;
    let mut parallel = PARALLEL_AVAILABLE;
    {
        for argument in env::args().skip(1) {
            if !doubledash {
                if argument == "-d" {
                    do_compress = false;
                    continue;
                }
                if argument == "-serial" {
                    parallel = false;
                    continue;
                }
                if argument == "-skipcrc" {
                    skip_crc = true;
                    continue;
                }
                if argument == "-nocrc" {
                    skip_crc = true;
                    continue;
                }
                if argument == "--" {
                    doubledash = true;
                    continue;
                }
                if argument.starts_with("-bytescore") {
                    brotli_literal_byte_score = Some(argument.trim_matches(
                        '-').trim_matches(
                        'b').trim_matches(
                        'y').trim_matches(
                        't').trim_matches(
                        'e').trim_matches(
                        's').trim_matches(
                        'c').trim_matches(
                        'o').trim_matches(
                        'r').trim_matches(
                        'e').trim_matches(
                        '=').parse::<u32>().unwrap());
                    continue;
                }
                if argument == "-utf8" {
                    force_literal_context_mode = Some(LiteralPredictionModeNibble(brotli::enc::interface::LITERAL_PREDICTION_MODE_UTF8));
                    continue;
                }
                if argument == "-msb" {
                    force_literal_context_mode = Some(LiteralPredictionModeNibble(brotli::enc::interface::LITERAL_PREDICTION_MODE_MSB6));
                    continue;
                }
                if argument == "-lsb" {
                    force_literal_context_mode = Some(LiteralPredictionModeNibble(brotli::enc::interface::LITERAL_PREDICTION_MODE_LSB6));
                    continue;
                }
                if argument.starts_with("-sign") {
                    force_literal_context_mode = Some(LiteralPredictionModeNibble(brotli::enc::interface::LITERAL_PREDICTION_MODE_SIGN));
                    continue;
                }

                if argument.starts_with("-bs") {
                    buffer_size = argument.trim_matches(
                        '-').trim_matches(
                        'b').trim_matches('s').parse::<usize>().unwrap();
                    continue;
                }
                if argument.starts_with("-benchmark") {
                    num_benchmarks = argument.trim_matches(
                        '-').trim_matches(
                        'b').trim_matches(
                        'e').trim_matches(
                        'n').trim_matches(
                        'c').trim_matches(
                        'h').trim_matches(
                        'm').trim_matches(
                        'a').trim_matches(
                        'r').trim_matches(
                        'k').trim_matches(
                        '=').parse::<usize>().unwrap();
                    continue;
                }
                if argument == "--recode" {
                    do_recode = true;
                    continue;
                }
                if argument.starts_with("-lgwin") {
                    let fs = argument.trim_matches(
                        '-').trim_matches(
                        'l').trim_matches(
                        'g').trim_matches(
                        'w').trim_matches(
                        'i').trim_matches(
                        'n').trim_matches(
                        '=').parse::<u32>().unwrap();
                    lgwin=Some(fs);
                    continue;
                }
                if argument.starts_with("-q9.5") {
                    if argument == "-q9.5x" {
                        q9_5 = true;
                        quality = Some(11);
                    } else {
                        q9_5 = true;
                        quality = Some(10);
                    }
                    continue;
                } else if argument.starts_with("-q") {
                    let fs = argument.trim_matches(
                        '-').trim_matches(
                        'q').trim_matches(
                        'u').trim_matches(
                        'a').trim_matches(
                        'l').trim_matches(
                        'i').trim_matches(
                        't').trim_matches(
                        'y').trim_matches(
                        '=').parse::<u16>().unwrap();
                    quality=Some(fs);
                    continue;
                }
                if argument.starts_with("-p") {
                    let fs = argument.trim_matches(
                        '-').trim_matches(
                        'p').trim_matches(
                        'r').trim_matches(
                        'i').trim_matches(
                        'o').trim_matches(
                        'r').trim_matches(
                        'd').trim_matches(
                        'e').trim_matches(
                        'p').trim_matches(
                        't').trim_matches(
                        'h').trim_matches(
                    '=').parse::<i32>().unwrap();
                    force_prior_algorithm=Some(fs as u16);
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
                if argument == "-brotlistride" {
                    force_stride_value = StrideSelection::UseBrotliRec;
                    stride_detection_quality = Some(1);
                    continue;
                }
                if argument == "-advbrotlistride" {
                    force_stride_value = StrideSelection::UseBrotliRec;
                    stride_detection_quality = Some(2);
                    continue;
                }
                if argument == "-expbrotlistride" {
                    force_stride_value = StrideSelection::UseBrotliRec;
                    stride_detection_quality = Some(3);
                    continue;
                }
                if argument.starts_with("-nostride") {
                    force_stride_value = StrideSelection::PriorDisabled;
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
                if argument == "-nocm" || argument == "-nocontextmap" {
                    use_context_map = false;
                    continue;
                }
                if argument == "-i" {
                    do_compress = true;
                    raw_compress = false;
                    continue;
                }
                if argument.starts_with("-O") {
                    if argument != "-O0" {
                        divans_ir_optimizer = true
                    }
                    continue;
                }
                if argument == "-c" {
                    do_compress = true;
                    raw_compress = true;
                    force_compress = true;
                    continue;
                }
                if argument == "-nobrotli" {
                    do_compress = true;
                    raw_compress = true;
                    use_brotli = false;
                    continue;
                }
                if argument == "-findprior" {
                    prior_bitmask_detection = true;
                    continue;
                }
                if argument == "-defaultprior" {
                    prior_bitmask_detection = false;
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
                if argument == "-findspeed" {
                    speed_detection_quality = Some(1);
                    continue;
                }
                if argument.starts_with("-speed=") {
                    let spd = argument.trim_matches(
                        '-').trim_matches(
                        's').trim_matches(
                        'p').trim_matches(
                        'e').trim_matches(
                        'e').trim_matches(
                        'd').trim_matches(
                        '=').parse::<Speed>().unwrap();
                    match literal_adaptation {
                        None => literal_adaptation = Some([spd, spd, spd, spd]),
                        Some(ref mut adapt) => {
                            (*adapt)[1] = spd;
                            if !set_low {
                                (*adapt)[0] = spd;
                            }
                        },
                    }
                    continue
                }
                if argument.starts_with("-speedlow=") {
                    let spd = argument.trim_matches(
                        '-').trim_matches(
                        's').trim_matches(
                        'p').trim_matches(
                        'e').trim_matches(
                        'e').trim_matches(
                        'd').trim_matches(
                        'l').trim_matches(
                        'o').trim_matches(
                        'w').trim_matches(
                        '=').parse::<Speed>().unwrap();
                    match literal_adaptation {
                        None => literal_adaptation = Some([spd, spd, spd, spd]),
                        Some(ref mut adapt) => {
                           (*adapt)[0] = spd;
                            if !set_low {
                                (*adapt)[2] = spd;
                            }
                        },
                    }
                    set_low = true;
                    continue
                }
                if argument.starts_with("-cmspeed=") {
                    let spd = argument.trim_matches(
                        '-').trim_matches(
                        'c').trim_matches(
                        'm').trim_matches(
                        's').trim_matches(
                        'p').trim_matches(
                        'e').trim_matches(
                        'e').trim_matches(
                        'd').trim_matches(
                        '=').parse::<Speed>().unwrap();
                    match literal_adaptation {
                        None => literal_adaptation = Some([spd, spd, spd, spd]),
                        Some(ref mut adapt) => {
                          (*adapt)[3] = spd;
                           if !set_low {
                               (*adapt)[2] = spd;                                
                           }
                        },
                    }
                    continue
                }
                if argument.starts_with("-cmspeedlow=") {
                    let spd = argument.trim_matches(
                        '-').trim_matches(
                        'c').trim_matches(
                        'm').trim_matches(
                        's').trim_matches(
                        'p').trim_matches(
                        'e').trim_matches(
                        'e').trim_matches(
                        'd').trim_matches(
                        'l').trim_matches(
                        'o').trim_matches(
                        'w').trim_matches(
                        '=').parse::<Speed>().unwrap();
                    match literal_adaptation {
                        None => literal_adaptation = Some([spd, spd, spd, spd]),
                        Some(ref mut adapt) => {
                            (*adapt)[2] = spd;
                            if !set_low {
                                (*adapt)[0] = spd;                                
                            }
                        },
                    }
                    set_low = true;
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
        let brotli_setting = if use_brotli  {
            divans::BrotliCompressionSetting::UseBrotliCommandSelection
        } else {
            divans::BrotliCompressionSetting::UseInternalCommandSelection
        };
        let opts = divans::DivansCompressorOptions{
            brotli_literal_byte_score: brotli_literal_byte_score,
            use_brotli: brotli_setting,
            dynamic_context_mixing: dynamic_context_mixing.clone(),
            literal_adaptation: literal_adaptation.clone(),
            use_context_map: use_context_map,
            prior_algorithm: force_prior_algorithm,
            force_stride_value: force_stride_value,
            quality: quality,
            q9_5: q9_5,
            window_size: window_size,
            lgblock: lgwin,
            stride_detection_quality: stride_detection_quality,
            speed_detection_quality: speed_detection_quality,
            prior_bitmask_detection: if prior_bitmask_detection {1} else {0},
            force_literal_context_mode: force_literal_context_mode,
            divans_ir_optimizer: if divans_ir_optimizer {1} else {0},
        };
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
                        match compress_ir(&mut buffered_input, &mut output, opts) {
                            Ok(_) => {}
                            Err(e) => panic!("Error {:?}", e),
                        }
                        input = buffered_input.into_inner();
                    } else if do_compress {
                        match compress_raw(&mut input,
                                           &mut output,
                                           opts,
                                           buffer_size, use_brotli, force_compress, parallel) {
                            Ok(_) => {}
                            Err(e) => panic!("Error {:?}", e),
                        }
                    } else if do_recode {
                        let mut buffered_input = BufReader::new(input);
                        recode(&mut buffered_input,
                               &mut output).unwrap();
                        input = buffered_input.into_inner();
                    } else {
                        match decompress(&mut input, &mut output, buffer_size, &mut [], skip_crc, parallel) {
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
                    match compress_ir (&mut buffered_input, &mut io::stdout(), opts) {
                        Ok(_) => {}
                        Err(e) => panic!("Error {:?}", e),
                    }
                } else if do_compress {
                    match compress_raw(&mut input,
                                       &mut io::stdout(),
                                       opts,
                                       buffer_size,
                                       use_brotli, force_compress, parallel) {
                        Ok(_) => {}
                        Err(e) => panic!("Error {:?}", e),
                    }
                } else if do_recode {
                    let mut buffered_input = BufReader::new(input);
                    recode(&mut buffered_input,
                           &mut io::stdout()).unwrap()
                } else {
                    match decompress(&mut input, &mut io::stdout(), buffer_size, &mut [], skip_crc, parallel) {
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
                match compress_ir(&mut stdin, &mut io::stdout(), opts) {
                    Ok(_) => return,
                    Err(e) => panic!("Error {:?}", e),
                }
            } else if do_compress {
                match compress_raw(&mut std::io::stdin(),
                                   &mut io::stdout(),
                                   opts,
                                   buffer_size,
                                   use_brotli, force_compress, parallel) {
                    Ok(_) => return,
                    Err(e) => panic!("Error {:?}", e),
                }
            } else if do_recode {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                recode(&mut stdin,
                       &mut io::stdout()).unwrap()
            } else {
                match decompress(&mut io::stdin(), &mut io::stdout(), buffer_size, &mut [], skip_crc, parallel) {
                    Ok(_) => return,
                    Err(e) => panic!("Error {:?}", e),
                }
            }
        }
    }
}
