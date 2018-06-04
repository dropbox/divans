extern crate core;
extern crate divans;
extern crate alloc_no_stdlib as alloc;


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

fn hex_string_to_vec(s: &str) -> Result<Vec<u8>, io::Error> {
    let mut output = Vec::with_capacity(s.len() >> 1);
    let mut rem = 0;
    let mut buf : u8 = 0;
    for byte in s.bytes() {
        if byte >= 'A' as u8 && byte <= 'F' as u8 {
            buf <<= 4;
            buf |= byte - 'A' as u8 + 10;
        } else if byte >= 'a' as u8 && byte <= 'f' as u8 {
            buf <<= 4;
            buf |= byte - 'a' as u8 + 10;
        } else if byte >= '0' as u8 && byte <= '9' as u8 {
            buf <<= 4;
            buf |= byte - '0' as u8;
        } else if byte == '\n' as u8|| byte == '\t' as u8|| byte == '\r' as u8 {
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
        return Ok(Some(Command::Literal(LiteralCommand{
            data:ItemVec(data),
            high_entropy:cmd == "rndins",
            prob:FeatureFlagSliceType::<ItemVec<u8>>::default(),
        })));
    }
    Err(io::Error::new(io::ErrorKind::InvalidInput,
                       String::from("Unknown ") + s))
}

fn parse_comands<Reader:std::io::BufRead>(r0:&mut Reader, r1: &mut Reader) -> Result<(), io::Error>{
    let mut buffer = String::new();
    r0.read_line(&mut buffer);
    println!("{}", buffer);
    r1.read_line(&mut buffer); // window size
    loop {
        buffer.clear();
        match r0.read_line(&mut buffer) {
            Err(e) => {
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e)
            },
            Ok(count) => {
                let line = buffer.trim().to_string();
                match command_parse(&line).unwrap() {
                    None => {},
                    Some(c) => {
                        //FIXME
                    }
                }
            }
        }
    }
}

fn main() {
}
