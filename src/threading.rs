use core;
#[allow(unused_imports)]
use interface::{DivansCompressorFactory, BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType};
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
