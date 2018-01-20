use ::std::io::{ErrorKind, BufReader, Result};
use std::env;
use std::collections::HashMap;
use std::collections::BTreeMap;
use std::vec;

const NUM_SPEED:usize = 512;
const MAX_MAX: i32 = 16384;
const MIN_MAX: i32 = 0x200;
#[derive(Clone, Copy, Debug)]
struct Speed {
    inc: i32,
    max: i32,
}

impl Default for Speed {
    fn default() -> Speed {
        Speed{
            inc:1,
            max:MIN_MAX,
        }
    }
}
impl Speed {
    fn inc(&mut self) {
        if self.max == MIN_MAX {
            self.max += 0x200;
        } else {
            self.max += 0x400;
        }
        if self.max > MAX_MAX {
            self.inc += 1;
            //self.inc = self.inc / 2 + (self.inc & 1);
            self.max = MIN_MAX;
        }
    }
}
#[derive(Clone,Copy)]
struct FrequentistCDF16([i32;16]);

impl Default for FrequentistCDF16 {
    fn default() -> Self {
        FrequentistCDF16(
            [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]
        )
    }
}

impl FrequentistCDF16 {
    fn max(&self) -> i32 {
        self.0[15]
    }
    fn pdf(&self, nibble: u8) -> i32 {
        if nibble == 0 {
            self.0[0]
        } else {
            self.0[nibble as usize] - self.0[nibble as usize - 1]
        }
    }
    fn assert_ok(&self, _old: FrequentistCDF16) {
        let mut last = 0i32;
        for item in self.0.iter() {
            assert!(*item != last);
            last = *item;
        }
    }
    fn blend(&mut self, nibble: u8, speed:Speed) {
        let old_self = *self;
        for i in nibble as usize..16 {
            self.0[i] += speed.inc;
        }
        if self.max() >= speed.max {
            for (index, item) in self.0.iter_mut().enumerate() {
                let cdf_bias = 1 + index as i32;
                *item = *item + cdf_bias - (*item  + cdf_bias) / 2;
            }
        }
        self.assert_ok(old_self);
    }
}

type DefaultCDF16 = FrequentistCDF16;
fn determine_cost(cdf: &DefaultCDF16,
                  nibble: u8) -> f64 {
    let pdf = cdf.pdf(nibble);
    assert!(pdf != 0);
    let prob = (pdf as f64) / (cdf.max() as f64);
    return -prob.log2()
}

