use core;
use core::default::Default;
use probability::CDF16;
use super::interface::ArithmeticEncoderOrDecoder;
use super::BrotliResult;
use super::encoder::{
    EntropyEncoder,
    EntropyDecoder,
    ByteQueue,
};

#[cfg(test)]
use std::io::{Write};

#[cfg(test)]
macro_rules! perror(
    ($($val:tt)*) => { {
        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);

#[cfg(not(test))]
macro_rules! perror(
    ($($val:tt)*) => { {
//        writeln!(&mut ::std::io::stderr(), $($val)*).unwrap();
    } }
);

pub struct ANS1 {
    /// ls, number of occurences per symbol
    /// must add up to 1<<BITS
    ls: [u64; 2],
    /// sum of the occurences up to but NOT including the symbol
    /// aka frequency start
    bs: [u64; 2],
    /// sum of the occurences up to AND including the symbol
    /// aka freq end.  ss.last == 1<<BITS
    ss: [u64; 2],
    /// current state
    r: u64,
}

/// total sum of the occurences, last(ss) == 1<<m
const BITS: u64 = 8;
/// max u32 value before flushing
const RANS64_L: u64 = 1u64<<31;

impl ANS1 {
    pub fn reset(&mut self) {
        self.r = 0;
    }
    pub fn update(&mut self, p1: u8) {
        let v1 = p1 as u64;
        let v0 = (1u64<<BITS) - v1;
        assert!(v1 != 0);
        assert!(v0 != 0);
        self.ls = [v0, v1];
        self.bs = [0, v0];
        self.ss = [v0, v0 + v1];
        assert!(self.ss[1] == 1<<BITS);
    }
    pub fn encode_init(&mut self) {
        self.r = RANS64_L;
    }
    pub fn encode(&mut self, bb: bool, dst: &mut [u8], n: &mut usize) {
        let bit = bb as usize;
        let x = self.r;
        assert!(x != 0);        //failed to call encode_init
        let bs = self.bs[bit];  // freq start
        let ls = self.ls[bit];
        let x_max = ((RANS64_L >> BITS) << 32).wrapping_mul(ls);
        let x1 = 
            if x >= x_max {
                *n = *n - 4;
                ANS1::write_u32(x as u32, &mut dst[*n .. *n + 4]);
                x >> 32
            } else {
                x
            };
        assert!(x1 < x_max);
        let r = ((x1 / ls)<<BITS).wrapping_add(bs.wrapping_add(x1 % ls));
        self.r = r;
        //make sure we can decode the encoded bit
        assert!(self.decode().1 == bb);
    }
    ///dst, flush the last 8 bytes of the buffer, should be the front 8 bytes of the encoded data
    pub fn encode_flush(&self, dst: &mut [u8], n: &mut usize) {
        let x0 = self.r as u32;
        let x1 = (self.r >> 32) as u32;
        *n = *n - 8;
        ANS1::write_u32(x0, &mut dst[*n + 0 .. *n + 4]);
        ANS1::write_u32(x1, &mut dst[*n + 4 .. *n + 8]);
    }
    ///dst, should have at least 4 elements left, advance this backwards
    pub fn write_u32(o: u32, dst: &mut [u8]) {
        dst[0] = ((o>>24) & 0xff) as u8;
        dst[1] = ((o>>16) & 0xff) as u8;
        dst[2] = ((o>>8) & 0xff) as u8;
        dst[3] = (o & 0xff) as u8;
    }

    pub fn decode(&mut self) -> (u64, bool) {
        let x = self.r;
        let m = (1<<BITS) - 1;
        let xm = x & m;
        let bit = xm >= self.ss[0];
        let bs = self.bs[bit as usize]; // frequency start
        let ls = self.ls[bit as usize]; // frequency
        let x1 = (ls * (x >> BITS)) + xm - bs;
        return (x1, bit);
    }
    pub fn decode_will_advance(x1: u64) -> bool {
        return x1 < RANS64_L;
    }
    pub fn decode_advance(&mut self, x1: u64, src: &[u8], n: &mut usize) {
        self.r =
            if ANS1::decode_will_advance(x1) {
                let o = ANS1::read_u32(&src[*n .. *n + 4]) as u64;
                *n = *n + 4;
                let x2 = (x1 << 32) | o;
                assert!(x2 >= RANS64_L);
                x2
            } else {
                x1
            };
    }

    pub fn read_u32(src: &[u8]) -> u32 {
        let mut o: u32 = 0;
        o = o | ((src[0] as u32)<<24);
        o = o | ((src[1] as u32)<<16);
        o = o | ((src[2] as u32)<<8);
        o = o | (src[3] as u32);
        return o;
    }
    pub fn read_u64(src: &[u8]) -> u64 {
        let x0 = ANS1::read_u32(&src[0 .. 4]) as u64;
        let x1 = ANS1::read_u32(&src[4 .. 8]) as u64;
        return x0 | (x1 << 32);
    }
    pub fn decode_init(&mut self, src: &[u8], n: &mut usize) {
        self.r = ANS1::read_u64(&src[*n .. *n + 8]);
        *n = *n + 8;
    }
}

impl Default for ANS1 {
    fn default() -> Self {
        return ANS1{ls: [0,0], bs: [0,0], ss: [0,0], r: 0};
    }
}

pub struct EntropyDecoderANS {
    c: ANS1,
    q: CycleQueue,
    len: u16,
}

const MAX_BUFFER_BYTES: usize = 1*1024; // with space for size
const MAX_BUFFER_BITS: usize = MAX_BUFFER_BYTES * 8;

struct BitStack {
    data : [bool; MAX_BUFFER_BITS],
    nbits : usize,
}

pub struct ByteStack {
    data : [u8; MAX_BUFFER_BITS],
    nbytes : usize,
}
const CYCLE_QUEUE_SIZE: usize = 16;
pub struct CycleQueue {
    data : [u8; CYCLE_QUEUE_SIZE],
    start : usize,
    used : usize,
}

impl ByteQueue for CycleQueue {
    fn num_push_bytes_avail(&self) -> usize {
        return self.data.len() - self.used;
    }
    fn num_pop_bytes_avail(&self) -> usize {
        return self.used;
    }
    fn push_data(&mut self, src:&[u8]) -> usize {
        let end = (self.start + self.used) % self.data.len();
        let n = core::cmp::min(src.len(), self.num_push_bytes_avail());
        let ixes = (0 .. self.data.len()).cycle().skip(end);
        for (d,s) in ixes.zip(src.iter().take(n)) {
            self.data[d] = *s;
        }
        self.used = self.used + n;
        return n;
    }
    fn pop_data(&mut self, dst:&mut [u8]) -> usize {
        let n = core::cmp::min(dst.len(), self.used);
        for (s,d) in self.data.iter().cycle().skip(self.start).zip(dst.iter_mut().take(n)) {
            *d = *s;
        }
        self.start = (self.start + n) % self.data.len();
        self.used = self.used - n;
        return n;
    }
}

impl Default for CycleQueue {
    fn default() -> Self {
        return  CycleQueue {data: [0u8; 16], start: 0, used: 0};
    }
}

pub struct EntropyEncoderANS {
    c: ANS1,
    q: ByteStack,
    bits: BitStack,
    probs: ByteStack,
}

impl Default for ByteStack {
    fn default() -> Self {
        let data = [0u8; MAX_BUFFER_BITS];
        return ByteStack {data: data, nbytes: data.len()};
    }
}

impl Default for BitStack {
    fn default() -> Self {
        let data = [false; MAX_BUFFER_BITS];
        return BitStack {data: data, nbits: data.len()};
    }
}

impl ByteStack {
    fn reset(&mut self) {
        self.nbytes = self.data.len();
    }
    fn stack_num_bytes(&self) -> usize {
        return self.data.len() - self.nbytes;
    }
    fn stack_bytes_avail(&self) -> usize {
        return self.nbytes;
    }
    fn stack_data(&mut self, src: &[u8]) {
        for v in src.iter().rev() {
            self.stack_byte(*v);
        }
    }
    fn stack_byte(&mut self, b: u8) {
        assert!(self.nbytes > 0);
        self.nbytes = self.nbytes - 1;
        self.data[self.nbytes] = b;
    }
    fn stack_u16(&mut self, s: u16) {
        self.stack_byte((s & 0xff) as u8);
        self.stack_byte(((s >> 8) & 0xff) as u8);
    }
}

impl ByteQueue for ByteStack {
    fn num_push_bytes_avail(&self) -> usize {
        return self.nbytes;
    }
    fn num_pop_bytes_avail(&self) -> usize {
        return self.data.len() - self.nbytes;
    }
    fn push_data(&mut self, _data:&[u8]) -> usize {
        assert!(false); //only pop from this queue
        return 0;
    }
    fn pop_data(&mut self, data:&mut [u8]) -> usize {
        let n = core::cmp::min(data.len(), self.num_pop_bytes_avail());
        let sl = self.data[self.nbytes .. self.nbytes + n].iter();
        for (d, s) in data.iter_mut().zip(sl) {
            *d = *s;
        }
        self.nbytes = self.nbytes + n;
        return n;
    }
}

impl BitStack {
    fn reset(&mut self) {
        self.nbits = self.data.len();
    }
    fn stack_num_bits(&mut self) -> usize {
        return self.data.len() - self.nbits;
    }
    fn stack_bits_avail(&mut self) -> usize {
        return self.nbits;
    }
    fn stack_bit(&mut self, b: bool) {
        let bit = self.nbits - 1;
        self.data[bit] = b;
        self.nbits = bit;
    }
}

impl Default for EntropyDecoderANS {
    fn default() -> Self {
        let c = ANS1::default();
        let q = CycleQueue::default();
        return EntropyDecoderANS{c: c, q: q, len: 0};
    }
}

impl Default for EntropyEncoderANS {
    fn default() -> Self {
        let mut c = ANS1::default();
        c.encode_init();
        let q = ByteStack::default();
        let b = BitStack::default();
        let p = ByteStack::default();
        assert!(p.stack_bytes_avail() > 0);
        return EntropyEncoderANS{c: c, q: q, bits: b, probs: p};
    }
}

impl EntropyEncoderANS {
    fn encode_bit(c: &mut ANS1, q: &mut ByteStack, bit: bool, prob_of_false: u8) {
        assert!((prob_of_false as u64) != 1u64<<BITS);
        assert!(prob_of_false != 0);
        let p1 = ((1u64<<BITS) - (prob_of_false as u64)) as u8;
        c.update(p1);
        //TODO(anatoly): optimize to use whole words instead of arrays
        let mut dst = [0u8; 4];
        let mut n = dst.len();
        assert!(q.num_push_bytes_avail() >= 4);
        c.encode(bit, &mut dst, &mut n);
        assert!(n == 0 || n == dst.len());
        if n == 0 {
            q.stack_data(&dst);
        }
    }
    //encodes the internal buffer, prefixed with the length of the
    //encoded result
    fn encode_buffer(&mut self) {
        let num = self.bits.stack_num_bits();
        if num > 0 {
            //TODO(anatoly): this can be relaxed
            //pop all bytes before pushing another buffer
            assert!(self.q.stack_num_bytes() == 0);
            {
                //bits and probs should be same size
                assert!(self.probs.stack_num_bytes() == num);
                let bits = self.bits.data.iter();
                let probs = self.probs.data.iter();
                for (b,p) in bits.zip(probs) {
                    EntropyEncoderANS::encode_bit(&mut self.c, &mut self.q, *b,*p);
                }
            }
            perror!("encode pre flush len {}", self.q.stack_num_bytes() as u16);

            let mut dst = [0u8; 8];
            let mut n = dst.len();
            self.c.encode_flush(&mut dst, &mut n);
            perror!("encode R = {:x}", self.c.r);
            assert!(n == 0);
            //NOTE: encode is in reverse, but dst is already in the right order
            //so the first 4 bytes we want the q to pop are the first 4 bytes of dst
            self.q.stack_data(&dst);

            //encode len
            assert!(self.q.stack_num_bytes() < (u16::max_value() as usize));
            let len = self.q.stack_num_bytes() as u16;
            perror!("encode len {}", len);
            self.q.stack_u16(len);

            self.probs.reset();
            self.bits.reset();
            //reset
            self.c.encode_init();
        }
    }
}

/// TODO(anatoly): each chunk can be run in parallel
/// output format:
/// [<size: u16, encoded_buffer: [u8; size]>]
/// This avoids using 2 bytes for buffers that fit into the initial 8 byte state
impl EntropyEncoder for EntropyEncoderANS {
    type Queue = ByteStack;
    fn get_internal_buffer(&mut self) -> &mut ByteStack {
        return &mut self.q;
    }
    fn put_bit(&mut self, bit: bool, mut prob_of_false: u8) {
        if  prob_of_false == 0 {
            prob_of_false = 1;
        }
        self.bits.stack_bit(bit);
        self.probs.stack_byte(prob_of_false);
        if self.probs.stack_bytes_avail() == 0 {
            //stacks are full, encode all the bytes
            assert!(self.bits.stack_bits_avail() == 0);
            self.encode_buffer();
        }
    }
    fn flush(&mut self) {
        self.encode_buffer();
    }
}

impl EntropyDecoderANS {
    fn read_len(&mut self) {
        let mut dst = [0u8; 2];
        assert!(self.q.num_pop_bytes_avail() >= 2);
        let sz = self.q.pop_data(&mut dst);
        assert!(sz == 2);
        let len = ((dst[0] as usize) << 8) | (dst[1] as usize);
        assert!(len <= u16::max_value() as usize);
        self.len = len as u16;
        perror!("decode len = {}", self.len);
    }
    fn read_reg(&mut self) {
        let mut dst = [0u8; 8];
        let mut n = 0;
        assert!(self.q.num_pop_bytes_avail() >= 8);
        let b0 = self.q.pop_data(&mut dst);
        self.len = self.len - 8;
        assert!(b0 == 8);
        self.c.decode_init(&dst, &mut n);
        assert!(n == 8);
        perror!("decode R = {:x}", self.c.r);
    }
}

impl EntropyDecoder for EntropyDecoderANS {
    type Queue = CycleQueue;
    fn get_internal_buffer(&mut self) -> &mut CycleQueue {
        return &mut self.q;
    }
    //TODO(anatoly): clean this up
    fn get_bit(&mut self, mut prob_of_false: u8) -> bool {
        if  prob_of_false == 0 {
            prob_of_false = 1;
        }
        if self.len == 0 && self.c.r <= RANS64_L {
            self.read_len();
            self.read_reg();
        }
        let p1 = ((1<<BITS) - (prob_of_false as u64)) as u8;
        self.c.update(p1);
        let (x1, bit) = self.c.decode();
        let mut dst = [0u8; 4];
        let mut n = 0;
        if ANS1::decode_will_advance(x1) {
            assert!(self.q.num_pop_bytes_avail() >= 4);
            let p = self.q.pop_data(&mut dst);
            assert!(p == 4);
            self.len = self.len - 4;
        }
        self.c.decode_advance(x1, &dst, &mut n);
        assert!(n == 4 || false == ANS1::decode_will_advance(x1));
        return bit;
    }
    fn flush(&mut self) -> BrotliResult {
        self.c.reset();
        self.len = 0;
        return BrotliResult::ResultSuccess;
    }
}

arithmetic_encoder_or_decoder_impl!(EntropyEncoderANS);

#[cfg(test)]
fn init_src(src: &mut [u8]) -> u8 {
    let mut ones = 0u64;
    let seed: [u8; 16] = [0xef, 0xbf,0xff,0xfd,0xef,0x3f,0xc0,0xfd,0xef,0xc0,0xff,0xfd,0xdf,0x3f,0xff,0xfd];
    for (s,v) in seed.iter().cycle().zip(src.iter_mut()) {
        *v = *s;
    }
    for v in src.iter() {
        for i in 0..8 {
            if 1u8<<i & v != 0 {
                ones = ones + 1;
            }
        }
    }
    return ((ones<<BITS) as u64 / (src.len() as u64 * 8)) as u8;
} 

#[cfg(test)]
#[test]
fn rw_u32_test() {
    let mut buf: [u8; 4] = [0; 4];
    let inp: u32 = 0xdeadc0de;
    ANS1::write_u32(inp, &mut buf);
    let out = ANS1::read_u32(&mut buf);
    assert!(inp == out);
}

#[cfg(test)]
fn encode(e: &mut EntropyEncoderANS, p0: u8, src: &[u8], dst: &mut [u8], n: &mut usize) {
    let mut t = 0;
    *n = 0;
    for u in src.iter() {
        let v = *u;
        //left to right
        for i in (0..8).rev() {
            let b: bool = (v & (1u8<<i)) != 0;
            e.put_bit(b, p0);
            let mut q = e.get_internal_buffer(); 
            let qb = q.num_pop_bytes_avail();
            if qb > 0 {
                assert!(qb + *n <= dst.len());
                q.pop_data(&mut dst[*n  .. *n + qb]);
                *n = *n + qb;
            }
            t = t + 1;
        }
    }
    assert!(t == src.len() * 8);
    e.flush();
    {
        let mut q = e.get_internal_buffer(); 
        let qb = q.num_pop_bytes_avail();
        q.pop_data(&mut dst[*n .. *n + qb]);
        *n = *n + qb;
    }
}

#[cfg(test)]
fn decode(d: &mut EntropyDecoderANS, p0: u8, src: &[u8], n: &mut usize, end: &mut [u8]) {
    let mut t = 0;
    {
        let mut q = d.get_internal_buffer(); 
        let sz = q.num_push_bytes_avail();
        assert!(sz >= 10);
        assert!(sz <= 16);
        assert!(src.len() >= sz);
        let p = q.push_data(&src[*n  .. *n + sz]);
        assert!(p == sz);
        assert!(q.num_pop_bytes_avail() == sz);
        *n = *n + sz;
    }
    for v in end.iter_mut() {
        *v = 0;
        for b in 0..8 {
            let bit = d.get_bit(p0);
            if bit {
                *v = *v | (1u8<<(7 - b));
            }
            let mut q = d.get_internal_buffer(); 
            if q.num_push_bytes_avail() > 0 && *n < src.len() {
                let sz = core::cmp::min(src.len() - *n, q.num_push_bytes_avail());
                q.push_data(&src[*n .. *n + sz]);
                *n = *n + sz;
            }
            t = t + 1;
        }
    }
    assert!(t == 8*end.len());
}

#[cfg(test)]
#[test]
fn entropy_trait_test() {
    const SZ: usize = 1024*4;
    let mut d = EntropyDecoderANS::default();
    let mut e = EntropyEncoderANS::default();
    let mut src: [u8; SZ] = [0; SZ];
    let mut dst: [u8; SZ] = [0; SZ];
    let mut n: usize = 0;
    let mut end: [u8; SZ] = [0; SZ];
    let prob = init_src(&mut src);
    let prob0: u8 = ((1u64<<BITS) - (prob as u64)) as u8;
    let mut start = [0u8; SZ];
    start.clone_from_slice(src.iter().as_slice());
    encode(&mut e, prob0, &src, &mut dst, &mut n);
    perror!("encoded size: {}", n);

    let nbits = n * 8;
    let z = SZ as f64 * 8.0;
    let p1 = prob as f64 / 256.0; 
    let p0 = 1.0 - p1;
    let optimal = -1.0 * p1.log2() * (p1 * z) + (-1.0) * p0.log2() * (p0 * z);
    let actual = nbits as f64;
    assert!(actual >= optimal);
    perror!("effeciency: {}", actual / optimal);
    n = 0;
    decode(&mut d, prob0, &dst, &mut n, &mut end);
    let mut t = 0;
    for (e,s) in end.iter().zip(start.iter()) {
        assert!(e == s, "byte {} mismatch {:b} != {:b} ", t, e, s);
        t = t + 1;
    }
    assert!(t == SZ);
    perror!("done!");
}

#[cfg(test)]
#[test]
fn cyclequeue_test() {
    let mut c = CycleQueue::default();
    for v in 0 .. (CYCLE_QUEUE_SIZE + 1) {
        assert!(c.num_pop_bytes_avail() == 0);
        assert!(c.num_push_bytes_avail() == CYCLE_QUEUE_SIZE);
        for t in 0 .. v {
            let d = [t as u8];
            assert!(1 == c.push_data(&d))
        }
        assert!(c.num_pop_bytes_avail() == v);
        assert!(c.num_push_bytes_avail() == CYCLE_QUEUE_SIZE - v);
        for t in 0 .. v {
            let mut d = [0u8; 1];
            assert!(1 == c.pop_data(&mut d));
            assert!(d[0] == t as u8);
        }
        assert!(c.num_push_bytes_avail() == CYCLE_QUEUE_SIZE);
        assert!(c.num_pop_bytes_avail() == 0);
    }
}

#[cfg(test)]
#[test]
fn ans1_test() {
    const SZ: usize = 1024*4;
    let mut c = ANS1::default();
    let mut src: [u8; SZ] = [0; SZ];
    let mut dst: [u8; SZ] = [0; SZ];
    let mut n: usize;
    let mut end: [u8; SZ] = [0; SZ];
    let mut t = 0;
    let prob: u8 = init_src(&mut src);
    c.update(prob);
    let mut start = [0u8; SZ];
    start.clone_from_slice(src.iter().as_slice());
    c.encode_init();
    n = dst.len();
    for u in src.iter().rev() {
        let v = *u;
        for i in 0..8 {
            let b: bool = (v & (1u8<<i)) != 0;
            c.encode(b, &mut dst, &mut n);
            t = t + 1;
        }
    }
    assert!(t == SZ * 8);
    assert!(n >= 8);
    c.encode_flush(&mut dst, &mut n);

    let nbits = (dst.len() - n) * 8;
    let z = SZ as f64 * 8.0;
    let p1 = prob as f64 / 256.0; 
    let p0 = 1.0 - p1;
    let optimal = -1.0 * p1.log2() * (p1 * z) + (-1.0) * p0.log2() * (p0 * z);
    let actual = nbits as f64;
    assert!(actual >= optimal);
    perror!("effeciency: {}", actual / optimal);
    c.decode_init(&dst, &mut n);
    t = 0;
    for v in end.iter_mut() {
        *v = 0;
        for b in 0..8 {
            let (xn, bit) = c.decode();
            if bit {
                *v = *v | (1u8<<(7 - b));
            }
            c.decode_advance(xn, &dst, &mut n);
            t = t + 1;
        }
    }
    assert!(t == SZ * 8);
    t = 0;
    for (e,s) in end.iter().zip(start.iter()) {
        assert!(e == s, "byte {} mismatch {:b} != {:b} ", t, e, s);
        t = t + 1;
    }
    assert!(t == SZ);
}
