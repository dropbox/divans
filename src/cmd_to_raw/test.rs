// Copyright 2017 Dropbox, Inc
//
//   Licensed under the Apache License, Version 2.0 (the "License");
//   you may not use this file except in compliance with the License.
//   You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
//   Unless required by applicable law or agreed to in writing, software
//   distributed under the License is distributed on an "AS IS" BASIS,
//   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//   See the License for the specific language governing permissions and
//   limitations under the License.

#![cfg(test)]

use super::super::SliceWrapper;
use super::super::BrotliResult;
const TEST_RING_SIZE: usize = 1<<7;
struct ExRingBuffer([u8;TEST_RING_SIZE]);
impl Default for ExRingBuffer {
    fn default() -> Self {
        ExRingBuffer([0u8;TEST_RING_SIZE])
    }
}
impl super::SliceWrapperMut<u8> for ExRingBuffer {
    fn slice_mut(&mut self) -> &mut [u8] {
        return &mut self.0[..];
    }
}
impl super::SliceWrapper<u8> for ExRingBuffer {
    fn slice(&self) -> &[u8] {
        return &self.0[..];
    }
}
#[allow(unused)]
fn make_ring_buffer_state() -> super::DivansRecodeState<ExRingBuffer>{
    super::DivansRecodeState{
        total_offset:0,
        input_sub_offset: 0,
        ring_buffer: ExRingBuffer::default(),
        ring_buffer_decode_index:0,
        ring_buffer_output_index:0,
    }
}
#[allow(unused)]
fn help_ring_buffer_dict(mut state: super::DivansRecodeState<ExRingBuffer>) -> super::DivansRecodeState<ExRingBuffer>{
    for index in 0..6 {
        let ret = state.parse_dictionary(&super::DictCommand{
            word_size:22,
            transform:1,
            final_size:23,
            empty:0,
            word_id:index
        });
        match ret {
            BrotliResult::ResultSuccess => {
                assert!(index < 5);
                state.input_sub_offset = 0; // reset decode
            },
            BrotliResult::NeedsMoreOutput => assert_eq!(index, 5),
            _ => panic!("Unexpected code from dict parsing"),
        }
    }
    let mut flush_buffer = [0u8; 31];
    let mut oindex = 0;
    state.flush(&mut flush_buffer[..], &mut oindex);
    assert_eq!(flush_buffer,
               [100, 101, 115, 99, 114, 105, 112, 116,
                105, 111, 110, 34, 32, 99, 111, 110,
                116, 101, 110, 116, 61, 34, 32, 100,
                111, 99, 117, 109, 101, 110, 116]);
    assert_eq!(oindex, flush_buffer.len());
    oindex = 0;
    state.flush(&mut flush_buffer[..], &mut oindex);
    assert_eq!(oindex, flush_buffer.len());
    assert_eq!(flush_buffer,
               [46, 108, 111, 99, 97, 116, 105, 111,
                110, 46, 112, 114, 111, 116, 32, 46,
                103, 101, 116, 69, 108, 101, 109, 101,
                110, 116, 115, 66, 121, 84, 97]);
    oindex = 3;
    state.flush(&mut flush_buffer[..], &mut oindex);
    assert_eq!(oindex, flush_buffer.len());
    assert_eq!(flush_buffer,
               [46, 108, 111, 103, 78, 97, 109, 101,
                40, 32, 60, 33, 68, 79, 67, 84,
                89, 80, 69, 32, 104, 116, 109, 108,
                62, 10, 60, 104, 116, 109, 108]);
    oindex = 0;
    for item in flush_buffer.iter_mut() {*item = 0;}
    state.flush(&mut flush_buffer[..], &mut oindex);
    assert_eq!(oindex, 25); // only wrote 31 * 3 - 3 + 25 bytes
    assert_eq!(flush_buffer,
               [32, 32, 60, 109, 101, 116, 97, 32,
                99, 104, 97, 114, 115, 101, 116,61,
                34, 117, 116, 102, 45, 56, 34, 62,
                32, 0, 0, 0, 0, 0, 0]);
    for index in 0..6 {
        let ret = state.parse_dictionary(&super::DictCommand{
            word_size:22,
            transform:4,
            final_size:23,
            empty:0,
            word_id:index
        });
        match ret {
            BrotliResult::ResultSuccess => {
                assert!(index < 5);
                state.input_sub_offset = 0;
            },
            BrotliResult::NeedsMoreOutput => assert_eq!(index, 5),
            _ => panic!("Unexpected code from dict parsing"),
        }
    }
    for item in flush_buffer.iter_mut() {*item = 0;}
    let mut oindex = 0;
    state.flush(&mut flush_buffer[..], &mut oindex);
    assert_eq!(flush_buffer,
               [68, 101, 115, 99, 114, 105, 112, 116,
                105, 111, 110, 34, 32, 99, 111, 110,
                116, 101, 110, 116, 61, 34, 32, 68,
                111, 99, 117, 109, 101, 110, 116]);
    assert_eq!(oindex, flush_buffer.len());
    oindex = 0;
    state.flush(&mut flush_buffer[..], &mut oindex);
    assert_eq!(oindex, flush_buffer.len());
    assert_eq!(flush_buffer,
               [46, 108, 111, 99, 97, 116, 105, 111,
                110, 46, 112, 114, 111, 116, 32, 46,
                103, 101, 116, 69, 108, 101, 109, 101,
                110, 116, 115, 66, 121, 84, 97]);
    oindex = 3;
    state.flush(&mut flush_buffer[..], &mut oindex);
    assert_eq!(oindex, flush_buffer.len());
    assert_eq!(flush_buffer,
               [46, 108, 111, 103, 78, 97, 109, 101,
                40, 32, 60, 33, 68, 79, 67, 84,
                89, 80, 69, 32, 104, 116, 109, 108,
                62, 10, 60, 104, 116, 109, 108]);
    oindex = 0;
    for item in flush_buffer.iter_mut() {*item = 0;}
    state.flush(&mut flush_buffer[..], &mut oindex);
    assert_eq!(oindex, 25); // only wrote 31 * 3 - 3 + 25 bytes
    assert_eq!(flush_buffer,
               [32, 32, 60, 109, 101, 116, 97, 32,
                99, 104, 97, 114, 115, 101, 116,61,
                34, 117, 116, 102, 45, 56, 34, 62,
                32, 0, 0, 0, 0, 0, 0]);        
    state
}
#[allow(unused)]
fn help_copy_far(mut state: super::DivansRecodeState<ExRingBuffer>,
             mut buffer: &mut [u8]) -> super::DivansRecodeState<ExRingBuffer> {
    assert!(state.ring_buffer_decode_index == 102); //thhis makes sure we test wraparound
    assert_state_equals_history_buffer(&state, buffer);
    let mut scratch_buffer = [0u8; TEST_RING_SIZE];
    let mut count = 0;
    for _i in 0..4 {
        count += 1;
        match state.parse_copy(&super::CopyCommand{distance:112,
                                                  num_bytes:29}) {
            BrotliResult::NeedsMoreOutput=>{},
            BrotliResult::ResultSuccess=>break,
            res => panic!("UH OH"),
        }
    }
    assert_eq!(count, 3); // this is not necessary for correctness
    // this just asserts that the algorithm broke the job into 3 pieces... the piece
    // that copied the first chunk of data (wrapping around the ring by 10),
    // the piece that copied the second chunk and the rest.
    let mut first_copy_data = [0u8;29];
    let mut first_readout = [0u8;29];
    first_copy_data.clone_from_slice(&buffer[(TEST_RING_SIZE - 112)..(TEST_RING_SIZE - 112 + 29)]);

    scratch_buffer[..(TEST_RING_SIZE - 29)].clone_from_slice(&buffer[29..]);
    scratch_buffer[(TEST_RING_SIZE - 29)..].clone_from_slice(&first_copy_data[..]);
    let mut first_index = 0;
    state.flush(&mut first_readout, &mut first_index);
    assert_eq!(first_index, 29);
    assert_eq!(first_readout, first_copy_data);
    state
}
#[allow(unused)]
fn assert_state_equals_history_buffer(state: &super::DivansRecodeState<ExRingBuffer>,
                                      buffer: &[u8]) {
    for i in 0..TEST_RING_SIZE {
        let ring_index = if (state.ring_buffer_decode_index as usize) <= i {
            state.ring_buffer_decode_index as usize + state.ring_buffer.slice().len() - i - 1
        } else {
            state.ring_buffer_decode_index as usize - i - 1
        };
        let flat_index = TEST_RING_SIZE - 1 - i;
        assert_eq!(buffer[flat_index], state.ring_buffer.slice()[ring_index]);
    }
}
#[allow(unused)]
fn help_copy_near_overlap(mut state: super::DivansRecodeState<ExRingBuffer>,
             buffer: &[u8]) {
    assert!(state.ring_buffer_decode_index == 102); //thhis makes sure we test wraparound
    assert_state_equals_history_buffer(&state, buffer);
    let mut scratch_buffer = [0u8; TEST_RING_SIZE];
    let mut count = 0;
    for _i in 0..4 {
        count += 1;
        match state.parse_copy(&super::CopyCommand{distance:15,
                                                   num_bytes:64}) {
            BrotliResult::NeedsMoreOutput=>{},
            BrotliResult::ResultSuccess=>break,
            res => panic!("UH OH"),
        }
    }
    assert_eq!(count, 1); // this is not necessary for correctness
    // this just asserts that the algorithm did the job in one go
    let mut first_copy_data = [0u8;15];
    let mut first_readout = [0u8;65];
    first_copy_data.clone_from_slice(&buffer[(TEST_RING_SIZE - 15)..TEST_RING_SIZE]);
    
    let mut first_index = 0;
    state.flush(&mut first_readout, &mut first_index);
    assert_eq!(first_index, 64);
    assert_eq!(first_readout[..15], first_copy_data);
    assert_eq!(first_readout[15..30], first_copy_data);
    assert_eq!(first_readout[45..60], first_copy_data);
    assert_eq!(first_readout[60..64], first_copy_data[0..4]);
    assert_eq!(first_readout[64], 0);
}
#[allow(unused)]
fn help_copy_big_overlap(mut state: super::DivansRecodeState<ExRingBuffer>,
             buffer: &[u8]) {
    assert!(state.ring_buffer_decode_index == 102); //thhis makes sure we test wraparound
    assert_state_equals_history_buffer(&state, buffer);
    let mut scratch_buffer = [0u8; TEST_RING_SIZE];
    let mut count = 0;
    for _i in 0..256 {
        count += 1;
        match state.parse_copy(&super::CopyCommand{distance:125,
                                                   num_bytes:258}) {
            BrotliResult::NeedsMoreOutput=>{},
            BrotliResult::ResultSuccess=>panic!("Not enough buffer room"),
            res => panic!("uh oh"),
        }
    }
    assert_eq!(count, 256); // this is not necessary for correctness
    // this just asserts that the algorithm did the job in one go
    let mut first_copy_data = [0u8;125];
    first_copy_data.clone_from_slice(&buffer[(TEST_RING_SIZE - 125)..TEST_RING_SIZE]);
    let mut first_readout = [0u8;128];
    let mut first_index = 0;
    state.flush(&mut first_readout, &mut first_index);
    for _i in 0..64 {
        count += 1;
        match state.parse_copy(&super::CopyCommand{distance:125,
                                                   num_bytes:258}) {
            BrotliResult::NeedsMoreOutput=>{},
            BrotliResult::ResultSuccess=>panic!("Not enough buffer room"),
            res => panic!("uh oh"),
        }
    }
    let mut sec_readout = [0u8;128];
    let mut sec_index = 0;
    state.flush(&mut sec_readout, &mut sec_index);
    assert_eq!(first_index, 127);

    assert_eq!(first_readout[..64], first_copy_data[..64]);
    assert_eq!(first_readout[64..125], first_copy_data[64..]);
    assert_eq!(first_readout[125..127], first_copy_data[..2]);
    assert_eq!(first_readout[..64], first_copy_data[..64]);
    assert_eq!(first_readout[64..125], first_copy_data[64..]);
    assert_eq!(first_readout[125..127], first_copy_data[..2]);

    assert_eq!(sec_index, 127);
    assert_eq!(sec_readout[0..123], first_copy_data[2..125]);
    assert_eq!(sec_readout[123..127], first_copy_data[0..4]);
    for _i in 0..16 {
        count += 1;
        match state.parse_copy(&super::CopyCommand{distance:125,
                                                   num_bytes:258}) {
            BrotliResult::NeedsMoreOutput=>{},
            BrotliResult::ResultSuccess=>break,
            res => panic!("uh oh"),
        }
    }
    let mut last_readout = [0u8;128];
    let mut last_index = 0;
    state.flush(&mut last_readout, &mut last_index);
    assert_eq!(last_index, 4);
    assert_eq!(last_readout[0..4], first_copy_data[4..8]);
}
#[test]
fn test_ring_buffer_dict() {
    help_ring_buffer_dict(make_ring_buffer_state());
}
#[allow(unused)]
static HISTORY_OF_DICT_TEST:[u8; TEST_RING_SIZE] = [
        115, 101, 116,61,
        34, 117, 116, 102, 45, 56, 34, 62,
        32,
        68, 101, 115, 99, 114, 105, 112, 116,
        105, 111, 110, 34, 32, 99, 111, 110,
        116, 101, 110, 116, 61, 34, 32, 68,
        111, 99, 117, 109, 101, 110, 116,
        46, 108, 111, 99, 97, 116, 105, 111,
        110, 46, 112, 114, 111, 116, 32, 46,
        103, 101, 116, 69, 108, 101, 109, 101,
        110, 116, 115, 66, 121, 84, 97,
        /* oindex was 3 on this line above */103, 78, 97, 109, 101,
        40, 32, 60, 33, 68, 79, 67, 84,
        89, 80, 69, 32, 104, 116, 109, 108,
        62, 10, 60, 104, 116, 109, 108,
        32, 32, 60, 109, 101, 116, 97, 32,
        99, 104, 97, 114, 115, 101, 116,61,
        34, 117, 116, 102, 45, 56, 34, 62,
        32];
