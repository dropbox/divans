#![allow(unused)]

pub type Prob = i16; // can be i32

pub trait CDFUpdater {
    fn blend(&self, data:&mut [Prob;16], symbol: u8);
    fn initialization_prob(&self) -> [Prob; 16];
    fn max(&self, data:&[Prob; 16]) -> i64;
    fn log_max(&self, data:&[Prob; 16]) -> Option<i8>;
    fn valid(&self, data:&[Prob;16])->bool;
}

pub struct CDF16<T:CDFUpdater>{
    pub cdf:[Prob;16],
    updater:T,
}
impl<T:CDFUpdater+Default> Default for CDF16<T> {
    fn default() -> CDF16<T> {
        let updater = T::default();
        let starting_prob = updater.initialization_prob();
        CDF16::<T>{
            cdf:starting_prob,
            updater:updater,
        }
    }
}

const CDF_BITS : usize = 15; // 15 bits
const CDF_MAX : Prob = 32767; // last value is implicitly 32768
const CDF_LIMIT : i64 = CDF_MAX as i64 + 1;
impl<T:CDFUpdater> CDF16<T> {
    pub fn float_array(&self) -> [f32; 16]{
        let mut ret = [0.0f32; 16];
        for i in 1..16 {
            ret[i] = ((self.cdf[i].wrapping_sub(self.cdf[i-1])) as f32) / self.updater.max(&self.cdf) as f32;
        }
        ret[0] = self.cdf[0] as f32 / self.updater.max(&self.cdf) as f32;
        for i in 0..16 {
            ret[i] *= 16.0f32;
        }
        ret
    }
    pub fn valid(&self) -> bool {
        self.updater.valid(&self.cdf)
    }
    pub fn blend (&mut self, symbol: u8){
        self.updater.blend(&mut self.cdf, symbol)
    }
    pub fn max(&self) -> i64 {
        return self.updater.max(&self.cdf);
    }
    pub fn log_max(&self) -> Option<i8> {
        return self.updater.log_max(&self.cdf);
    }
}



#[allow(unused)]
macro_rules! each16{
    ($src0: expr, $func: expr) => {
    [$func($src0[0]),
     $func($src0[1]),
     $func($src0[2]),
     $func($src0[3]),
     $func($src0[4]),
     $func($src0[5]),
     $func($src0[6]),
     $func($src0[7]),
     $func($src0[8]),
     $func($src0[9]),
     $func($src0[10]),
     $func($src0[11]),
     $func($src0[12]),
     $func($src0[13]),
     $func($src0[14]),
     $func($src0[15]),
    ]
    }
}
#[allow(unused)]
macro_rules! set1 {
    ($src: expr, $val: expr) =>{
        [$val; 16]
    }
}
macro_rules! each16bin {
    ($src0 : expr, $src1 : expr, $func: expr) => {
    [$func($src0[0], $src1[0]),
           $func($src0[1], $src1[1]),
           $func($src0[2], $src1[2]),
           $func($src0[3], $src1[3]),
           $func($src0[4], $src1[4]),
           $func($src0[5], $src1[5]),
           $func($src0[6], $src1[6]),
           $func($src0[7], $src1[7]),
           $func($src0[8], $src1[8]),
           $func($src0[9], $src1[9]),
           $func($src0[10], $src1[10]),
           $func($src0[11], $src1[11]),
           $func($src0[12], $src1[12]),
           $func($src0[13], $src1[13]),
           $func($src0[14], $src1[14]),
           $func($src0[15], $src1[15])]
    }
}

struct BlendCDFUpdater {
}

impl CDFUpdater for BlendCDFUpdater {
    fn initialization_prob(&self) -> [Prob;16] {
        [
             (1 * CDF_LIMIT / 16) as Prob,
             (2 * CDF_LIMIT / 16) as Prob,
             (3 * CDF_LIMIT / 16) as Prob,
             (4 * CDF_LIMIT / 16) as Prob,
             (5 * CDF_LIMIT / 16) as Prob,
             (6 * CDF_LIMIT / 16) as Prob,
             (7 * CDF_LIMIT / 16) as Prob,
             (8 * CDF_LIMIT / 16) as Prob,
             (9 * CDF_LIMIT / 16) as Prob,
             (10 * CDF_LIMIT / 16) as Prob,
             (11 * CDF_LIMIT / 16) as Prob,
             (12 * CDF_LIMIT / 16) as Prob,
             (13 * CDF_LIMIT / 16) as Prob,
             (14 * CDF_LIMIT / 16) as Prob,
             (15 * CDF_LIMIT / 16) as Prob,
             CDF_LIMIT as Prob]
    }
    fn valid(&self, data:&[Prob; 16]) -> bool { // nonzero everywhere
        let prev = 0;
        for item in data.split_at(15).0.iter() {
            if *item <= prev || !(*item <= CDF_MAX) {
                return false;
            }
        }
        assert_eq!(data[15], CDF_LIMIT as Prob);
        return true;
    }

