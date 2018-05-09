// this handles the lepton-format io muxer


use core;
use alloc::{Allocator, SliceWrapper, SliceWrapperMut};
use super::slice_util;
pub use interface::{StreamID, StreamMuxer, StreamDemuxer};
const NUM_STREAMS: StreamID = 2;
const STREAM_ID_MASK: StreamID = 0x1;
const MAX_HEADER_SIZE: usize = 3;
const MAX_FLUSH_VARIANCE: usize = 131073;
enum BytesToDeserialize {
    None,
    Some(StreamID, u32),
    Header0(StreamID),
    Header1(StreamID, u8),
}
enum StreamState {
    Running,
    EofStart,
    EofMid,
    EofDone,
}




pub struct Mux<AllocU8:Allocator<u8> > {
   buf: [AllocU8::AllocatedMemory; NUM_STREAMS as usize],
   read_cursor: [usize; NUM_STREAMS as usize],
   write_cursor: [usize; NUM_STREAMS as usize],
   cur_stream_bytes_avail: u32,
   cur_stream:StreamID,
   last_flush:[usize; NUM_STREAMS as usize],
   bytes_flushed: usize,
   bytes_to_deserialize:BytesToDeserialize,
   eof: StreamState,
}

fn chunk_size(last_flushed:usize, lagging_stream: bool) -> usize {
    if lagging_stream  {
        return 16;
    }
    if last_flushed <= 1024 {
        return 4096;
    }
    if last_flushed <= 65536 {
        return 16384;
    }
    return 65536;
}
#[derive(Debug)]
enum MuxSliceHeader {
    Var([u8;MAX_HEADER_SIZE]),
    Fixed([u8;1]),
}
pub const EOF_MARKER: [u8;3] = [0xff, 0xfe, 0xff];
fn get_code(stream_id: StreamID, bytes_to_write: usize, is_lagging: bool) -> (MuxSliceHeader, usize) {
    if is_lagging == false || bytes_to_write == 4096 || bytes_to_write == 16384 || bytes_to_write >= 65536 {
        if bytes_to_write < 4096 {
            return get_code(stream_id, bytes_to_write, true);
        }
        if bytes_to_write < 16384 {
            return (MuxSliceHeader::Fixed([stream_id  as u8 | (1 << 4)]), 4096);
        }
        if bytes_to_write < 65536 {
            return (MuxSliceHeader::Fixed([stream_id  as u8 | (2 << 4)]), 16384);
        }
        return (MuxSliceHeader::Fixed([stream_id  as u8 | (3 << 4)]), 65536);
    }
    assert!(bytes_to_write < 65536);
    let ret = [stream_id as u8,
               (bytes_to_write - 1) as u8 & 0xff,
               ((bytes_to_write - 1)>> 8) as u8 & 0xff];
    return (MuxSliceHeader::Var(ret), bytes_to_write);
}
impl<AllocU8:Allocator<u8> > Default for Mux<AllocU8> {
    fn default() -> Self {
        Mux::<AllocU8> {
            bytes_to_deserialize:BytesToDeserialize::None,
            cur_stream: 0,
            cur_stream_bytes_avail: 0,
            eof:StreamState::Running,
            buf:[
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
/*              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),
              AllocU8::AllocatedMemory::default(),*/
            ],
            read_cursor:[0;NUM_STREAMS as usize],
            write_cursor:[0;NUM_STREAMS as usize],
            last_flush:[0;NUM_STREAMS as usize],
            bytes_flushed: 0,
        }
    }
}
impl<AllocU8: Allocator<u8> > StreamDemuxer<AllocU8> for Mux<AllocU8>{
    fn write_linear(&mut self, data:&[u8], m8: &mut AllocU8) -> usize {
        self.deserialize(data, m8)
    }
    fn data_ready(&self, stream_id:StreamID) -> usize {
        self.how_much_data_avail(stream_id)
    }
    fn peek(&self, stream_id: StreamID) -> &[u8] {
        self.data_avail(stream_id)
    }
    fn pop(&mut self, stream_id: StreamID) -> slice_util::AllocatedMemoryRange<u8, AllocU8> {
        let ret = slice_util::AllocatedMemoryRange::<u8, AllocU8>(core::mem::replace(&mut self.buf[usize::from(stream_id)],
                                                                                 AllocU8::AllocatedMemory::default()),
                                                              self.read_cursor[usize::from(stream_id)]..self.write_cursor[usize::from(stream_id)]);
        self.read_cursor[usize::from(stream_id)] = 0;
        self.write_cursor[usize::from(stream_id)] = 0;
        ret
    }
    fn consume(&mut self, stream_id: StreamID, count: usize) {
        self.consume_data(stream_id, count)
    }
    fn encountered_eof(&self) -> bool {
        self.is_eof()
    }
    fn free_demux(&mut self, m8: &mut AllocU8) {
        self.free(m8)
    }
}


