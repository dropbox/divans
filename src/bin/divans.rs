extern crate core;
extern crate divans;
extern crate alloc_no_stdlib as alloc;
#[cfg(test)]
extern crate brotli_decompressor;

mod integration_test;
mod util;
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
use std::env;


use core::convert::From;
use std::vec::Vec;
use divans::CopyCommand;
use divans::LiteralCommand;
use divans::Command;
use divans::DictCommand;
use divans::BrotliResult;
use divans::Compressor;
use divans::Decompressor;
use divans::CMD_BUFFER_SIZE;
use divans::DivansCompressor;
use divans::DivansDecompressor;
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
fn hex_string_to_vec(s: &String) -> Result<Vec<u8>, io::Error> {
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
pub struct ByteVec(Vec<u8>);
impl Default for ByteVec {
    fn default() -> Self {
        ByteVec(Vec::<u8>::new())
    }
}
impl alloc::SliceWrapper<u8> for ByteVec {
    fn slice(&self) -> &[u8] {
        return &self.0[..];
    }
}

impl alloc::SliceWrapperMut<u8> for ByteVec {
    fn slice_mut(&mut self) -> &mut [u8] {
        return &mut self.0[..];
    }
}

impl core::ops::Index<usize> for ByteVec {
    type Output = u8;
    fn index<'a>(&'a self, index:usize) -> &'a u8 {
        return &self.0[index];
    }
}

impl core::ops::IndexMut<usize> for ByteVec {

    fn index_mut(&mut self, index:usize) -> &mut u8 {
        return &mut self.0[index];
    }
}

struct ByteVecAllocator {
}
impl alloc::Allocator<u8> for ByteVecAllocator {
    type AllocatedMemory = ByteVec;
    fn alloc_cell(&mut self, size:usize) ->ByteVec{
        ByteVec(vec![0;size])
    }
    fn free_cell(&mut self, _bv:ByteVec) {

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
fn command_parse(s : String) -> Result<Command<ByteVec>, io::Error> {
    let command_vec : Vec<String> = s.split(' ').map(|s| s.to_string()).collect();
    if command_vec.len() == 0 {
        panic!("Unexpected");
    }
    let cmd = &command_vec[0];
    if cmd == "window" {
            // FIXME validate
            return Ok(Command::Copy(CopyCommand{distance:1,num_bytes:0}));
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
                                      s + "needs a from statement in the 2nd arg"));
        }
        let distance = match command_vec[3].parse::<u32>() {
            Ok(el) => el,
            Err(msg) => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          msg.description()));
            }
        };
        return Ok(Command::Copy(CopyCommand{distance:distance, num_bytes:expected_len}));
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
                                      s + " needs a word after the expected len"));
        }
        let word_id : Vec<String> = command_vec[3].split(',').map(|s| s.to_string()).collect();
        if word_id.len() != 2 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      s + " needs a comma separated word value"));
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
                return Ok(Command::Dict(DictCommand{
                    word_size:word_len,
                    word_id:word_index,
                    empty:0,
                    final_size:expected_len,
                    transform:transform
                }));
            }
        }
    } else if cmd == "insert"{
        if command_vec.len() != 3 {
            if command_vec.len() == 2 && command_vec[1] == "0" {
                return Ok(Command::Literal(LiteralCommand{data:ByteVec(Vec::new())}));
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
        
        let data = try!(hex_string_to_vec(&command_vec[2]));
        if data.len() != expected_len {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      String::from("Length does not match ") + &s))
        }
        return Ok(Command::Literal(LiteralCommand{data:ByteVec(data)}));
    }
    return Err(io::Error::new(io::ErrorKind::InvalidInput,
                              String::from("Unknown ") + &s))
}

fn recode_cmd_buffer<Writer:std::io::Write,
          RState:Compressor>(mut state: &mut RState,
                                cmd_buffer:&[Command<ByteVec>],
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
    assert!(o_processed_index != 0);
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
    let mut ibuffer:[Command<ByteVec>; CMD_BUFFER_SIZE] = [Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop()];
    
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
                ibuffer[i_read_index] = command_parse(line).unwrap();
                i_read_index += 1;
            }
        }
    }
    Ok(())
}
fn compress_inner<Reader:std::io::BufRead,
                  Writer:std::io::Write,
                  AllocU8:alloc::Allocator<u8> >(
    mut state: DivansCompressor<AllocU8>,
    mut r:&mut Reader,
    mut w:&mut Writer) -> io::Result<()> {
    let mut buffer = String::new();
    let mut obuffer = [0u8;65536];
    let mut ibuffer:[Command<ByteVec>; CMD_BUFFER_SIZE] = [Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop(),
                                                           Command::<ByteVec>::nop()];
    
    let mut i_read_index = 0usize;
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
                ibuffer[i_read_index] = command_parse(line).unwrap();
                i_read_index += 1;
            }
        }
    }
    Ok(())
}
fn compress<Reader:std::io::BufRead,
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
    let state =DivansCompressor::<ByteVecAllocator>::new(
        ByteVecAllocator{},
        window_size as usize);
    compress_inner(state, r, w)
}

fn decompress<Reader:std::io::Read,
              Writer:std::io::Write> (mut r:&mut Reader,
                                      mut w:&mut Writer,
                                      buffer_size: usize) -> io::Result<()> {
    let mut m8 = ByteVecAllocator{};
    let mut ibuffer = m8.alloc_cell(buffer_size);
    let mut obuffer = m8.alloc_cell(buffer_size);
    let mut state = DivansDecompressor::<ByteVecAllocator>::new(m8);
    let mut input_offset = 0usize;
    let mut input_end = 0usize;
    let mut output_offset = 0usize;
    loop {
        if output_offset < obuffer.slice().len() {
            match state.decode(ibuffer.slice().split_at(input_end).0,
                         &mut input_offset,
                         obuffer.slice_mut(),
                               &mut output_offset) {
                BrotliResult::ResultSuccess => {
                    break
                },
                BrotliResult::ResultFailure => {
                    let mut m8 = state.free();
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
                                let mut m8 = state.free();
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
                        input_offset = 0;
                    }
                    loop {
                        match r.read(ibuffer.slice_mut().split_at_mut(input_offset).1) {
                            Ok(size) => {
                                if size == 0 {
                                    return Err(io::Error::new(
                                        io::ErrorKind::UnexpectedEof,
                                        "Divans file invalid: didn't have a terminator marker"));
                                }
                                input_end = input_offset + size;
                                break
                            },
                            Err(e) => {
                                if e.kind() == io::ErrorKind::Interrupted {
                                    continue;
                                }
                                let mut m8 = state.free();
                                m8.free_cell(ibuffer);
                                m8.free_cell(obuffer);
                                return Err(e);
                            },
                        }
                    }
                },
            }
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
                let mut m8 = state.free();
                m8.free_cell(ibuffer);
                m8.free_cell(obuffer);
                return Err(e);
            }
        }
    }
    let mut m8 = state.free();
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
            if argument == "-c" {
                do_compress = true;
                continue;
            }
            if argument == "-h" || argument == "-help" || argument == "--help" {
                println_stderr!("Decompression:\ndivans [input_file] [output_file]\nCompression:brotli -c [input_file] [output_file]\n");
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
                        match compress(&mut buffered_input, &mut output) {
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
                    match compress(&mut buffered_input, &mut io::stdout()) {
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
                    panic!("Unimpl");
                }
            }
        } else {
            assert_eq!(num_benchmarks, 1);
            if do_compress {
                let stdin = std::io::stdin();
                let mut stdin = stdin.lock();
                match compress(&mut stdin, &mut io::stdout()) {
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