fn eval_stream<Reader:std::io::BufRead>(
    r :&mut Reader,
    speed: Option<Speed>,
    use_preselected: bool, 
    is_hex: bool
) -> Result<f64> {
    let mut sub_streams = HashMap::<u64, vec::Vec<u8>>::new();
    let mut best_speed = BTreeMap::<(u64, bool), (Speed, f64)>::new();
    let mut buffer = String::new();
    let mut stream_state = HashMap::<(u64, u8), DefaultCDF16>::new();
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
        None => [Speed::default()],
    };
    let mut trial_speeds = [Speed::default(); NUM_SPEED];
    let mut cur_speed = Speed::default();
    for val in trial_speeds.iter_mut() {
        *val = cur_speed;
        cur_speed.inc();
    }
    let preselected_speeds = [
Speed {          inc: 13, max: 5120 },
Speed { inc: 1, max: 1024 },
Speed { inc: 1, max: 12288 },
Speed { inc: 1, max: 13312 },
Speed { inc: 1, max: 14336 },
Speed { inc: 1, max: 15360 },
Speed { inc: 1, max: 16384 },
Speed { inc: 1, max: 3072 },
Speed { inc: 1, max: 7168 },
Speed { inc: 1, max: 9216 },
Speed { inc: 20, max: 10240 },
Speed { inc: 28, max: 11264 },
Speed { inc: 2, max: 1024 },
Speed { inc: 2, max: 10240 },
Speed { inc: 2, max: 15360 },
Speed { inc: 2, max: 3072 },
Speed { inc: 2, max: 5120 },
Speed { inc: 2, max: 9216 },
Speed { inc: 3, max: 1024 },
Speed { inc: 3, max: 10240 },
Speed { inc: 4, max: 1024 },
Speed { inc: 4, max: 11264 },
Speed { inc: 4, max: 14336 },
Speed { inc: 5, max: 1024 },
Speed { inc: 5, max: 13312 },
Speed { inc: 5, max: 16384 },
Speed { inc: 5, max: 7168 },
Speed { inc: 6, max: 1024 },
Speed { inc: 7, max: 1024 },
Speed { inc: 7, max: 16384 },
    Speed { inc: 8, max: 1024},

    ];
    let speed_choice = match speed {
        Some(_) => &specified_speed[..],
        None => if use_preselected {
            &preselected_speeds[..]
        } else {
            &trial_speeds[..]
        },
    };
    for (&prior, sub_stream) in sub_streams.iter() {
        let mut best_cost_high: Option<f64> = None;
        let mut best_speed_high = Speed::default();
        let mut best_speed_low = Speed::default();
        let mut best_cost_low: Option<f64> = None;
        for cur_speed in speed_choice.iter() {
            let mut cur_cost_high: f64 = 0.0;
            let mut cur_cost_low: f64 = 0.0;
            for val in sub_stream.iter() {
                let val_nibbles = (val >> 4, val & 0xf);
                let prior_index_0 = (prior, 0xff);
                let prior_index_1 = (prior, val_nibbles.0);
                {
                    let mut cdf0 = &mut stream_state.entry(prior_index_0).or_insert(DefaultCDF16::default());
                    cur_cost_high += determine_cost(cdf0, val_nibbles.0);
                    cdf0.blend(val_nibbles.0, *cur_speed);
                }
                {
                    let mut cdf1 = &mut stream_state.entry(prior_index_1).or_insert(DefaultCDF16::default());
                    cur_cost_low += determine_cost(cdf1, val_nibbles.1);
                    cdf1.blend(val_nibbles.1, *cur_speed);
                }
            }
            best_cost_high = match best_cost_high.clone() {
                None => {
                    best_speed_high = *cur_speed;
                    Some(cur_cost_high)
                },
                Some(bc) => Some(if bc > cur_cost_high {
                    best_speed_high = *cur_speed;
                    cur_cost_high
                } else {bc}),
            };
            best_cost_low = match best_cost_low.clone() {
                None => {
                    best_speed_low = *cur_speed;
                    Some(cur_cost_low)
                },
                Some(bc) => Some(if bc > cur_cost_low {
                    best_speed_low = *cur_speed;
                    cur_cost_low
                } else {bc}),
            };
        }
        best_speed.insert((prior, false), (best_speed_low, best_cost_low.unwrap()));
        best_speed.insert((prior, true), (best_speed_high, best_cost_high.unwrap()));
        cost += best_cost_high.unwrap();
        cost += best_cost_low.unwrap();
    }
    for (prior, val) in best_speed.iter() {
        print!("{:?} {:?} cost: {}\n", prior, val.0, val.1);
    }
    
    Ok(cost)
}


fn main() {
    let stdin = std::io::stdin();
    let stdin = stdin.lock();
    let mut buffered_in = BufReader::new(stdin);
    let mut speed: Option<Speed> = None;
    let use_preselected = env::args_os().len() == 2;
    if use_preselected {
        print!("arg count == 1 Using preselected list\n");
    }
    if env::args_os().len() > 2 {
        let mut first:i32 = 0;
        let mut second:i32 = 0;
        for argument in env::args().skip(1) {
            first = argument.parse::<i32>().unwrap();
            break;
            //speed = Some(argument.parse::<Speed>().unwrap());
        }
        for argument in env::args().skip(2) {
            second = argument.parse::<i32>().unwrap();
            break;
            //speed = Some(argument.parse::<Speed>().unwrap());
        }
        speed = Some(Speed{inc:first, max:second});
    }
    let cost = eval_stream(&mut buffered_in, speed, use_preselected, true).unwrap();
    println!("{} bytes; {} bits", ((cost + 0.99) as u64) as f64 / 8.0, (cost + 0.99) as u64);
}
