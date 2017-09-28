extern crate core;
extern crate divans;
extern crate alloc_no_stdlib as alloc;

include!(concat!(env!("OUT_DIR"), "/version.rs"));

#[cfg(test)]
extern crate brotli_decompressor;

mod integration_test;
mod util;

pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
use std::env;


use core::convert::From;
use std::vec::Vec;
use divans::BlockSwitch;
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
use divans::DivansCompressor;
use divans::DivansCompressorFactoryStruct;
use divans::DivansCompressorFactory;
use divans::DivansDecompressorFactory;
use divans::DivansDecompressorFactoryStruct;
use divans::interface::{ArithmeticEncoderOrDecoder, NewWithAllocator};
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
                return Err(io::Error::new(io::ErrorKind::InvalidInput, s.clone()));
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
        return &self.0[..];
    }
}

impl<Item:Sized+Default> alloc::SliceWrapperMut<Item> for ItemVec<Item> {
    fn slice_mut(&mut self) -> &mut [Item] {
        return &mut self.0[..];
    }
}

impl<Item:Sized+Default> core::ops::Index<usize> for ItemVec<Item> {
    type Output = Item;
    fn index<'a>(&'a self, index:usize) -> &'a Item {
        return &self.0[index];
    }
}

impl<Item:Sized+Default> core::ops::IndexMut<usize> for ItemVec<Item> {

    fn index_mut(&mut self, index:usize) -> &mut Item {
        return &mut self.0[index];
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
fn window_parse(s : String) -> Result<i32, io::Error> {
    let window_vec : Vec<String> = s.split(' ').map(|s| s.to_string()).collect();
    if window_vec.len() == 0 {
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
    return Ok(expected_window_size)
}

fn command_parse(s : String, do_context_map:bool, do_stride: bool) -> Result<Option<Command<ItemVec<u8>>>, io::Error> {
    let command_vec : Vec<&str>= s.split(' ').collect();
    if command_vec.len() == 0 {
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
        let pmode = match command_vec[1].as_ref() {
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
        if !do_context_map {
            return Ok(Some(Command::PredictionMode(ret)));
        }
        match command_vec.iter().enumerate().find(|r| *r.1 == "lcontextmap") {
            Some((index, _)) => {
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
            },
            None =>{},
        }
        match command_vec.iter().enumerate().find(|r| *r.1 == "dcontextmap") {
            Some((index, _)) => {
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
            },
            None =>{},
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
            'l' => Command::BlockSwitchLiteral(LiteralBlockSwitch::new(block_type, if command_vec.len() < 2 {0} else {match command_vec[2].parse::<u32>() {
            Ok(stride) => {
                 if stride > 8 {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          "Strude must be <= 8"));

                 }
                 if do_stride{stride as u8} else {0}
            },
            Err(msg) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          msg.description()));
            }
        }})),
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
        if command_vec.len() != 3 {
            if command_vec.len() == 2 && command_vec[1] == "0" {
                return Ok(None);
            }
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          String::from("insert needs 3 arguments, not (") + &s + ")"));
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
        let data = try!(hex_string_to_vec(&command_vec[2]));
        if data.len() != expected_len {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      String::from("Length does not match ") + &s))
        }
        return Ok(Some(Command::Literal(LiteralCommand{data:ItemVec(data)})));
    }
    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                              String::from("Unknown ") + &s))
}