impl<AllocU8:Allocator<u8> > StreamMuxer<AllocU8> for Mux<AllocU8> {
    fn write(&mut self, stream_id: StreamID, data: &[u8], m8: &mut AllocU8) -> usize {
        self.push_data(stream_id, data, m8);
        data.len()
    }
    fn linearize(&mut self, output:&mut[u8]) -> usize {
        self.serialize(output)
    }
    fn close(&mut self, output:&mut[u8]) -> usize {
        self.serialize_close(output)
    }
    fn wrote_eof(&self) -> bool {
        self.is_eof()
    }
    fn free_mux(&mut self, m8: &mut AllocU8) {
        self.free(m8);
    }
}
impl<AllocU8:Allocator<u8>> Mux<AllocU8> {
   #[inline(always)]
   pub fn how_much_data_avail(&self, stream_id: StreamID) -> usize {
       self.write_cursor[usize::from(stream_id)] - self.read_cursor[usize::from(stream_id)]
   }
   #[inline(always)]
   pub fn data_avail(&self, stream_id: StreamID) -> &[u8] {
      &self.buf[usize::from(stream_id)].slice()[self.read_cursor[usize::from(stream_id)]..self.write_cursor[usize::from(stream_id)]]
   }
   pub fn consume_data(&mut self, stream_id: StreamID, count: usize) {
      self.read_cursor[usize::from(stream_id)] += count;
   }
    pub fn is_eof(&self) -> bool {
        for index in 0..NUM_STREAMS as usize {
            if self.read_cursor[index]  != self.write_cursor[index] {
                return false;
            }
        }
        match self.eof {
            StreamState::EofDone => true,
            _ => false,
        }
   }
   fn unchecked_push(buf: &mut[u8], write_cursor: &mut usize, data: &[u8]) {
       buf.split_at_mut(*write_cursor).1.split_at_mut(data.len()).0.clone_from_slice(data);
       *write_cursor += data.len();
   }
   pub fn prealloc(&mut self, m8: &mut AllocU8, amount_per_stream: usize) {
       for buf in self.buf.iter_mut() {
           assert_eq!(buf.slice().len(), 0);
           let mfd = core::mem::replace(buf, m8.alloc_cell(amount_per_stream));
           m8.free_cell(mfd);
       }
   }
   pub fn free(&mut self, m8: &mut AllocU8) {
      for buf in self.buf.iter_mut() {
          m8.free_cell(core::mem::replace(buf, AllocU8::AllocatedMemory::default()));
      }        
   }
   // this pushes data from a source into the stream buffers. This data may later be serialized through serialize()
   // or else consumed through data_avail/consume
   pub fn push_data(&mut self, stream_id: StreamID, data: &[u8], m8: &mut AllocU8) {
      let write_cursor = &mut self.write_cursor[stream_id as usize];
      let read_cursor = &mut self.read_cursor[stream_id as usize];
      //let mut write_cursor = &mut self.write_cursor[stream_id as usize];
      let buf = &mut self.buf[usize::from(stream_id)];
      // if there's space in the buffer, simply copy it in
      if buf.slice().len() - *write_cursor >= data.len() {
          Self::unchecked_push(buf.slice_mut(), write_cursor, data);
          return;
      }
      // if there's too much room at the beginning and the new data fits, then move everything to the beginning and write in the new data
      if buf.slice().len() + (*write_cursor - *read_cursor) >= data.len() && (*read_cursor == *write_cursor
                                                                              || (*read_cursor >= 16384 && *read_cursor > *write_cursor - *read_cursor + MAX_HEADER_SIZE)) {

          {
              let (unbuffered_empty_half, full_half) = buf.slice_mut().split_at_mut(*read_cursor);
              let empty_half = unbuffered_empty_half.split_at_mut(MAX_HEADER_SIZE).1; // leave some room on the beginning side for header data to be flushed
              let amount_of_data_to_copy = *write_cursor - *read_cursor;
              *write_cursor = MAX_HEADER_SIZE + amount_of_data_to_copy;
              *read_cursor = MAX_HEADER_SIZE;
              empty_half.split_at_mut(amount_of_data_to_copy).0.clone_from_slice(
                  full_half.split_at(amount_of_data_to_copy).0);
          }
          Self::unchecked_push(buf.slice_mut(), write_cursor, data);
          return;
      }
      // find the next power of two buffer size that could hold everything including the recently added data
      let desired_size:u64 = (MAX_HEADER_SIZE + data.len() + (*write_cursor - *read_cursor)) as u64;
      let log_desired_size = (64 - desired_size.leading_zeros()) + 1;
      // allocate the new data and then copy in the current data
      let mut new_buf = m8.alloc_cell(1 << core::cmp::max(log_desired_size, 9));
      debug_assert!(new_buf.slice().len() >= *write_cursor - *read_cursor + data.len());
      new_buf.slice_mut().split_at_mut(MAX_HEADER_SIZE).1.split_at_mut(*write_cursor - *read_cursor).0.clone_from_slice(
          buf.slice().split_at(*read_cursor).1.split_at(*write_cursor - *read_cursor).0);
      *write_cursor = MAX_HEADER_SIZE + *write_cursor - *read_cursor;
      *read_cursor = MAX_HEADER_SIZE;
      m8.free_cell(core::mem::replace(buf, new_buf));
      // finally copy in the input data
      Self::unchecked_push(buf.slice_mut(), write_cursor, data);
   }
   // copy the remaining data from a previous serialize
   fn serialize_leftover(&mut self, output:&mut[u8]) -> usize {
       let to_copy = core::cmp::min(self.cur_stream_bytes_avail as usize, output.len());
       output.split_at_mut(to_copy).0.clone_from_slice(
           self.buf[usize::from(self.cur_stream)].slice().split_at(self.read_cursor[usize::from(self.cur_stream)]).1.split_at(to_copy).0);
       self.read_cursor[usize::from(self.cur_stream)] += to_copy;
       self.cur_stream_bytes_avail -= to_copy as u32;
       to_copy
   }
    fn serialize_stream_id(&mut self, stream_id: StreamID, output: &mut [u8], output_offset: &mut usize, is_lagging: bool) {
        let outputted_cursor = &mut self.read_cursor[usize::from(stream_id)];
        let populated_cursor = &mut self.write_cursor[usize::from(stream_id)];
        let buf = self.buf[usize::from(stream_id)].slice_mut();
        // find the header and number of bytes that should be written to it
        let (header, mut num_bytes_should_write) = get_code(stream_id, *populated_cursor - *outputted_cursor, is_lagging);
        //eprint!("{}) header {:?} bytes: {}\n", stream_id, header, num_bytes_should_write);
        self.bytes_flushed += num_bytes_should_write;
        assert!(*outputted_cursor >= MAX_HEADER_SIZE);
        match header {
            MuxSliceHeader::Var(hdr) =>  {
                // add on the number of bytes that should be written
                num_bytes_should_write += hdr.len();
                // subtract the location of the buffer...this should not bring us below zero
                *outputted_cursor -= hdr.len();
                for i in 0..hdr.len(){
                    buf[*outputted_cursor + i] = hdr[i];
                }
            },
            MuxSliceHeader::Fixed(hdr) => {
                num_bytes_should_write += hdr.len();
                *outputted_cursor -= hdr.len();
                for i in 0..hdr.len(){
                    buf[*outputted_cursor + i] = hdr[i];
                }
            }
        }
        // set bytes_flushed to the end of the desired bytes to flush, so we know this stream isn't lagging too badly
        self.last_flush[usize::from(stream_id)] = self.bytes_flushed;
        // compute the number of bytes that will fit into otput
        let to_write = core::cmp::min(num_bytes_should_write, output.len() - *output_offset);
        output.split_at_mut(*output_offset).1.split_at_mut(to_write).0.clone_from_slice(
            buf.split_at(*outputted_cursor).1.split_at(to_write).0);
        *outputted_cursor += to_write;
        // if we have produced everything from this stream, reset the cursors to the beginning to support quick copies
        if *outputted_cursor == *populated_cursor {
            *outputted_cursor = MAX_HEADER_SIZE;
            *populated_cursor = *outputted_cursor; // reset cursors to the beginning of the buffer
        }
        *output_offset += to_write;
        // we have some leftovers that would not fit into the output buffer..store these for the next serialize_leftovers call
        if to_write != num_bytes_should_write {
            self.cur_stream_bytes_avail = (num_bytes_should_write - to_write) as u32;
            self.cur_stream = stream_id as StreamID;
        }
    }
    pub fn deserialize_eof(&mut self, mut input: &[u8]) -> usize {
        let mut ret = 0usize;
        assert_eq!(EOF_MARKER.len(), 3);
        match self.eof {
            StreamState::Running => {
                if input[0] == EOF_MARKER[0] {
                    ret += 1;
                    input = input.split_at(1).1;
                    self.eof = StreamState::EofStart;
                }
            }
            _ => {},
        }
        if input.len() == 0 {
            return ret;
        }
        match self.eof {
            StreamState::EofStart => {
                if input[0] == EOF_MARKER[1] {
                    ret += 1;
                    input = input.split_at(1).1;
                    self.eof = StreamState::EofMid
                }
            }
            _ => {},
        } 
        if input.len() == 0 {
            return ret;
        }
        match self.eof {
            StreamState::EofMid => {
                if input[0] == EOF_MARKER[2] {
                    ret += 1;
                    self.eof = StreamState::EofDone;
                    return ret;
                }
            }
            _ => {},
        }
        return ret;
    }
    pub fn deserialize(&mut self, mut input:&[u8], m8: &mut AllocU8) -> usize {
        let mut ret = 0usize;
        while input.len() != 0 && match self.eof {StreamState::EofDone => false, _ => true} {
            match self.bytes_to_deserialize {
                BytesToDeserialize::Header0(stream_id) => {
                    self.bytes_to_deserialize = BytesToDeserialize::Header1(stream_id, input[0]);
                    return ret + 1 + self.deserialize(input.split_at(1).1, m8);
                },
                BytesToDeserialize::Header1(stream_id, lsb) => {
                    self.bytes_to_deserialize = BytesToDeserialize::Some(stream_id, (lsb as u32 | (input[0] as u32) << 8) + 1);
                    //eprint!("{}) Deserializing {}\n", stream_id, (lsb as u32 | (input[0] as u32) << 8) + 1);
                    return ret + 1 + self.deserialize(input.split_at(1).1, m8);
                },
                BytesToDeserialize::Some(stream_id, count) => {
                    if count as usize > input.len() {
                        self.push_data(stream_id, input, m8);
                        self.bytes_to_deserialize = BytesToDeserialize::Some(stream_id, count - input.len() as u32);
                        return ret + input.len();
                    }
                    let (to_push, remainder) = input.split_at(count as usize);
                    self.push_data(stream_id, to_push, m8);
                    input = remainder;
                    self.bytes_to_deserialize = BytesToDeserialize::None;
                    ret += to_push.len();
                }
                BytesToDeserialize::None => {
                    if input[0] == EOF_MARKER[0] || input[0] == EOF_MARKER[1] || input[0] == EOF_MARKER[2] {
                        if input[0] == EOF_MARKER[0] || match self.eof {
                            StreamState::Running => false,
                            _ => true,
                        } {
                            //eprint!("DESERIALIZING EOF\n");
                            return ret + self.deserialize_eof(input);
                        }
                    }
                    let stream_id = input[0] & STREAM_ID_MASK;
                    let count: usize;
                    let bytes_to_copy: u32;
                    if input[0] < 16 {
                        if input.len() < 3 {
                        self.bytes_to_deserialize=BytesToDeserialize::Header0(stream_id);
                            return ret + 1 + self.deserialize(input.split_at(1).1, m8); 
                        }
                        count = 3;
                        bytes_to_copy = (input[1] as u32 | (input[2] as u32) << 8) + 1;
                    } else {
                        count = 1;
                        bytes_to_copy = 1024 << ((input[0] >> 4) << 1);
                    }
                    //eprint!("{}) Deserializing {}\n", stream_id, bytes_to_copy);
                    self.bytes_to_deserialize = BytesToDeserialize::Some(stream_id, bytes_to_copy);
                    input = input.split_at(count).1;
                    ret += count;
                },
            }
        }
        ret
    }
    pub fn serialize(&mut self, output:&mut [u8]) -> usize {
        let mut output_offset = 0usize;
        if self.cur_stream_bytes_avail != 0 {
           output_offset += self.serialize_leftover(output);
        }
        while output_offset < output.len() {
           let mut flushed_any = false;
           let mut last_flush = self.last_flush[0];
           let mut max_flush = self.last_flush[0];
           for lf in self.last_flush[1..].iter() {
               if *lf < last_flush {
                  last_flush = *lf;
               }
               if *lf > max_flush {
                   max_flush = *lf;
               }
           }
           for index in 0..(NUM_STREAMS as usize) {
               let mut is_lagging = max_flush  > MAX_FLUSH_VARIANCE + self.last_flush[index];
               if self.write_cursor[index] - self.read_cursor[index] >= chunk_size(self.last_flush[index],
                                                             is_lagging) && self.last_flush[index] <= last_flush + MAX_FLUSH_VARIANCE {
                   flushed_any = true;
                   self.serialize_stream_id(index as u8, output, &mut output_offset, is_lagging);
                   if self.cur_stream_bytes_avail != 0 {
                       break;
                   }
               }
           }
           if !flushed_any {
             break;
           }
        }
        output_offset
    }
    pub fn serialize_close(&mut self, output:&mut [u8]) -> usize {
        match self.eof {
            StreamState::EofDone => return 0,
            _ => {},
        }
        let mut ret = self.flush(output);
        if output.len() == ret {
            return ret;
        }
        match self.eof {
            StreamState::Running => {
                output[ret] = EOF_MARKER[0];
                ret += 1;
                self.eof = StreamState::EofStart;
            }
            _ => {},
        }
        if output.len() == ret {
            return ret;
        }
        match self.eof {
            StreamState::EofStart => {
                output[ret] = EOF_MARKER[1];
                ret += 1;
                self.eof = StreamState::EofMid;
            }
            _ => {},
        }
        if output.len() == ret {
            return ret;
        }
        match self.eof {
            StreamState::EofMid => {
                output[ret] = EOF_MARKER[2];
                ret += 1;
                self.eof = StreamState::EofDone;
            }
            _ => {},
        }
        return ret;
    }
    fn flush(&mut self, output:&mut [u8]) -> usize {
        let mut output_offset = 0usize;
        if self.cur_stream_bytes_avail != 0 {
            output_offset += self.serialize_leftover(output);
        }
        while output_offset < output.len() {
            let mut flushed_any = false;
            let mut last_flush: Option<usize> = None;
            for (lf, (rc, wc)) in self.last_flush.iter().zip(self.read_cursor.iter().zip(self.write_cursor.iter())) {
                if match last_flush {
                    None => *rc != *wc, // only consider this item for being the last flush point if it has data to flush
                    Some(last_flush_some) => *lf < last_flush_some && *rc != *wc,
                } {
                    last_flush = Some(*lf);
                }
            }
            for index in 0..(NUM_STREAMS as usize) {
                if match last_flush {
                    None => true,
                    Some(last_flush_some) => self.last_flush[index] <= last_flush_some + MAX_FLUSH_VARIANCE,
                } {
                    let mut written = output_offset;
                    if self.read_cursor[index] != self.write_cursor[index] {
                        self.serialize_stream_id(index as u8, output, &mut written, true);
                    }
                    if written != output_offset {
                        flushed_any = true;
                    }
                    output_offset = written;
                    if self.cur_stream_bytes_avail != 0 {
                        break;
                    }
                }
            }
            if !flushed_any {
                break;
            }
        }
        output_offset
    }
}

