type StreamID = u8;
const NUM_STREAMS: StreamID = 2;
const STREAM_ID_MASK: StreamID = 0x3;
const MAX_FLUSH_VARIANCE: usize = 131073;
const CHUNK_SIZE: usize = 65536;
struct Mux<AllocU8:Allocator<u8> > {
   buf: [AllocU8::AllocatedMemory; NUM_STREAMS as usize],
   read_cursor: [usize; NUM_STREAMS as usize],
   write_cursor: [usize; NUM_STREAMS as usize],
   cur_stream_bytes_avail: u32,
   cur_stream:StreamID;
   last_flush:[usize; NUM_STREAMS as usize],
   bytes_flushed: usize,
}

fn chunk_size(last_flushed:usize) {
  if last_flushed <= 1024 {
     return 256;
  }
  if last_flushed <= 65536 {
     return 16384;
  }
  return 65536;
}

impl<AllocU8:Allocator<u8> > Default for Mux<AllocU8> {
    fn default() -> Self {
        Mux::<AllocU8> {
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

impl<AllocU8:Allocator<u8>> Mux<AllocU8> {
   #[inline(always)]
   fn data_avail(&self, stream_id: StreamID) -> &[u8] {
      self.buf[stream_id].split_at(self.read_cursor[stream_id]).1.split_at(self.write_cursor[stream_id]).0
   }
   fn consume(&self, count: usize) {
      read_cursor += count;
   }
   fn unchecked_push(buf: &mut[u8], read_cursor: &mut usize, write_cursor: &mut usize, data: &mut[u8]) {
       buf.split_at_mut(*write_cursor).1.split_at_mut(data.len()).0.clone_from_slice(data);
       *write_cursor += data.len();
   }
   fn push_data(&mut self, stream_id: StreamID, data: &[u8], m8: &mut AllocU8) {
      let mut read_cursor = &mut self.read_cursor[stream_id as usize];
      let mut write_cursor = &mut self.write_cursor[stream_id as usize];
      let mut buf = &mut self.buf[stream_id as usize];
      if buf.len() - *write_cursor >= data.len() {
          Self::unchecked_push(buf, read_cursor, write_cursor, data);
          return;
      }
      if buf.len() + (*write_cursor - *read_cursor) >= data.len() && (*read_cursor == *write_cursor
                                                                    || (*read_cursor >= 16384 && *write_cursor - *read_cursor > *read_cursor)) {
          let (empty_half, full_half) = buf.split_at_mut(read_cursor);
          *write_cursor = *write_cursor - *read_cursor;
          *read_cursor = 0;
          empty_half.split_at_mut(*write_cursor).0.clone_from_slice(full_half.split_at(*write_cursor));
          Self::unchecked_push(buf, read_cursor, write_cursor, data);
          return;
      }
      let desired_size:u64 = (data.len() + (*write_cursor - *read_cursor)) as u64;
      let log_desired_size = (64 - desired_size.leading_zeros()) + 1;
      let new_buf = m8.alloc_cell(1 << log_desired_size);
      debug_assert(new_buf.len() >= *write_cursor - *read_cursor + data.len());
      new_buf.split_at_mut(*write_cursor - *read_cursor).0.clone_from_slice(buf.split_at(*read_cursor).1.split_at(*write_cursor - *read_cursor).0);
      *write_cursor = 3 + *write_cursor - *read_cursor;
      *read_cursor = 3;
      m8.free_cell(core::mem::replace(buf, new_buf));
      Self::unchecked_push(buf, read_cursor, write_cursor, data);
   }
   fn serialize_leftover(&mut self, output:&mut[u8]) -> usize {
       let to_copy = core::cmp::min(self.cur_stream_bytes_avail, output.len());
       output.split_at_mut(to_copy).0.clone_from_slice(self.buf[self.cur_stream].split_at(self.read_cursor[self.cur_stream]).1.split_at(to_copy).0;
       self.read_cursor[self.cur_stream] += to_copy;
       self.cur_stream_bytes_avail -= to_copy;
       self.bytes_flushed += to_copy;
       self.last_flushed[self.cur_stream] = self.bytes_flushed;
       to_copy
   }
   fn serialize(&mut self, output:&mut [u8]) -> usize {
        let mut output_offset = 0usize;
        if self.cur_stream_bytes_avail != 0 {
           output_offset += self.serialize_leftover();
        }
        let output_len = output.len();
        while output_offset < output.len() {
           let mut flushed_any = false;
           let mut last_flush = self.last_flush[0];
           for lf in self.last_flush[1..] {
               if *lf < last_flush {
                  last_flush = *lf;
               }
           }
           for index in 0..(NUM_STREAMS as usize) {
              let mut read_cursor = &mut self.read_cursor[index];
              let mut write_cursor = &mut self.write_cursor[index];
              if *read_cursor - *write_cursor >= chunk_size(self.last_flush[index]) && self.last_flush[index] <= last_flush + MAX_FLUSH_VARIANCE {
                   let mut buf = &mut self.buf[index];
                   flushed_any = true;
                   //FIXME: flush what we have here
                   let should_write:usize;
                   (output[output_offset], should_write) = get_code(*write_cursor - *read_cursor);
                   offset += 1;
                   let to_write = core::cmp::min(should_write, output.len() - offset);
                   output.split_at_mut(offset).1.split_at_mut(to_write).0.clone_from_slice(self.buf.split_at(*read_cursor).1.split_at(to_write).0);
                   *read_cursor += to_write;
                   output_offset += to_write;
                   if to_write != should_write {
                       self.cur_stream_bytes_avail = this_written;
                       self.cur_stream = index;
                   }
               }
           }
           if !flushed_any {
             break;
           }
       }
       output_offset
   }
   fn flush(&mut self, output:&mut [u8]) {
       
   }
}