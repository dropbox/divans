use alloc::Allocator;

use util;

pub const MAX_NUM_STREAM: usize = 16;
pub const STREAM_ID_MASK: StreamID = 0xf;
pub type StreamID = u8;

pub struct ReadableBytes<'a> {
    pub data: &'a [u8],
    pub read_offset: &'a mut usize,
}

impl<'a> ReadableBytes<'a> {
    #[inline(always)]
    pub fn bytes_avail(&self) -> usize {
        self.data.len() - *self.read_offset
    }
}

pub struct WritableBytes<'a> {
    pub data: &'a mut [u8],
    pub write_offset: &'a mut usize,
}

pub trait StreamMuxer<AllocU8: Allocator<u8>> {
    /// Writes `data` to the specified stream.
    #[inline(always)]
    fn write(&mut self, stream_id: StreamID, data: &[u8], alloc_u8: &mut AllocU8) -> usize;
    /// Returns an array of `n_stream` `WritableByte`s. Each WritableByte is connected to
    /// the buffer of its corresponding stream.
    #[inline(always)]
    fn write_buffer(&mut self, stream_id: StreamID, alloc_u8: &mut AllocU8) -> WritableBytes;
    #[inline(always)]
    fn can_serialize() -> bool {
        true
    }
    /// Populate `output` with content buffered by each stream in a "fair" manner.
    #[inline(always)]
    fn serialize(&mut self, output: &mut [u8]) -> usize;
    #[inline(always)]
    fn flush(&mut self, output: &mut [u8]) -> usize;
    #[inline(always)]
    fn wrote_eof(&self) -> bool;
    #[inline(always)]
    fn free(&mut self, alloc_u8: &mut AllocU8);
    #[inline(always)]
    fn n_stream(&self) -> usize;
}

pub trait StreamDemuxer<AllocU8: Allocator<u8>> {
    /// Demultiplexes `data` into streams.
    #[inline(always)]
    fn deserialize(&mut self, data: &[u8], alloc_u8: &mut AllocU8) -> usize;
    /// Returns an array of `n_stream` `ReadableByte`s. Each ReadableByte is connected to
    /// the buffer of its corresponding stream.
    #[inline(always)]
    fn read_buffer(&mut self, stream_id: StreamID) -> ReadableBytes;
    #[inline(always)]
    fn data_len(&self, stream_id: StreamID) -> usize;
    #[inline(always)]
    fn data(&self, stream_id: StreamID) -> &[u8];
    #[inline(always)]
    fn editable_data(
        &mut self,
        stream_id: StreamID,
    ) -> &mut util::AllocatedMemoryRange<u8, AllocU8>;
    #[inline(always)]
    fn consume_data(&mut self, stream_id: StreamID, count: usize);
    #[inline(always)]
    fn consumed_all_streams_until_eof(&self) -> bool;
    #[inline(always)]
    fn encountered_eof(&self) -> bool;
    #[inline(always)]
    fn free(&mut self, alloc_u8: &mut AllocU8);
    #[inline(always)]
    fn n_stream(&self) -> usize;
}