pub struct DevNull<AllocU8:Allocator<u8>> {
    _placeholder: core::marker::PhantomData<AllocU8>
}
impl<AllocU8: Allocator<u8> > Default for DevNull<AllocU8>{
    fn default() ->Self {
        DevNull::<AllocU8> {
            _placeholder: core::marker::PhantomData::<AllocU8>::default(),
        }
    }
}
impl<AllocU8: Allocator<u8> > StreamDemuxer<AllocU8> for DevNull<AllocU8>{
    fn write_linear(&mut self, data:&[u8], _m8: &mut AllocU8) -> usize {
        debug_assert_eq!(data.len(), 0);
        0
    }
    fn data_ready(&self, _stream_id:StreamID) -> usize {
        0
    }
    fn peek(&self, _stream_id: StreamID) -> &[u8] {
        &[]
    }
    fn pop(&mut self, _stream_id: StreamID) -> slice_util::AllocatedMemoryRange<u8, AllocU8> {
        slice_util::AllocatedMemoryRange::<u8, AllocU8>::default()
    }
    fn consume(&mut self, _stream_id: StreamID, count: usize) {
        debug_assert_eq!(count, 0);
    }
    fn encountered_eof(&self) -> bool {
        true
    }
    fn free_demux(&mut self, _m8: &mut AllocU8) {
    }
}


impl<AllocU8:Allocator<u8> > StreamMuxer<AllocU8> for DevNull<AllocU8> {
    fn write(&mut self, _stream_id: StreamID, data: &[u8], _m8: &mut AllocU8) -> usize {
        debug_assert_eq!(data.len(), 0);
        0
    }
    fn linearize(&mut self, _output:&mut[u8]) -> usize {
        0
    }
    fn close(&mut self, _output:&mut[u8]) -> usize {
        0
    }
    fn wrote_eof(&self) -> bool {
        true
    }
    fn free_mux(&mut self, _m8: &mut AllocU8) {
    }
}
