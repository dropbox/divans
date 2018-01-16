extern crate divans;
use ::std::io::{ErrorKind, BufReader, Result};
use std::env;
use std::collections::HashMap;
use divans::CDF16;
use divans::BaseCDF;
fn determine_cost(cdf: &divans::DefaultCDF16,
                  nibble: u8) -> f64 {
    let pdf = cdf.pdf(nibble);
    let prob = (pdf as f64) / (cdf.max() as f64);
    return -prob.log2()
}
fn eval_stream<Reader:std::io::BufRead>(
    r :&mut Reader,
    speed: divans::Speed,
    is_hex: bool
) -> Result<f64> {
    let mut buffer = String::new();
    let mut stream_state = HashMap::<(u64, u8), divans::DefaultCDF16>::new();
    let mut cost: f64 = 0.0;
    loop {
        buffer.clear();
        match r.read_line(&mut buffer) {
            Err(e) => {
                if e.kind() == ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            },
            Ok(val) => {
                if val == 0 || val == 1{
                    return Ok(cost);
                }
                let line = buffer.trim().to_string();
                let prior_val: Vec<String> = line.split(' ').map(|s| s.to_string()).collect();
                let prior = if is_hex {
                    match u64::from_str_radix(&prior_val[0], 16) {
                        Err(_) => return Err(std::io::Error::new(ErrorKind::InvalidData,prior_val[0].clone())),
                        Ok(val) => val, 
                    }
                } else {
                    match prior_val[0].parse::<u64>() {
                        Err(_) => return Err(std::io::Error::new(ErrorKind::InvalidData,prior_val[0].clone())),
                        Ok(val) => val,
                    }
                };
                    
                let val = if is_hex {
                    match u8::from_str_radix(&prior_val[1], 16) {
                        Err(_) => return Err(std::io::Error::new(ErrorKind::InvalidData,prior_val[1].clone())),
                        Ok(val) => val,
                    }                    
                } else {
                    match prior_val[1].parse::<u8>() {
                        Err(_) => return Err(std::io::Error::new(ErrorKind::InvalidData, prior_val[1].clone())),
                        Ok(val) => val,
                    }
                };
                let val_nibbles = (val >> 4, val & 0xf);
                let prior_index_0 = (prior, 0xff);
                let prior_index_1 = (prior, val_nibbles.0);
                {
                    let mut cdf0 = &mut stream_state.entry(prior_index_0).or_insert(divans::DefaultCDF16::default());
                    cost += determine_cost(cdf0, val_nibbles.0);
                    cdf0.blend(val_nibbles.0, speed);
                }
                {
                    let mut cdf1 = &mut stream_state.entry(prior_index_1).or_insert(divans::DefaultCDF16::default());
                    cost += determine_cost(cdf1, val_nibbles.1);
                    cdf1.blend(val_nibbles.1, speed);
                }
            }
        }
    }
}


fn main() {
    let stdin = std::io::stdin();
    let stdin = stdin.lock();
    let mut buffered_in = BufReader::new(stdin);
    let mut speed = divans::Speed::MUD;
    if env::args_os().len() > 1 {
        for argument in env::args().skip(1) {
            speed  = argument.parse::<divans::Speed>().unwrap();
        }
    }
    let cost = eval_stream(&mut buffered_in, speed, true).unwrap();
    println!("{} bytes; {} bits", ((cost + 0.99) as u64) as f64 / 8.0, (cost + 0.99) as u64);
}
