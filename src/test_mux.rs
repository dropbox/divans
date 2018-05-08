#![cfg(not(feature="no-stdlib"))]
#![cfg(test)]
use std::vec::Vec;
use alloc::HeapAlloc;
use alloc::Allocator;
use alloc::SliceWrapperMut;
use alloc::SliceWrapper;
use super::mux;


fn help_test_mux(i0:&[u8], i1:&[u8], copy_pattern: &[(mux::StreamID, usize)], in_buf_size: usize, out_buf_size: usize) {
    let mut m8 = HeapAlloc::<u8>::new(0);
    let mut v = Vec::<u8>::new();
    let mut mux = mux::Mux::<HeapAlloc<u8>>::default();
    let mut buf = m8.alloc_cell(in_buf_size);
    let mut input = [i0, i1];
    for (index, copy) in copy_pattern.iter().enumerate() {
        let sl = input[usize::from(copy.0)];
        if sl.len() < copy.1 {
            eprint!("Copy pattern {} beyond index: {} < {}\n",
                    index, sl.len(), copy.1);
            assert!(false);
        }
        let (to_copy, rem) = sl.split_at(copy.1);
        input[usize::from(copy.0)] = rem;
        mux.push_data(copy.0, to_copy, &mut m8);
        let amt = mux.serialize(buf.slice_mut());
        if amt != 0 {
            v.extend(buf.slice().split_at(amt).0);
        }
    }
    loop {
        let amt = mux.serialize_close(buf.slice_mut());
        if amt == 0 {
            break;
        }
        v.extend(buf.slice().split_at(amt).0);
    }
    assert_eq!(mux.is_eof(), true);
    m8.free_cell(buf);
    buf = m8.alloc_cell(out_buf_size);
    input = [i0, i1];
    mux = mux::Mux::<HeapAlloc<u8>>::default();
    let mut dv = &v[..];
    loop {
        let actually_deserialized = mux.deserialize(&dv[..out_buf_size], &mut m8);
        dv = &dv[actually_deserialized..];
        for stream_id in 0..input.len() {
            let to_match_len;
            {
                let to_match = mux.data_avail(stream_id as u8);
                to_match_len = to_match.len();
                let (checkme, rem) = input[stream_id].split_at(to_match.len());
                assert_eq!(to_match, checkme);
                input[stream_id] = rem;
            }
            mux.consume(stream_id as u8, to_match_len);
        }
        if mux.is_eof() {
           break; 
        }
    }
    m8.free_cell(buf);
    assert_eq!(input, [&[], &[]]);
}

fn rand(size: usize, mut seed: u32) -> Vec<u8> {
    let mut ret = vec![seed as u8; size];
    for (index,val) in ret.iter_mut().enumerate() {
        seed = seed.wrapping_add(159871);
        *val = seed as u8;
    }
    ret
}
#[test]
fn test_mux() {
    help_test_mux(&rand(1000000,1)[..],
                  &rand(1000000,2)[..],
                  &[(0,1),(0,1000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (1, 1000000)], 1, 10);
}