fn recode_cmd_buffer<Writer:std::io::Write,
          RState:Compressor>(mut state: &mut RState,
                                cmd_buffer:&[Command<ItemVec<u8>>],
                                mut w: &mut Writer,
                                mut output_scratch:&mut [u8]) -> Result<usize, io::Error> {
    let mut i_processed_index = 0usize;
    let mut o_processed_index = 0usize;
    let mut ret = 0usize;
    while i_processed_index < cmd_buffer.len() {
        match state.encode(cmd_buffer,
                           &mut i_processed_index,
                           output_scratch,
                           &mut o_processed_index) {
            BrotliResult::ResultSuccess => {
                assert_eq!(i_processed_index, cmd_buffer.len());
                break;
            },
            BrotliResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                match w.write_all(output_scratch.split_at(o_processed_index).0) {
                    Err(x) => return Err(x),
                    Ok(_) => {},
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
    match w.write_all(output_scratch.split_at(o_processed_index).0) {
        Err(x) => return Err(x),
        Ok(_) => {},
    }
    ret += o_processed_index;
    Ok(ret)
}

fn recode_inner<Reader:std::io::BufRead,
                Writer:std::io::Write,
                RingBuffer:core::default::Default+SliceWrapper<u8>+SliceWrapperMut<u8>>(
    mut r:&mut Reader,
    mut w:&mut Writer) -> io::Result<()> {
    let mut buffer = String::new();
    let mut obuffer = [0u8;65536];
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
                match command_parse(line, true, true).unwrap() {
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
                    match w.write_all(obuffer.split_at(o_processed_index).0) {
                        Err(x) => return Err(x),
                        Ok(_) => {},
                    }
                }
                break;
            },
            BrotliResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                match w.write_all(obuffer.split_at(o_processed_index).0) {
                    Err(x) => return Err(x),
                    Ok(_) => {},
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

fn allowed_command(cmd: &Command<ItemVec<u8>>, do_context_map:bool, do_stride: bool, last_literal_switch: &mut divans::LiteralBlockSwitch) -> bool {
    if do_context_map == false || do_stride == false {
        match cmd {
           &divans::Command::BlockSwitchLiteral(lbs) => {
               let retval = if do_context_map {
                   last_literal_switch.block_type() != lbs.block_type()
               } else {
                   last_literal_switch.stride() != lbs.stride()
               };
               *last_literal_switch = lbs;
              return retval;
           },
           &divans::Command::BlockSwitchDistance(_) => {
               return do_context_map;
           },
           &divans::Command::BlockSwitchCommand(_) => {
               return do_context_map;
           },
           _ => {},
        }
    }
    true
}

fn compress_inner<Reader:std::io::BufRead,
                  Writer:std::io::Write,
                  Encoder:ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                  AllocU8:alloc::Allocator<u8>,
                  AllocCDF2:alloc::Allocator<divans::CDF2>,
                  AllocCDF16:alloc::Allocator<divans::DefaultCDF16>>(
    mut state: DivansCompressor<Encoder,
                                AllocU8,
                                AllocCDF2,
                                AllocCDF16>,
    mut r:&mut Reader,
    mut w:&mut Writer,
    do_context_map: bool,
    do_stride: bool) -> io::Result<()> {
    let mut buffer = String::new();
    let mut obuffer = [0u8;65536];
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
                match command_parse(line, do_context_map, do_stride).unwrap() {
                    None => {},
                    Some(c) => {
                        if allowed_command(&c,
                                           do_context_map, do_stride,
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
                    match w.write_all(obuffer.split_at(o_processed_index).0) {
                        Err(x) => return Err(x),
                        Ok(_) => {},
                    }
                }
                break;
            },
            BrotliResult::NeedsMoreOutput => {
                assert!(o_processed_index != 0);
                match w.write_all(obuffer.split_at(o_processed_index).0) {
                    Err(x) => return Err(x),
                    Ok(_) => {},
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
fn compress<Reader:std::io::BufRead,
            Writer:std::io::Write>(
    mut r:&mut Reader,
    mut w:&mut Writer,
    literal_adaptation_speed: Option<Speed>,
    do_context_map: bool,
    do_stride: bool) -> io::Result<()> {
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
                window_size = window_parse(line).unwrap();
                break;
            }
        }
    }
    let state =DivansCompressorFactoryStruct::<ItemVecAllocator<u8>,
                                  ItemVecAllocator<divans::CDF2>,
                                  ItemVecAllocator<divans::DefaultCDF16>>::new(
        ItemVecAllocator::<u8>::default(),
        ItemVecAllocator::<divans::CDF2>::default(),
        ItemVecAllocator::<divans::DefaultCDF16>::default(),
        window_size as usize,
        literal_adaptation_speed,
   );
    compress_inner(state, r, w, do_context_map, do_stride)
}

fn zero_slice(sl: &mut [u8]) -> usize {
    for v in sl.iter_mut() {
        *v = 0u8;
    }
    return sl.len();
}

fn decompress<Reader:std::io::Read,
              Writer:std::io::Write> (mut r:&mut Reader,
                                      mut w:&mut Writer,
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
                window_size = window_parse(line).unwrap();
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
    let mut do_recode = false;
    let mut filenames = [std::string::String::new(), std::string::String::new()];
    let mut num_benchmarks = 1;
    let mut use_context_map = false;
    let mut use_stride = true;
    let mut force_stride = false;
    let mut literal_adaptation : Option<Speed> = None;
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
            if argument.starts_with("-b") {
                num_benchmarks = argument.trim_matches(
                    '-').trim_matches(
                    'b').parse::<usize>().unwrap();
                continue;
            }
            if argument == "-r" {
                do_recode = true;
                continue;
            }
            if argument == "-stride" || argument == "-s" {
                use_stride = true;
                force_stride = true;
                continue;
            }
            if argument == "-cm" || argument == "-contextmap" {
                use_context_map = true;
                if !force_stride {
                    use_stride = false;
                }
                continue;
            }
            if argument == "-c" {
                do_compress = true;
                continue;
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
                println_stderr!("Decompression:\ndivans [input_file] [output_file]\nCompression:brotli -c [input_file] [output_file]\n");
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
                    if do_compress {
                        let mut buffered_input = BufReader::new(input);
                        match compress(&mut buffered_input, &mut output, literal_adaptation.clone(), use_context_map, use_stride) {
                            Ok(_) => {}
                            Err(e) => panic!("Error {:?}", e),
                        }
                        input = buffered_input.into_inner();
                    } else if do_recode {
                        let mut buffered_input = BufReader::new(input);
                        recode(&mut buffered_input,
                               &mut output).unwrap();
                        input = buffered_input.into_inner();
                    } else {
                        match decompress(&mut input, &mut output, 65536) {
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
                if do_compress {
                    let mut buffered_input = BufReader::new(input);
                    match compress(&mut buffered_input, &mut io::stdout(), literal_adaptation, use_context_map, use_stride) {
                        Ok(_) => {}
                        Err(e) => panic!("Error {:?}", e),
                    }
                } else if do_recode {
                    let mut buffered_input = BufReader::new(input);
                    recode(&mut buffered_input,
                           &mut io::stdout()).unwrap()
                } else {
                    match decompress(&mut input, &mut io::stdout(), 65536) {
                        Ok(_) => {}
                        Err(e) => panic!("Error {:?}", e),
                    }
                }
            }
        } else {
            assert_eq!(num_benchmarks, 1);
            if do_compress {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                match compress(&mut stdin, &mut io::stdout(), literal_adaptation, use_context_map, use_stride) {
                    Ok(_) => return,
                    Err(e) => panic!("Error {:?}", e),
                }
            } else if do_recode {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                recode(&mut stdin,
                       &mut io::stdout()).unwrap()
            } else {
                match decompress(&mut io::stdin(), &mut io::stdout(), 65536) {
                    Ok(_) => return,
                    Err(e) => panic!("Error {:?}", e),
                }
            }
        }
    } else {
        assert_eq!(num_benchmarks, 1);
        match decompress(&mut io::stdin(), &mut io::stdout(), 65536) {
            Ok(_) => return,
            Err(e) => panic!("Error {:?}", e),
        }
    }
}
