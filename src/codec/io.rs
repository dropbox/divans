use core;
use interface::{Command, PredictionModeContextMap, free_cmd, StreamDemuxer, ReadableBytes, StreamID, NUM_STREAMS};
use ::interface::{
    DivansOutputResult,
    MAX_PREDMODE_SPEED_AND_DISTANCE_CONTEXT_MAP_SIZE,
    MAX_LITERAL_CONTEXT_MAP_SIZE,
    EncoderOrDecoderRecoderSpecialization,
    ErrMsg,
};
use codec::interface::CMD_CODER;
use slice_util::{AllocatedMemoryRange, AllocatedMemoryPrefix};

use alloc::{Allocator};
use alloc_util::{RepurposingAlloc, UninitializedOnAlloc};
use cmd_to_raw::DivansRecodeState;

use threading::{ThreadToMain,ThreadData};


pub struct DemuxerAndRingBuffer<AllocU8:Allocator<u8>,
                                LinearInputBytes:StreamDemuxer<AllocU8>>{
    input: LinearInputBytes,
    phantom: core::marker::PhantomData<AllocU8>,
    err: DivansOutputResult,
}

impl<AllocU8:Allocator<u8>, LinearInputBytes:StreamDemuxer<AllocU8>+Default> Default for DemuxerAndRingBuffer<AllocU8, LinearInputBytes> {
  fn default() ->Self {
     DemuxerAndRingBuffer::<AllocU8, LinearInputBytes>::new(LinearInputBytes::default())
  }
}
impl<AllocU8:Allocator<u8>, LinearInputBytes:StreamDemuxer<AllocU8>> DemuxerAndRingBuffer<AllocU8, LinearInputBytes> {
    fn new(demuxer: LinearInputBytes) -> Self {
        DemuxerAndRingBuffer::<AllocU8, LinearInputBytes>{
            input:demuxer,
            phantom:core::marker::PhantomData::<AllocU8>::default(),
            err: DivansOutputResult::Success,
        }
    }
}

impl<AllocU8:Allocator<u8>, LinearInputBytes:StreamDemuxer<AllocU8>> StreamDemuxer<AllocU8> for DemuxerAndRingBuffer<AllocU8, LinearInputBytes> {
    #[inline(always)]
    fn write_linear(&mut self, data:&[u8], m8: &mut AllocU8) -> usize {
        self.input.write_linear(data, m8)
    }
    #[inline(always)]
    fn read_buffer(&mut self) -> [ReadableBytes; NUM_STREAMS] {
        self.input.read_buffer()
    }
    #[inline(always)]
    fn data_ready(&self, stream_id:StreamID) -> usize {
        self.input.data_ready(stream_id)
    }
    #[inline(always)]
    fn peek(&self, stream_id: StreamID) -> &[u8] {
        self.input.peek(stream_id)
    }
    #[inline(always)]
    fn edit(&mut self, stream_id: StreamID) -> &mut AllocatedMemoryRange<u8, AllocU8> {
        self.input.edit(stream_id)
    }
    #[inline(always)]
    fn consume(&mut self, stream_id: StreamID, count: usize) {
        self.input.consume(stream_id, count)
    }
    #[inline(always)]
    fn consumed_all_streams_until_eof(&self) -> bool {
        self.input.consumed_all_streams_until_eof()
    }
    #[inline(always)]
    fn encountered_eof(&self) -> bool {
        self.input.encountered_eof()
    }
    #[inline(always)]
    fn free_demux(&mut self, m8: &mut AllocU8) {
        self.input.free_demux(m8);
    }
}

// this is an implementation of simply printing to the ring buffer that masquerades as communicating with a 'main thread'
impl<AllocU8:Allocator<u8>, LinearInputBytes:StreamDemuxer<AllocU8>> ThreadToMain<AllocU8> for DemuxerAndRingBuffer<AllocU8, LinearInputBytes> {
    const COOPERATIVE:bool = false;
    const ISOLATED:bool = false;
    fn pull_data(&mut self) -> ThreadData<AllocU8> {
        ThreadData::Data(core::mem::replace(self.input.edit(CMD_CODER as StreamID), AllocatedMemoryRange::<u8, AllocU8>::default()))
    }
    fn pull_context_map(&mut self, mut m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> Result<PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>, ()> {
        match m8 {
            Some(ref mut m) => {
                let lit = m.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(MAX_LITERAL_CONTEXT_MAP_SIZE);
                Ok(PredictionModeContextMap::<AllocatedMemoryPrefix<u8, AllocU8>> {
                    literal_context_map:lit,
                    predmode_speed_and_distance_context_map:m.use_cached_allocation::<UninitializedOnAlloc>().alloc_cell(
                        MAX_PREDMODE_SPEED_AND_DISTANCE_CONTEXT_MAP_SIZE),
                })
            },
            None => {
                panic!("Pull context map in Demuxer+RingBuffer without an allocator");
            },
        }
    }
    fn push_eof(&mut self) -> DivansOutputResult {
        self.err
    }
    fn push_consumed_data(&mut self,
        data: &mut AllocatedMemoryRange<u8, AllocU8>,
        mut m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
    ) -> DivansOutputResult {
        m8.as_mut().unwrap().free_cell(core::mem::replace(&mut data.0, AllocU8::AllocatedMemory::default()));
        self.err
    }
    fn broadcast_err(&mut self, err:ErrMsg) {
        self.err = DivansOutputResult::Failure(err);
    }
    fn push_cmd<Specialization:EncoderOrDecoderRecoderSpecialization, CopyCallback:FnMut(&[u8])>(
        &mut self,
        cmd:&mut Command<AllocatedMemoryPrefix<u8, AllocU8>>,
        mut m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
        mut recoder: Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
        specialization:&mut Specialization,
        output:&mut [u8],
        output_offset: &mut usize,
        copy_cb:&mut CopyCallback,
    ) -> DivansOutputResult {
        let mut tmp_output_offset_bytes_backing: usize = 0;
        let tmp_output_offset_bytes = specialization.get_recoder_output_offset(
            output_offset,
            &mut tmp_output_offset_bytes_backing);
        let ret = recoder.as_mut().unwrap().encode_cmd(cmd,
                                                       specialization.get_recoder_output(output),
                                                       tmp_output_offset_bytes,
                                                       copy_cb);
        match ret {
            DivansOutputResult::Success => {
                free_cmd(cmd, &mut m8.as_mut().unwrap().use_cached_allocation::<
                        UninitializedOnAlloc>());
                self.err
            },
            DivansOutputResult::Failure(_) => {
                free_cmd(cmd, &mut m8.as_mut().unwrap().use_cached_allocation::<
                        UninitializedOnAlloc>());
                ret
            }
            _ => ret,
        }
    }
}
