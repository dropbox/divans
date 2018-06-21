use alloc::{Allocator, SliceWrapper, SliceWrapperMut};
use core;

use interface::{ReadableBytes, StreamDemuxer, StreamID, StreamMuxer, WritableBytes,
                MAX_NUM_STREAM, STREAM_ID_MASK};
use util::AllocatedMemoryRange;

pub const EOF_MARKER: [u8; 3] = [0xff, 0xfe, 0xff];
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

#[derive(Debug)]
enum MuxSliceHeader {
    Var([u8; MAX_HEADER_SIZE]),
    Fixed([u8; 1]),
}

fn chunk_size(last_flushed: usize, lagging_stream: bool) -> usize {
    if lagging_stream {
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

fn get_mux_header(
    stream_id: StreamID,
    n_bytes_to_write: usize,
    is_lagging: bool,
) -> (MuxSliceHeader, usize) {
    //eprintln!("want to: {},{},", stream_id, bytes_to_write);
    if is_lagging == false || n_bytes_to_write == 4096 || n_bytes_to_write == 16384
        || n_bytes_to_write >= 65536
    {
        if n_bytes_to_write < 4096 {
            return get_mux_header(stream_id, n_bytes_to_write, true);
        }
        if n_bytes_to_write < 16384 {
            //eprintln!("({},{})", stream_id, 4096);
            return (MuxSliceHeader::Fixed([stream_id as u8 | (1 << 4)]), 4096);
        }
        if n_bytes_to_write < 65536 {
            //eprintln!("({},{})", stream_id, 16384);
            return (MuxSliceHeader::Fixed([stream_id as u8 | (2 << 4)]), 16384);
        }
        //eprintln!("({},{})", stream_id, 65536);
        return (MuxSliceHeader::Fixed([stream_id as u8 | (3 << 4)]), 65536);
    }
    assert!(n_bytes_to_write < 65536);
    //eprintln!("({},{})", stream_id, bytes_to_write);
    let ret = [
        stream_id,
        (n_bytes_to_write - 1) as u8,
        ((n_bytes_to_write - 1) >> 8) as u8,
    ];
    return (MuxSliceHeader::Var(ret), n_bytes_to_write);
}

pub struct Mux<AllocU8: Allocator<u8>> {
    buf: Vec<AllocatedMemoryRange<u8, AllocU8>>,
    cur_stream: StreamID,
    cur_stream_bytes_avail: usize,
    bytes_flushed: usize,
    // The total number of bytes that have been flushed out by the mux
    // when the stream finished flushing last time
    stream_state: StreamState,
    last_flush: Vec<usize>,
    bytes_to_deserialize: BytesToDeserialize,
}

impl<AllocU8: Allocator<u8>> Default for Mux<AllocU8> {
    fn default() -> Self {
        Self::new(2)
    }
}

impl<AllocU8: Allocator<u8>> StreamMuxer<AllocU8> for Mux<AllocU8> {
    fn write(&mut self, stream_id: StreamID, data: &[u8], alloc_u8: &mut AllocU8) -> usize {
        self.push_data(stream_id, data, alloc_u8);
        data.len()
    }

    fn write_buffer(&mut self, stream_id: StreamID, alloc_u8: &mut AllocU8) -> WritableBytes {
        const MIN_BYTES: usize = 16;
        self.prep_push_for_n_bytes(stream_id, MIN_BYTES, alloc_u8);
        let buf = &mut self.buf[stream_id as usize];
        WritableBytes {
            data: buf.mem.slice_mut(),
            write_offset: &mut buf.range.end,
        }
    }

    fn serialize(&mut self, output: &mut [u8]) -> usize {
        let mut output_offset = 0usize;
        if self.cur_stream_bytes_avail != 0 {
            output_offset += self.serialize_leftover(output);
        }
        while output_offset < output.len() {
            let mut flushed_any = false;
            let mut min_flush = self.last_flush[0];
            let mut max_flush = self.last_flush[0];
            for lf in self.last_flush[1..].iter() {
                if *lf < min_flush {
                    min_flush = *lf;
                }
                if *lf > max_flush {
                    max_flush = *lf;
                }
            }
            for index in 0..self.n_stream() {
                let mut is_lagging = self.last_flush[index] + MAX_FLUSH_VARIANCE < max_flush;
                if self.write_cursor(index) - self.read_cursor(index)
                    >= chunk_size(self.last_flush[index], is_lagging)
                    && self.last_flush[index] <= min_flush + MAX_FLUSH_VARIANCE
                {
                    flushed_any = true;
                    self.serialize_stream_id(
                        index as StreamID,
                        output,
                        &mut output_offset,
                        is_lagging,
                    );
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

    fn flush(&mut self, output: &mut [u8]) -> usize {
        match self.stream_state {
            StreamState::EofDone => return 0,
            _ => {}
        }
        let mut ret = self.flush_internal(output);
        if ret == output.len() {
            return ret;
        }
        match self.stream_state {
            StreamState::Running => {
                output[ret] = EOF_MARKER[0];
                ret += 1;
                self.stream_state = StreamState::EofStart;
            }
            _ => {}
        }
        if ret == output.len() {
            return ret;
        }
        match self.stream_state {
            StreamState::EofStart => {
                output[ret] = EOF_MARKER[1];
                ret += 1;
                self.stream_state = StreamState::EofMid;
            }
            _ => {}
        }
        if ret == output.len() {
            return ret;
        }
        match self.stream_state {
            StreamState::EofMid => {
                output[ret] = EOF_MARKER[2];
                ret += 1;
                self.stream_state = StreamState::EofDone;
            }
            _ => {}
        }
        return ret;
    }

    fn wrote_eof(&self) -> bool {
        self.is_eof()
    }

    fn free(&mut self, alloc_u8: &mut AllocU8) {
        self.free(alloc_u8);
    }

    fn n_stream(&self) -> usize {
        self.n_stream()
    }
}

impl<AllocU8: Allocator<u8>> StreamDemuxer<AllocU8> for Mux<AllocU8> {
    fn deserialize(&mut self, data: &[u8], alloc_u8: &mut AllocU8) -> usize {
        let mut input = data;
        let mut ret = 0usize;
        while input.len() != 0 && match self.stream_state {
            StreamState::EofDone => false,
            _ => true,
        } {
            match self.bytes_to_deserialize {
                BytesToDeserialize::Header0(stream_id) => {
                    self.bytes_to_deserialize = BytesToDeserialize::Header1(stream_id, input[0]);
                    return ret + 1 + self.deserialize(input.split_at(1).1, alloc_u8);
                }
                BytesToDeserialize::Header1(stream_id, lsb) => {
                    self.bytes_to_deserialize = BytesToDeserialize::Some(
                        stream_id,
                        (lsb as u32 | (input[0] as u32) << 8) + 1,
                    );
                    //eprint!("{}) Deserializing {}\n", stream_id, (lsb as u32 | (input[0] as u32) << 8) + 1);
                    //eprintln!("({},{}),", stream_id, (lsb as u32 | (input[0] as u32) << 8) + 1);
                    return ret + 1 + self.deserialize(input.split_at(1).1, alloc_u8);
                }
                BytesToDeserialize::Some(stream_id, count) => {
                    if count as usize > input.len() {
                        self.push_data(stream_id, input, alloc_u8);
                        self.bytes_to_deserialize =
                            BytesToDeserialize::Some(stream_id, count - input.len() as u32);
                        return ret + input.len();
                    }
                    let (to_push, remainder) = input.split_at(count as usize);
                    self.push_data(stream_id, to_push, alloc_u8);
                    input = remainder;
                    self.bytes_to_deserialize = BytesToDeserialize::None;
                    ret += to_push.len();
                }
                BytesToDeserialize::None => {
                    if input[0] == EOF_MARKER[0] || input[0] == EOF_MARKER[1]
                        || input[0] == EOF_MARKER[2]
                    {
                        if input[0] == EOF_MARKER[0] || match self.stream_state {
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
                        // Fixed header
                        if input.len() < 3 {
                            self.bytes_to_deserialize = BytesToDeserialize::Header0(stream_id);
                            return ret + 1 + self.deserialize(input.split_at(1).1, alloc_u8);
                        }
                        count = 3;
                        bytes_to_copy = (input[1] as u32 | (input[2] as u32) << 8) + 1;
                    //eprintln!("({},{}),", stream_id, bytes_to_copy);
                    } else {
                        // Var header
                        count = 1;
                        bytes_to_copy = 1024 << ((input[0] >> 4) << 1);
                        //eprintln!("({},{}),", stream_id, bytes_to_copy);
                    }
                    //eprint!("{}) Deserializing {}\n", stream_id, bytes_to_copy);
                    self.bytes_to_deserialize = BytesToDeserialize::Some(stream_id, bytes_to_copy);
                    input = input.split_at(count).1;
                    ret += count;
                }
            }
        }
        ret
    }

    fn read_buffer(&mut self, stream_id: StreamID) -> ReadableBytes {
        let buf = &mut self.buf[stream_id as usize];
        ReadableBytes {
            data: buf.mem.slice().split_at(buf.range.end).0,
            read_offset: &mut buf.range.start,
        }
    }

    fn data_len(&self, stream_id: StreamID) -> usize {
        self.write_cursor(usize::from(stream_id)) - self.read_cursor(usize::from(stream_id))
    }

    fn data(&self, stream_id: StreamID) -> &[u8] {
        &self.buf[usize::from(stream_id)].slice()
    }

    fn editable_data(&mut self, stream_id: StreamID) -> &mut AllocatedMemoryRange<u8, AllocU8> {
        &mut self.buf[usize::from(stream_id)]
    }

    fn consume_data(&mut self, stream_id: StreamID, count: usize) {
        self.buf[usize::from(stream_id)].range.start += count;
    }

    fn consumed_all_streams_until_eof(&self) -> bool {
        self.is_eof()
    }

    fn encountered_eof(&self) -> bool {
        match self.stream_state {
            StreamState::EofDone => true,
            _ => false,
        }
    }

    fn free(&mut self, alloc_u8: &mut AllocU8) {
        self.free(alloc_u8)
    }

    fn n_stream(&self) -> usize {
        self.n_stream()
    }
}

impl<AllocU8: Allocator<u8>> Mux<AllocU8> {
    pub fn new(n_stream: usize) -> Self {
        assert!(n_stream <= MAX_NUM_STREAM);
        let mut buf = Vec::with_capacity(n_stream);
        for _ in 0..n_stream {
            buf.push(AllocatedMemoryRange::default());
        }
        Mux::<AllocU8> {
            buf,
            cur_stream: 0,
            cur_stream_bytes_avail: 0,
            bytes_flushed: 0,
            last_flush: vec![0; n_stream],
            stream_state: StreamState::Running,
            bytes_to_deserialize: BytesToDeserialize::None,
        }
    }

    pub fn n_stream(&self) -> usize {
        self.buf.len()
    }

    pub fn read_cursor(&self, index: usize) -> usize {
        self.buf[index].range.start
    }

    pub fn write_cursor(&self, index: usize) -> usize {
        self.buf[index].range.end
    }

    pub fn is_eof(&self) -> bool {
        for index in 0..self.n_stream() {
            if self.read_cursor(index) != self.write_cursor(index) {
                return false;
            }
        }
        match self.stream_state {
            StreamState::EofDone => true,
            _ => false,
        }
    }

    pub fn prealloc(&mut self, alloc_u8: &mut AllocU8, amount_per_stream: usize) {
        for buf in self.buf.iter_mut() {
            assert_eq!(buf.mem.slice().len(), 0);
            let mfd = core::mem::replace(&mut buf.mem, alloc_u8.alloc_cell(amount_per_stream));
            alloc_u8.free_cell(mfd);
        }
    }

    pub fn free(&mut self, alloc_u8: &mut AllocU8) {
        for buf in self.buf.iter_mut() {
            alloc_u8.free_cell(core::mem::replace(
                &mut buf.mem,
                AllocU8::AllocatedMemory::default(),
            ));
        }
    }

    /// Pushes data from a source into the stream buffer specified by `stream_id`.
    /// This data may later be serialized through `serialize` or else consumed
    /// through `data` or `consume`.
    pub fn push_data(&mut self, stream_id: StreamID, data: &[u8], alloc_u8: &mut AllocU8) {
        let (buf, offset) = self.prep_push_for_n_bytes(stream_id, data.len(), alloc_u8);
        Self::unchecked_push(buf.slice_mut(), offset, data)
    }

    fn unchecked_push(buf: &mut [u8], write_cursor: &mut usize, data: &[u8]) {
        buf.split_at_mut(*write_cursor)
            .1
            .split_at_mut(data.len())
            .0
            .clone_from_slice(data);
        *write_cursor += data.len();
    }

    fn prep_push_for_n_bytes(
        &mut self,
        stream_id: StreamID,
        data_len: usize,
        alloc_u8: &mut AllocU8,
    ) -> (&mut AllocU8::AllocatedMemory, &mut usize) {
        //let mut write_cursor = &mut self.write_cursor[stream_id as usize];
        let buf_entry = &mut self.buf[usize::from(stream_id)];
        let write_cursor = &mut buf_entry.range.end;
        let read_cursor = &mut buf_entry.range.start;
        let buf = &mut buf_entry.mem;
        // if there's space in the buffer, simply return it
        if buf.slice().len() - *write_cursor >= data_len {
            return (buf, write_cursor);
        }
        // if there's too much room at the beginning and the new data fits, then move everything to the beginning
        if buf.slice().len() >= (*write_cursor - *read_cursor) + data_len + MAX_HEADER_SIZE
            && (*read_cursor == *write_cursor
                || (*read_cursor >= 16384
                    && *read_cursor > *write_cursor - *read_cursor + MAX_HEADER_SIZE))
        {
            {
                let (unbuffered_empty_half, full_half) = buf.slice_mut().split_at_mut(*read_cursor);
                let empty_half = unbuffered_empty_half.split_at_mut(MAX_HEADER_SIZE).1; // leave some room on the beginning side for header data to be flushed
                let amount_of_data_to_copy = *write_cursor - *read_cursor;
                empty_half
                    .split_at_mut(amount_of_data_to_copy)
                    .0
                    .clone_from_slice(full_half.split_at(amount_of_data_to_copy).0);
                *write_cursor = MAX_HEADER_SIZE + amount_of_data_to_copy;
                *read_cursor = MAX_HEADER_SIZE;
            }
            return (buf, write_cursor);
        }
        // find the next power of two buffer size that could hold everything including the recently added data
        let desired_size: u64 =
            (MAX_HEADER_SIZE + data_len + (*write_cursor - *read_cursor)) as u64;
        let log_desired_size = (64 - desired_size.leading_zeros()) + 1;
        // allocate space for new data and copy in the current data
        let mut new_buf = alloc_u8.alloc_cell(1 << core::cmp::max(log_desired_size, 9));
        debug_assert!(new_buf.slice().len() >= *write_cursor - *read_cursor + data_len);
        new_buf
            .slice_mut()
            .split_at_mut(MAX_HEADER_SIZE)
            .1
            .split_at_mut(*write_cursor - *read_cursor)
            .0
            .clone_from_slice(
                buf.slice()
                    .split_at(*read_cursor)
                    .1
                    .split_at(*write_cursor - *read_cursor)
                    .0,
            );
        *write_cursor = MAX_HEADER_SIZE + *write_cursor - *read_cursor;
        *read_cursor = MAX_HEADER_SIZE;
        alloc_u8.free_cell(core::mem::replace(buf, new_buf));
        (buf, write_cursor)
    }

    /// copy the remaining data from a previous serialize
    fn serialize_leftover(&mut self, output: &mut [u8]) -> usize {
        let to_copy = core::cmp::min(self.cur_stream_bytes_avail, output.len());
        output.split_at_mut(to_copy).0.clone_from_slice(
            self.buf[usize::from(self.cur_stream)]
                .mem
                .slice()
                .split_at(self.read_cursor(usize::from(self.cur_stream)))
                .1
                .split_at(to_copy)
                .0,
        );
        self.buf[usize::from(self.cur_stream)].range.start += to_copy;
        self.cur_stream_bytes_avail -= to_copy;
        to_copy
    }

    fn serialize_stream_id(
        &mut self,
        stream_id: StreamID,
        output: &mut [u8],
        output_offset: &mut usize,
        is_lagging: bool,
    ) {
        let buf_entry = &mut self.buf[usize::from(stream_id)];
        let write_cursor = &mut buf_entry.range.end;
        let read_cursor = &mut buf_entry.range.start;
        let buf = &mut buf_entry.mem.slice_mut();

        // find the header and number of bytes that should be written to it
        let (header, mut n_bytes_to_write) =
            get_mux_header(stream_id, *write_cursor - *read_cursor, is_lagging);
        //eprint!("{}) header {:?} bytes: {}\n", stream_id, header, num_bytes_should_write);
        self.bytes_flushed += n_bytes_to_write;
        assert!(*read_cursor >= MAX_HEADER_SIZE);
        match header {
            MuxSliceHeader::Var(hdr) => {
                // add on the number of bytes that should be written
                n_bytes_to_write += hdr.len();
                // subtract the location of the buffer...this should not bring us below zero
                *read_cursor -= hdr.len();
                for i in 0..hdr.len() {
                    buf[*read_cursor + i] = hdr[i];
                }
            }
            MuxSliceHeader::Fixed(hdr) => {
                n_bytes_to_write += hdr.len();
                *read_cursor -= hdr.len();
                for i in 0..hdr.len() {
                    buf[*read_cursor + i] = hdr[i];
                }
            }
        }
        // set bytes_flushed to the end of the desired bytes to flush, so we know this stream isn't lagging too badly
        self.last_flush[usize::from(stream_id)] = self.bytes_flushed;
        // compute the number of bytes that will fit into otput
        let to_write = core::cmp::min(n_bytes_to_write, output.len() - *output_offset);
        output
            .split_at_mut(*output_offset)
            .1
            .split_at_mut(to_write)
            .0
            .clone_from_slice(buf.split_at(*read_cursor).1.split_at(to_write).0);
        *read_cursor += to_write;
        // if we have produced everything from this stream, reset the cursors to the beginning to support quick copies
        if *read_cursor == *write_cursor {
            *read_cursor = MAX_HEADER_SIZE;
            *write_cursor = *read_cursor; // reset cursors to the beginning of the buffer
        }
        *output_offset += to_write;
        // we have some leftovers that would not fit into the output buffer..store these for the next serialize_leftovers call
        if to_write != n_bytes_to_write {
            self.cur_stream_bytes_avail = n_bytes_to_write - to_write;
            self.cur_stream = stream_id as StreamID;
        }
    }

    fn deserialize_eof(&mut self, mut input: &[u8]) -> usize {
        let mut ret = 0usize;
        assert_eq!(EOF_MARKER.len(), 3);
        match self.stream_state {
            StreamState::Running => {
                if input[0] == EOF_MARKER[0] {
                    ret += 1;
                    input = input.split_at(1).1;
                    self.stream_state = StreamState::EofStart;
                }
            }
            _ => {}
        }
        if input.len() == 0 {
            return ret;
        }
        match self.stream_state {
            StreamState::EofStart => {
                if input[0] == EOF_MARKER[1] {
                    ret += 1;
                    input = input.split_at(1).1;
                    self.stream_state = StreamState::EofMid
                }
            }
            _ => {}
        }
        if input.len() == 0 {
            return ret;
        }
        match self.stream_state {
            StreamState::EofMid => {
                if input[0] == EOF_MARKER[2] {
                    ret += 1;
                    self.stream_state = StreamState::EofDone;
                    return ret;
                }
            }
            _ => {}
        }
        return ret;
    }

    fn flush_internal(&mut self, output: &mut [u8]) -> usize {
        let mut output_offset = 0usize;
        if self.cur_stream_bytes_avail != 0 {
            output_offset += self.serialize_leftover(output);
        }
        while output_offset < output.len() {
            let mut flushed_any = false;
            let mut last_flush: Option<usize> = None;
            for (lf, buf) in self.last_flush.iter().zip(self.buf.iter()) {
                let rc = buf.range.start;
                let wc = buf.range.end;
                if match last_flush {
                    None => rc != wc, // only consider this item for being the last flush point if it has data to flush
                    Some(last_flush_some) => *lf < last_flush_some && rc != wc,
                } {
                    last_flush = Some(*lf);
                }
            }
            for index in 0..self.n_stream() {
                if match last_flush {
                    None => true,
                    Some(last_flush_some) => {
                        self.last_flush[index] <= last_flush_some + MAX_FLUSH_VARIANCE
                    }
                } {
                    let mut written = output_offset;
                    if self.read_cursor(index) != self.write_cursor(index) {
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

pub struct DevNull<AllocU8: Allocator<u8>> {
    cursor: Vec<usize>,
    empty: AllocatedMemoryRange<u8, AllocU8>,
    _placeholder: core::marker::PhantomData<AllocU8>,
}

impl<AllocU8: Allocator<u8>> DevNull<AllocU8> {
    pub fn new(n_stream: usize) -> Self {
        assert!(n_stream <= MAX_NUM_STREAM);
        DevNull::<AllocU8> {
            cursor: vec![0; n_stream],
            empty: AllocatedMemoryRange::default(),
            _placeholder: core::marker::PhantomData::<AllocU8>::default(),
        }
    }

    pub fn n_stream(&self) -> usize {
        self.cursor.len()
    }
}

impl<AllocU8: Allocator<u8>> Default for DevNull<AllocU8> {
    fn default() -> Self {
        Self::new(2)
    }
}

impl<AllocU8: Allocator<u8>> StreamMuxer<AllocU8> for DevNull<AllocU8> {
    fn write_buffer(&mut self, stream_id: StreamID, _alloc_u8: &mut AllocU8) -> WritableBytes {
        WritableBytes {
            data: &mut [],
            write_offset: &mut self.cursor[stream_id as usize],
        }
    }

    fn write(&mut self, _stream_id: StreamID, data: &[u8], _alloc_u8: &mut AllocU8) -> usize {
        debug_assert_eq!(data.len(), 0);
        0
    }

    fn can_serialize() -> bool {
        false
    }

    fn serialize(&mut self, _output: &mut [u8]) -> usize {
        0
    }

    fn flush(&mut self, _output: &mut [u8]) -> usize {
        0
    }

    fn wrote_eof(&self) -> bool {
        true
    }

    fn free(&mut self, _alloc_u8: &mut AllocU8) {}

    fn n_stream(&self) -> usize {
        self.n_stream()
    }
}

impl<AllocU8: Allocator<u8>> StreamDemuxer<AllocU8> for DevNull<AllocU8> {
    fn deserialize(&mut self, data: &[u8], _alloc_u8: &mut AllocU8) -> usize {
        debug_assert_eq!(data.len(), 0);
        0
    }

    fn read_buffer(&mut self, stream_id: StreamID) -> ReadableBytes {
        ReadableBytes {
            data: &[],
            read_offset: &mut self.cursor[stream_id as usize],
        }
    }

    fn data_len(&self, _stream_id: StreamID) -> usize {
        0
    }

    fn data(&self, _stream_id: StreamID) -> &[u8] {
        &[]
    }

    fn editable_data(&mut self, _stream_id: StreamID) -> &mut AllocatedMemoryRange<u8, AllocU8> {
        &mut self.empty
    }

    fn consume_data(&mut self, _stream_id: StreamID, count: usize) {
        debug_assert_eq!(count, 0);
    }

    fn consumed_all_streams_until_eof(&self) -> bool {
        true
    }

    fn encountered_eof(&self) -> bool {
        true
    }

    fn free(&mut self, _alloc_u8: &mut AllocU8) {}

    fn n_stream(&self) -> usize {
        self.n_stream()
    }
}
