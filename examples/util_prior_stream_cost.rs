extern crate divans;
use ::std::io::{ErrorKind, BufReader, Result};
use std::env;
use std::collections::HashMap;
use divans::CDF16;
use divans::BaseCDF;
use divans::Speed;
use std::vec;
fn determine_cost(cdf: &divans::DefaultCDF16,
                  nibble: u8) -> f64 {
    let pdf = cdf.pdf(nibble);
    let prob = (pdf as f64) / (cdf.max() as f64);
    return -prob.log2()
}

fn eval_stream<Reader:std::io::BufRead>(
    r :&mut Reader,
    speed: Option<divans::Speed>,
    is_hex: bool
) -> Result<f64> {
    let mut sub_streams = HashMap::<u64, vec::Vec<u8>>::new();
    let mut buffer = String::new();
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
                    break;
                }
                let line = buffer.trim().to_string();
                let mut prior_val: Vec<String> = if let Some(_) = line.find(",") {
                     line.split(',').map(|s| s.to_string()).collect()
                } else {
                     line.split(' ').map(|s| s.to_string()).collect()
                };
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
                let mut prior_stream = &mut sub_streams.entry(prior).or_insert(vec::Vec::<u8>::new());
                prior_stream.push(val);
            }
        }
    }
    let specified_speed = match speed {
        Some(s) => [s],
        None => [divans::Speed::MUD],
    };
    let trial_speeds = [
        Speed::new(0, 32),
        Speed::new(1, 32),
        Speed::new(1, 128),
        Speed::new(1, 16384),
        Speed::new(2, 1024),
        Speed::new(4, 1024),
        Speed::new(8, 8192),
        Speed::new(16, 48),
        Speed::new(32, 4096),
        Speed::new(64, 16384),
        Speed::new(128, 256),
        Speed::new(128, 16384),
        Speed::new(512, 16384),
        Speed::new(1024, 16384),
        Speed::new(1664, 16384),
    ];
    let speed_choice = match speed {
        Some(_) => &specified_speed[..],
        None => &trial_speeds[..],
    };
    for (_prior, sub_stream) in sub_streams.iter() {
        let mut best_cost_high: Option<f64> = None;
        let mut best_cost_low: Option<f64> = None;
        for cur_speed in speed_choice.iter() {
            let mut cur_cost_high: f64 = 0.0;
            let mut cdf0 = divans::DefaultCDF16::default();
            for val in sub_stream.iter() {
                let val_nibbles = (val >> 4, val & 0xf);
                {
                    cur_cost_high += determine_cost(&cdf0, val_nibbles.0);
                    cdf0.blend(val_nibbles.0, *cur_speed);
                }
            }
            best_cost_high = match best_cost_high.clone() {
                None => Some(cur_cost_high),
                Some(bc) => Some(if bc > cur_cost_high {cur_cost_high} else {bc}),
            };
        }
        for cur_speed in speed_choice.iter() {
            let mut cur_cost_low: f64 = 0.0;
            let mut cdf1a = [
                divans::DefaultCDF16::default(), divans::DefaultCDF16::default(),divans::DefaultCDF16::default(), divans::DefaultCDF16::default(),
                divans::DefaultCDF16::default(), divans::DefaultCDF16::default(),divans::DefaultCDF16::default(), divans::DefaultCDF16::default(),
                divans::DefaultCDF16::default(), divans::DefaultCDF16::default(),divans::DefaultCDF16::default(), divans::DefaultCDF16::default(),
                divans::DefaultCDF16::default(), divans::DefaultCDF16::default(),divans::DefaultCDF16::default(), divans::DefaultCDF16::default(),
                ];
            for val in sub_stream.iter() {
                let val_nibbles = (val >> 4, val & 0xf);
                {
                    let cdf1 = &mut cdf1a[val_nibbles.0 as usize];
                    cur_cost_low += determine_cost(cdf1, val_nibbles.1);
                    cdf1.blend(val_nibbles.1, *cur_speed);
                }
            }
            best_cost_low = match best_cost_low.clone() {
                None => Some(cur_cost_low),
                Some(bc) => Some(if bc > cur_cost_low {cur_cost_low} else {bc}),
            };
        }
        cost += best_cost_high.unwrap();
        cost += best_cost_low.unwrap();
    }
    Ok(cost)
}


fn main() {
    let stdin = std::io::stdin();
    let stdin = stdin.lock();
    let mut buffered_in = BufReader::new(stdin);
    let mut speed: Option<divans::Speed> = None;
    if env::args_os().len() > 1 {
        for argument in env::args().skip(1) {
            speed = Some(argument.parse::<divans::Speed>().unwrap());
        }
    }
    let cost = eval_stream(&mut buffered_in, speed, true).unwrap();
    println!("{} bytes; {} bits", ((cost + 0.99) as u64) as f64 / 8.0, (cost + 0.99) as u64);
}