    fn blend(&self, data:&mut [Prob;16], symbol:u8) {
        *data = mul_blend(*data, symbol, 128, 0);
    }
    fn max(&self, data:&[Prob;16]) -> i64 {
        CDF_LIMIT
    }
    fn log_max(&self, data:&[Prob;16]) -> Option<i8> {
        Some(15)
    }
}
impl Default for BlendCDFUpdater {
    fn default() -> BlendCDFUpdater {BlendCDFUpdater{}}
}
fn srl(a:Prob) -> Prob {
    a >> 1
}


struct FrequentistCDFUpdater {}
impl Default for FrequentistCDFUpdater {
    fn default() -> FrequentistCDFUpdater {FrequentistCDFUpdater{}}
}

impl CDFUpdater for FrequentistCDFUpdater {
    fn initialization_prob(&self) -> [Prob;16] {
        [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]
    }
    fn blend(&self, data:&mut [Prob;16], symbol:u8) {
        const CDF_INDEX : [Prob;16] = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];
        const CDF_BIAS : [Prob;16] = [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16];
        let symbol16 = [symbol as Prob; 16];
        let adder_symbol = each16bin!(CDF_INDEX, symbol16, gte_bool);
        *data = each16bin!(adder_symbol, *data, add);
        const LIMIT: Prob = 32767 - 32;
        if data[data.len() - 1] >= LIMIT {
            let biased_data = each16bin!(CDF_BIAS, *data, add);
            *data = each16!(biased_data, srl);
        }
    }
    fn max(&self, data:&[Prob;16]) -> i64 {
        data[15] as i64
    }
    fn log_max(&self, data:&[Prob;16]) -> Option<i8> {
        None
    }
    fn valid(&self, data:&[Prob;16]) -> bool {
        let prev = 0;
        for item in data.split_at(15).0.iter() {
            if *item <= prev {
                return false;
            }
        }
        return true;

    }
}

#[allow(unused)]
fn gt(a:Prob, b:Prob) -> Prob {
    (-((a > b) as i64)) as Prob
}
#[allow(unused)]
fn gte(a:Prob, b:Prob) -> Prob {
    (-((a >= b) as i64)) as Prob
}
#[allow(unused)]
fn gte_bool(a:Prob, b:Prob) -> Prob {
    (a >= b) as Prob
}

fn and(a:Prob, b:Prob) -> Prob {
    a & b
}
fn add(a:Prob, b:Prob) -> Prob {
    a.wrapping_add(b)
}

const BLEND_FIXED_POINT_PRECISION : i8 = 15;

pub fn mul_blend(baseline :[Prob;16], symbol: u8, blend : i32, bias : i32) ->[Prob;16] {
    const SCALE :i32 = 1i32 << BLEND_FIXED_POINT_PRECISION;
    let to_blend = to_blend_lut(symbol);
    let mut epi32:[i32;8] = [to_blend[0] as i32,
                        to_blend[1] as i32,
                        to_blend[2] as i32,
                        to_blend[3] as i32,
                        to_blend[4] as i32,
                        to_blend[5] as i32,
                        to_blend[6] as i32,
                        to_blend[7] as i32];
    let scale_minus_blend = SCALE - blend;
    for i in 0..8 {
        epi32[i] *= blend;
        epi32[i] += baseline[i] as i32 * scale_minus_blend + bias;
        epi32[i] >>= BLEND_FIXED_POINT_PRECISION;
    }
    let mut retval : [Prob;16] =[epi32[0] as i16,
                                epi32[1] as i16,
                                epi32[2] as i16,
                                epi32[3] as i16,
                                epi32[4] as i16,
                                epi32[5] as i16,
                                epi32[6] as i16,
                                epi32[7] as i16,
                                0,0,0,0,0,0,0,0];
    let mut epi32:[i32;8] = [to_blend[8] as i32,
                             to_blend[9] as i32,
                             to_blend[10] as i32,
                             to_blend[11] as i32,
                             to_blend[12] as i32,
                             to_blend[13] as i32,
                             to_blend[14] as i32,
                             to_blend[15] as i32];
    for i in 8..16 {
        epi32[i - 8] *= blend;
        epi32[i - 8] += baseline[i] as i32 * scale_minus_blend + bias;
        retval[i] = (epi32[i - 8] >> BLEND_FIXED_POINT_PRECISION) as i16;
    }
    retval[15] = CDF_LIMIT as Prob;
    retval
}

fn to_blend(symbol: u8) -> [Prob;16] {
    let delta: Prob = CDF_MAX - 15;
    const CDF_INDEX : [Prob;16] = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];
    const BASELINE : [Prob;16] =[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16];
    let symbol16 = [symbol as i16; 16];
    let delta16 = [delta; 16];
    let mask_symbol = each16bin!(CDF_INDEX, symbol16, gte);
    let add_mask = each16bin!(delta16, mask_symbol, and);
    let to_blend = each16bin!(BASELINE, add_mask, add);
    to_blend
}

