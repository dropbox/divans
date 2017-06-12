#![allow(unused)]

type Prob = i16; // can be i32

trait CDFUpdater {
    fn blend(&mut self, symbol: u8);
    fn init(data:&mut[Prob; 16]);
    fn max(data:&[Prob; 16]) -> i64;
    fn log_max(data:&[Prob; 16]) -> Option<i64>;
}

#[derive(Clone, Debug)]
pub struct CDF16([Prob;16]);
const CDF_BITS : usize = 15; // 15 bits
const CDF_MAX : Prob = 32767; // last value is implicitly 32768
const CDF_LIMIT : i64 = CDF_MAX as i64 + 1;
impl CDF16 {
    pub fn valid(&self) -> bool { // nonzero everywhere
        let prev = self.0[0];
        if prev != 0 {
            return false;
        }
        for item in self.0[1..].iter() {
            if *item <= prev || !(*item <= CDF_MAX) {
                return false;
            }
        }
        return true;
    }
    pub fn float_array(&self) -> [f32; 16]{
        let mut ret = [0.0f32; 16];
        for i in 0..15 {
            ret[i] = ((self.0[i+1] - self.0[i]) as f32) / CDF_LIMIT as f32;
        }
        ret[15] = ((CDF_LIMIT - self.0[15] as i64) as f32) / CDF_LIMIT as f32;
        for i in 0..16 {
            ret[i] *= 16.0f32;
        }
        ret
    }
}
impl Default for CDF16 {
    fn default() -> CDF16 {
        const DEFAULT : CDF16 =
            CDF16([0,
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
             (15 * CDF_LIMIT / 16) as Prob]);
        DEFAULT 
    }
}
#[allow(unused)]
macro_rules! each16{
    ($src: expr, $dst: expr, $func: expr) => {
        for i in 0..16 {
            $dst[i] = $func($src[i]);
        }
    }
}
#[allow(unused)]
macro_rules! set1 {
    ($src: expr, $val: expr) =>{
        CDF16([$val; 16])
    }
}
macro_rules! each16bin {
    ($src0 : expr, $src1 : expr, $func: expr) => {
    CDF16([$func($src0[0], $src1[0]),
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
           $func($src0[15], $src1[15])])
    }
}
fn gt(a:Prob, b:Prob) -> Prob {
    (-((a > b) as i64)) as Prob
}
fn and(a:Prob, b:Prob) -> Prob {
    a & b
}
fn add(a:Prob, b:Prob) -> Prob {
    a + b
}

const BLEND_FIXED_POINT_PRECISION : i8 = 15;

pub fn blend(baseline :CDF16, symbol: u8, blend : i32, bias : i32) ->CDF16 {
    debug_assert!(baseline.valid());
    const SCALE :i32 = 1i32 << BLEND_FIXED_POINT_PRECISION;
    let to_blend = to_blend_lut(symbol);
    debug_assert!(to_blend.valid());
    let mut epi32:[i32;8] = [to_blend.0[0] as i32,
                        to_blend.0[1] as i32,
                        to_blend.0[2] as i32,
                        to_blend.0[3] as i32,
                        to_blend.0[4] as i32,
                        to_blend.0[5] as i32,
                        to_blend.0[6] as i32,
                        to_blend.0[7] as i32];
    let scale_minus_blend = SCALE - blend;
    for i in 0..8 {
        epi32[i] *= blend;
        epi32[i] += baseline.0[i] as i32 * scale_minus_blend + bias;
        epi32[i] >>= BLEND_FIXED_POINT_PRECISION;
    }
    let mut retval : CDF16 = CDF16([epi32[0] as i16,
                                epi32[1] as i16,
                                epi32[2] as i16,
                                epi32[3] as i16,
                                epi32[4] as i16,
                                epi32[5] as i16,
                                epi32[6] as i16,
                                epi32[7] as i16,
                                0,0,0,0,0,0,0,0]);
    let mut epi32:[i32;8] = [to_blend.0[8] as i32,
                        to_blend.0[9] as i32,
                        to_blend.0[10] as i32,
                        to_blend.0[11] as i32,
                        to_blend.0[12] as i32,
                        to_blend.0[13] as i32,
                        to_blend.0[14] as i32,
                        to_blend.0[15] as i32];
    for i in 8..16 {
        epi32[i - 8] *= blend;
        epi32[i - 8] += baseline.0[i] as i32 * scale_minus_blend + bias;
        retval.0[i] = (epi32[i - 8] >> BLEND_FIXED_POINT_PRECISION) as i16;
    }
    retval
}

fn to_blend(symbol: u8) -> CDF16 {
    let delta: Prob = CDF_MAX - 15;
    const CDF_INDEX : CDF16 = CDF16([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]);
    let symbol16 = CDF16([symbol as i16; 16]);
    let delta16 = CDF16([delta; 16]);
    let mask_symbol = each16bin!(CDF_INDEX.0, symbol16.0, gt);
    let add_mask = each16bin!(delta16.0, mask_symbol.0, and);
    let to_blend = each16bin!(CDF_INDEX.0, add_mask.0, add);
    to_blend
}

fn to_blend_lut(symbol: u8) -> CDF16 {
    const DEL: Prob = CDF_MAX - 15;
    static CDF_SELECTOR : [CDF16;16] = [
        CDF16([0,1+DEL,2+DEL,3+DEL,4+DEL,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2+DEL,3+DEL,4+DEL,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3+DEL,4+DEL,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4+DEL,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5+DEL,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6+DEL,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7+DEL,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8+DEL,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8,9+DEL,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8,9,10+DEL,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8,9,10,11+DEL,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8,9,10,11,12+DEL,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8,9,10,11,12,13+DEL,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14+DEL,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15+DEL]),
        CDF16([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15])];
    CDF_SELECTOR[symbol as usize].clone()
}

mod test {
    #[test]
    fn test_blend_lut() {
        for i in 0..16 {
            let a = super::to_blend(i as u8);
            let b = super::to_blend_lut(i as u8);
            for j in 0..16 {
                assert_eq!(a.0[j], b.0[j]);
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
                          desired_outcome : super::CDF16) {
        let mut sum : u32 = 0;
        for i in 0..16 {
            rand_table[i] += sum;
            sum = rand_table[i];
        }
        assert_eq!(sum, RAND_MAX + 1);
        let mut prob_state = super::CDF16::default();
        // make sure we have all probability taken care of
        let mut seed = 1u64;
        for i in 0..num_trials {
            let rand_num = simple_rand(&mut seed) as u32;
            for j in 0..16{
                if rand_num < rand_table[j] {
                    // we got an j as the next symbol
                    prob_state = super::blend(prob_state,
                                       j as u8,
                                       blend,
                                              bias);
                    assert!(prob_state.valid());
                    break;
                }
                assert!(j != 15); // should have broken
            }
        }
        //assert_eq!(prob_state.float_array(), [0.0f32;16]);
        assert_eq!(prob_state.0, desired_outcome.0);
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
                           super::CDF16([0,1,2,1605,1606,2728,3967,3968,3969,7880,7881,7882,7883,14150,20830,27071]));
                            
    }
}
