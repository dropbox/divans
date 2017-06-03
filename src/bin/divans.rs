extern crate core;
extern crate divans;
extern crate alloc_no_stdlib as alloc;

pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};

use std::io;
use std::error::Error;
use core::convert::From;
use std::vec::Vec;
use divans::CopyCommand;
use divans::LiteralCommand;
use divans::Command;
use divans::DictCommand;


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
                let err = [byte;1];
                return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                          String::from("Invalid hex character ")
                                          + &String::from(String::from_utf8_lossy(&err[..]))));
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

impl alloc::SliceWrapper<u8> for ByteVec {
    fn slice(&self) -> &[u8] {
        return &self.0[..];
    }
}
fn command_parse(s : String) -> Result<Command<ByteVec>, io::Error> {
    let command_vec : Vec<String> = s.split(' ').map(|s| s.to_string()).collect();
    if command_vec.len() == 0 {
        panic!("Unexpected");
    }
    let cmd = &command_vec[0];
    if cmd == "copy" {
        if command_vec.len() != 4 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      "copy needs 4 arguments"));                
        }
        let expected_len = match command_vec[1].parse::<usize>() {
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
        let distance = match command_vec[3].parse::<usize>() {
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
                    _empty:0,
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


fn main() {
    loop {
        let mut buffer = String::new();
        match io::stdin().read_line(&mut buffer) {
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                panic!(e);
            },
            Ok(count) => {
                if count == 0 {
                    break;
                }
                let line = buffer.trim().to_string();
                let command = command_parse(line).unwrap();
                println!("COMMAND {:?}", command)
            }
        }
    }
}