#[allow(unused)]
struct SimpleSliceWrapper<'a> (&'a [u8]);
impl<'a> SliceWrapper<u8> for SimpleSliceWrapper<'a> {
  fn slice(&self) -> &[u8] {
     self.0
  }
}
#[allow(unused)]
fn help_test_insert(mut state: super::DivansRecodeState<ExRingBuffer>,
                    values_to_insert: &[u8]) -> super::DivansRecodeState<ExRingBuffer> {
    let mut values_to_read = values_to_insert;
    let mut last_readout = [0u8;64];
    let mut last_index = 0;
    let mut done = false;
    while !done {
        state.flush(&mut last_readout, &mut last_index);
        if last_index == 0 {
            match state.parse_literal(&super::LiteralCommand{data:SimpleSliceWrapper(values_to_insert)}) {
                BrotliResult::NeedsMoreOutput=>{},
                BrotliResult::ResultSuccess=>{done=true;},
                res => panic!("uh oh"),
            }               
            state.flush(&mut last_readout, &mut last_index);
        }
        assert_eq!(&last_readout[0..last_index], &values_to_read[..last_index]);
        values_to_read = values_to_read.split_at(last_index).1;
        last_index = 0;
    }
    state
}
#[test]
fn test_copy_far() {
    let state = help_ring_buffer_dict(make_ring_buffer_state());
    let mut prev_buffer = [0u8; TEST_RING_SIZE];
    prev_buffer.clone_from_slice(&HISTORY_OF_DICT_TEST[..]);
    help_copy_far(state, &mut prev_buffer[..]);
}
#[test]
fn test_copy_near_overlap() {
    let state = help_ring_buffer_dict(make_ring_buffer_state());
    let mut prev_buffer = [0u8; TEST_RING_SIZE];
    prev_buffer.clone_from_slice(&HISTORY_OF_DICT_TEST[..]);
    help_copy_near_overlap(state, &mut prev_buffer[..]);
}
#[test]
fn test_copy_big_overlap() {
    let state = help_ring_buffer_dict(make_ring_buffer_state());
    let mut prev_buffer = [0u8; TEST_RING_SIZE];
    prev_buffer.clone_from_slice(&HISTORY_OF_DICT_TEST[..]);
    help_copy_big_overlap(state, &mut prev_buffer[..]);
}
#[test]
fn test_insert_medium() {
    let state = help_ring_buffer_dict(make_ring_buffer_state());
    let values: [u8;31] = [254,255,1,2,3,4,5,6,
                           2,4,6,8,10,12,14,16,
                           3,1,4,1,5,9,2,6,
                           5,3,6,121,122,96,97];
    help_test_insert(state, &values[..]);
}
#[test]
fn test_insert_huge() {
    let state = help_ring_buffer_dict(make_ring_buffer_state());
    let mut values: [u8;512] = [0;512];
    for i in 0..512 {
        values[i] = ((i * 2) & 255) as u8;
    }
    help_test_insert(state, &values[..]);
}