fn to_blend_lut(symbol: u8) -> [Prob;16] {
    const DEL: Prob = CDF_MAX - 15;
    static CDF_SELECTOR : [[Prob;16];16] = [
        [1+DEL,2+DEL,3+DEL,4+DEL,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2+DEL,3+DEL,4+DEL,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3+DEL,4+DEL,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4+DEL,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8,9,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8,9,10,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8,9,10,11,12+DEL,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8,9,10,11,12,13+DEL,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8,9,10,11,12,13,14+DEL,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15+DEL, CDF_LIMIT as Prob],
        [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15, CDF_LIMIT as Prob]];
    CDF_SELECTOR[symbol as usize]
}

mod test {
    #[test]
    fn test_blend_lut() {
        for i in 0..16 {
            let a = super::to_blend(i as u8);
            let b = super::to_blend_lut(i as u8);
            for j in 0..16 {
                assert_eq!(a[j], b[j]);
            }
        }
    }
    #[allow(unused)]
    const RAND_MAX : u32 = 32767;
    #[allow(unused)]
    fn simple_rand(state: &mut u64) -> u32 {
        *state = (*state).wrapping_mul(1103515245).wrapping_add(12345);
        return ((*state / 65536) as u32 % (RAND_MAX + 1)) as u32;
    }
    #[allow(unused)]
    fn test_random_helper(mut rand_table : [u32; 16],
                          num_trials: usize,
                          blend: i32,
                          bias: i32,
                          desired_outcome : [super::Prob;16]) {
        let mut sum : u32 = 0;
        for i in 0..16 {
            rand_table[i] += sum;
            sum = rand_table[i];
        }
        assert_eq!(sum, RAND_MAX + 1);
        let mut prob_state = super::CDF16::<super::BlendCDFUpdater>::default();
        // make sure we have all probability taken care of
        let mut seed = 1u64;
        for i in 0..num_trials {
            let rand_num = simple_rand(&mut seed) as u32;
            for j in 0..16{
                if rand_num < rand_table[j] {
                    // we got an j as the next symbol
                    prob_state.cdf = super::mul_blend(prob_state.cdf,
                                       j as u8,
                                       blend,
                                              bias);
                    assert!(prob_state.valid());
                    break;
                }
                assert!(j != 15); // should have broken
            }
        }
        //assert_eq!(prob_state.float_array(), [0.032;16]);
        assert_eq!(prob_state.cdf, desired_outcome);
    }
    #[allow(unused)]
    fn test_random_cdf<T:super::CDFUpdater>(mut prob_state:super::CDF16<T>,
                                                    mut rand_table : [u32; 16],
                                                    num_trials: usize,
                                                    desired_outcome : [super::Prob;16]) {
        let mut sum : u32 = 0;
        for i in 0..16 {
            rand_table[i] += sum;
            sum = rand_table[i];
        }
        assert_eq!(sum, RAND_MAX + 1);
        // make sure we have all probability taken care of
        let mut seed = 1u64;
        for i in 0..num_trials {
            let rand_num = simple_rand(&mut seed) as u32;
            for j in 0..16{
                if rand_num < rand_table[j] {
                    // we got an j as the next symbol
                    prob_state.blend(j as u8);
                    assert!(prob_state.valid());
                    break;
                }
                assert!(j != 15); // should have broken
            }
        }
        //assert_eq!(prob_state.float_array(), [0.032;16]);
        assert_eq!(prob_state.cdf, desired_outcome);
    }
    #[test]
    fn test_stationary_probability() {
        let rm = RAND_MAX as u32;
        test_random_helper([0,0,rm/16,0,
                            rm/32,rm/32,0,0,
                            rm/8,0,0,0,
                            rm/5 + 1,rm/5 + 1,rm/5 + 1,3 * rm/20 + 3],
                           1000000,
                           128,
                           0,
                           [1,2,1605,1606,2728,3967,3968,3969,7880,7881,7882,7883,14150,20830,27071,-32768]);
                            
    }
    #[test]
    fn test_stationary_probability_blend_cdf() {
        let rm = RAND_MAX as u32;
        test_random_cdf(super::CDF16::<super::BlendCDFUpdater>::default(),
                        [0,0,rm/16,0,
                         rm/32,rm/32,0,0,
                         rm/8,0,0,0,
                         rm/5 + 1,rm/5 + 1,rm/5 + 1,3 * rm/20 + 3],
                        1000000,
                        [1,2,1605,1606,2728,3967,3968,3969,7880,7881,7882,7883,14150,20830,27071,-32768]);
    }
    #[test]
    fn test_stationary_probability_frequentist_cdf() {
        let rm = RAND_MAX as u32;
        test_random_cdf(super::CDF16::<super::FrequentistCDFUpdater>::default(),
                        [0,0,rm/16,0,
                         rm/32,rm/32,0,0,
                         rm/8,0,0,0,
                         rm/5 + 1,rm/5 + 1,rm/5 + 1,3 * rm/20 + 3],
                        1000000,
                        [1,2,1175,1176,1739,2296,2297,2298,4592,4593,4594,4595,8302,11997,15658,18416]);
    }
}
