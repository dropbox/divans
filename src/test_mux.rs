#![cfg(not(feature="no-stdlib"))]
#![cfg(test)]
use core;
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
        let actually_deserialized = mux.deserialize(&dv[..core::cmp::min(out_buf_size, dv.len())], &mut m8);
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
        if dv.len() == 0 {
           break;
        }
    }
    m8.free_cell(buf);
    assert_eq!(input, [&[], &[]]);
    assert_eq!(mux.is_eof(), true);
    mux.free(&mut m8);
}

fn rand(size: usize, mut seed: u32) -> Vec<u8> {
    let mut ret = vec![seed as u8; size];
    for (_index,val) in ret.iter_mut().enumerate() {
        seed = seed.wrapping_add(159871);
        *val = seed as u8;
    }
    ret
}
#[test]
fn test_interleaved_mux() {
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
                   (1, 999986),(0,868986)], 373, 3021);
}

#[test]
fn test_long_mux() {
    help_test_mux(&rand(1000000,1)[..],
                  &rand(1000000,2)[..],
                  &[(0,1),(0,1000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                   (1,1),(0,10000),(1,1),
                    (1, 992986),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,4968),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(0,10000),(1,1),
                   (0,1),(1,4954),(1,1),
                   (0,1),(0,10001),(1,1),
                   (0,1),(0,10002),(1,1),
                   (0,1),(0,10003),(1,1),
                   (0,1),(0,10004),(1,1),
                   (0,1),(0,10005),(1,1),
                   (0,1),(0,10006),(1,1),
                   (0,1),(0,10007),(1,1),
                   (0,1),(0,10008),(1,1),
                   (0,1),(0,10009),(1,1),
                   (0,1),(0,10010),(1,1),
                   (0,1),(0,10011),(1,1),
                   (0,1),(0,10012),(1,1),
                   (0,1),(1,2000),(1,1),
                    (0,613930)], 373, 3021);
}

#[test]
fn test_short_mux() {
    help_test_mux(&[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17][..],
                  &[18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35, 36, 37, 38, 39, 40, 41][..],
                  &[(0,1),(0,10),(1,1),
                   (0,1),(1,16),(0,6),(1,7),
                   ], 1, 1);
}
