use core;
#[allow(unused_imports)]
use interface::{DivansCompressorFactory, BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType, StreamDemuxer, ReadableBytes, StreamID, NUM_STREAMS};

use slice_util::{AllocatedMemoryRange, AllocatedMemoryPrefix};
use alloc::{SliceWrapper, Allocator};

pub enum ThreadData<AllocU8:Allocator<u8>> {
    Data(AllocatedMemoryRange<u8, AllocU8>),
    Eof,
}
pub enum CommandResult<AllocU8: Allocator<u8>, SliceType:SliceWrapper<u8>> {
    Cmd(Command<SliceType>),
    Eof,
    ProcessedData(AllocatedMemoryRange<u8, AllocU8>),
}
pub trait MainToThread<AllocU8:Allocator<u8>> {
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()>;
    fn push(&mut self, data: AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()>;
    fn pull(&mut self) -> CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>;
}

pub trait ThreadToMain<AllocU8:Allocator<u8>> {
    fn pull_data(&mut self) -> ThreadData<AllocU8>;
    fn pull_context_map(&mut self) -> PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>;
    fn push_command(&mut self, CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>);
}

pub struct SerialWorker<AllocU8:Allocator<u8>> {
    data_len: usize,
    data: [ThreadData<AllocU8>;2],
    cm_len: usize,
    cm: [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; 2],
    result_len: usize,
    result:[CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>;3],
}

impl<AllocU8:Allocator<u8>> MainToThread<AllocU8> for SerialWorker<AllocU8> {
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()> {
        if self.cm_len == self.cm.len() {
            return Err(());
        }
        self.cm[self.cm_len] = cm;
        self.cm_len += 1;
        Ok(())
    }
    fn push(&mut self, data: AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()> {
        if self.data_len == self.data.len() {
            return Err(());
        }
        self.data[self.data_len] = ThreadData::Data(data);
        self.data_len += 1;
        Ok(())        
    }
    fn pull(&mut self) -> CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>{
        assert!(self.result_len != 0);
        let ret = core::mem::replace(&mut self.result[self.result_len - 1], CommandResult::Eof);
        self.result_len -= 1;
        ret
    }
}
type NopUsize = usize;
pub struct ThreadToMainDemuxer<AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>>{
    worker: WorkerInterface,
    slice: AllocatedMemoryRange<u8, AllocU8>,
    unused: NopUsize,
    eof: bool,
}
impl <AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>> ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    pub fn new(w:WorkerInterface) -> Self {
        Self{
            worker:w,
            slice: AllocatedMemoryRange::<u8, AllocU8>::default(),
            unused: NopUsize::default(),
            eof: false,
        }
    }
    fn pull_if_necessary(&mut self) {
        if self.slice.slice().len() == 0 {
            match self.worker.pull_data() {
                ThreadData::Eof => self.eof = true,
                ThreadData::Data(array) => self.slice = array,
            }
        }
    }
}

impl<AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>> StreamDemuxer<AllocU8> for ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    fn write_linear(&mut self, _data:&[u8], _m8: &mut AllocU8) -> usize {
        unimplemented!();
    }
    #[inline(always)]
    fn read_buffer(&mut self) -> [ReadableBytes; NUM_STREAMS] {
        self.pull_if_necessary();
        let data = self.slice.0.slice().split_at(self.slice.1.end).0;
        [ReadableBytes{data:data, read_offset:&mut self.slice.1.start},
         ReadableBytes{data:&[], read_offset:&mut self.unused},
         ]
    }
    #[inline(always)]
    fn data_ready(&self, stream_id:StreamID) -> usize {
        assert_eq!(stream_id, 0);
        self.slice.slice().len()
    }
    #[inline(always)]
    fn peek(&self, stream_id: StreamID) -> &[u8] {
        assert_eq!(stream_id, 0);
        self.slice.slice()
    }
    #[inline(always)]
    fn pop(&mut self, stream_id: StreamID) -> AllocatedMemoryRange<u8, AllocU8> {
        assert_eq!(stream_id, 0);
        self.pull_if_necessary();
        core::mem::replace(&mut self.slice, AllocatedMemoryRange::<u8, AllocU8>::default())
    }
    #[inline(always)]
    fn consume(&mut self, stream_id: StreamID, count: usize) {
        assert_eq!(stream_id, 0);
        self.slice.1.start += count;
        if self.slice.slice().len() == 0 && self.slice.0.slice().len() != 0 {
            self.worker.push_command(CommandResult::ProcessedData(
                core::mem::replace(&mut self.slice, AllocatedMemoryRange::<u8, AllocU8>::default())));
        }
    }
    #[inline(always)]
    fn encountered_eof(&self) -> bool {
        self.eof && self.slice.slice().len() == 0
    }
    #[inline(always)]
    fn free_demux(&mut self, _m8: &mut AllocU8){
        if self.slice.0.slice().len() != 0 {
            self.worker.push_command(CommandResult::ProcessedData(
                core::mem::replace(&mut self.slice, AllocatedMemoryRange::<u8, AllocU8>::default())));
        }
    }


}
impl<AllocU8:Allocator<u8>> ThreadToMain<AllocU8> for SerialWorker<AllocU8> {
    fn pull_data(&mut self) -> ThreadData<AllocU8> {
        assert!(self.data_len != 0);
        let ret = core::mem::replace(&mut self.data[self.data_len - 1], ThreadData::Eof);
        self.data_len -= 1;
        ret
    }
    fn pull_context_map(&mut self) -> PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>> {
        assert!(self.cm_len != 0);
        let ret = core::mem::replace(&mut self.cm[self.cm_len - 1], PredictionModeContextMap::<AllocatedMemoryPrefix<u8, AllocU8>> {
            literal_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
            predmode_speed_and_distance_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
        });
        self.cm_len -= 1;
        ret
    }
    fn push_command(&mut self, cmd:CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>) {
        assert!(self.result_len != self.result.len());
        self.result[self.result_len] = cmd;
        self.result_len += 1;
    }
}
