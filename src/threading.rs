use core;
#[allow(unused_imports)]
use interface::{DivansCompressorFactory, BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType, StreamDemuxer, ReadableBytes, StreamID, NUM_STREAMS, EncoderOrDecoderRecoderSpecialization};
use ::interface::{DivansOutputResult, ErrMsg};
use slice_util::{AllocatedMemoryRange, AllocatedMemoryPrefix, SlicePlaceholder32};
use alloc::{SliceWrapper, SliceWrapperMut, Allocator};
use alloc_util::RepurposingAlloc;
use ::alloc_util::UninitializedOnAlloc;
use cmd_to_raw::DivansRecodeState;
pub enum ThreadData<AllocU8:Allocator<u8>> {
    Data(AllocatedMemoryRange<u8, AllocU8>),
    Yield,
    Eof,
}
pub type StaticCommand = Command<SlicePlaceholder32<u8>>;
pub const NUM_DATA_BUFFERED:usize = 2;

impl<AllocU8:Allocator<u8>> Default for ThreadData<AllocU8> {
    fn default() -> Self {
        ThreadData::Data(AllocatedMemoryRange::<u8, AllocU8>::default())
    }
}

pub fn empty_prediction_mode_context_map<ISl:SliceWrapper<u8>+Default>() -> PredictionModeContextMap<ISl> {
    PredictionModeContextMap::<ISl> {
        literal_context_map:ISl::default(),
        predmode_speed_and_distance_context_map:ISl::default(),
    }
}
#[derive(Clone,Copy,Debug)]
pub enum CommandResult {
    Ok,
    Eof,
    Err(ErrMsg)
}
pub trait PullAllocatedCommand<AllocU8:Allocator<u8>, AllocCommand: Allocator<StaticCommand>> {
    fn pull_command_buf(&mut self) -> (&mut AllocatedMemoryPrefix<StaticCommand, AllocCommand>, &mut [AllocatedMemoryRange<u8, AllocU8>;NUM_DATA_BUFFERED], &mut [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; NUM_DATA_BUFFERED], CommandResult);
}
pub trait MainToThread<AllocU8:Allocator<u8>> {
    const COOPERATIVE_MAIN: bool;
    type CommandOutputType: SliceWrapperMut<StaticCommand>+Default;
    #[inline(always)]
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()>;
    #[inline(always)]
    fn push(&mut self, data: &mut AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()>;
    #[inline(always)]
    fn pull(&mut self) -> (&mut Self::CommandOutputType, &mut [AllocatedMemoryRange<u8, AllocU8>;NUM_DATA_BUFFERED], &mut [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; NUM_DATA_BUFFERED], CommandResult);
}

pub trait ThreadToMain<AllocU8:Allocator<u8>> {
    const COOPERATIVE: bool;
    const ISOLATED: bool;
    #[inline(always)]
    fn pull_data(&mut self) -> ThreadData<AllocU8>;
    #[inline(always)]
    fn pull_context_map(&mut self, m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> Result<PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>, ()>;
    //fn alloc_literal(&mut self, len: usize, m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> LiteralCommand<AllocatedMemoryPrefix<u8, AllocU8>>;
    #[inline(always)]
    fn push_cmd<Specialization:EncoderOrDecoderRecoderSpecialization>(
        &mut self,
        cmd:&mut Command<AllocatedMemoryPrefix<u8, AllocU8>>,
        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
        recoder: Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
        specialization: &mut Specialization,
        output:&mut [u8],
        output_offset: &mut usize,
    ) -> DivansOutputResult;
    #[inline(always)]
    fn push_consumed_data(
        &mut self,
        data:&mut AllocatedMemoryRange<u8, AllocU8>,
        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
    ) -> DivansOutputResult;
    #[inline(always)]
    fn push_eof(
        &mut self,
    ) -> DivansOutputResult;
}
pub const NUM_SERIAL_COMMANDS_BUFFERED: usize = 256;
pub struct SerialWorker<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> {
    data_len: usize,
    data: [ThreadData<AllocU8>;NUM_DATA_BUFFERED],
    cm_len: usize,
    cm: [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; 2],
    result: AllocatedMemoryPrefix<StaticCommand, AllocCommand>,
    result_data: [AllocatedMemoryRange<u8, AllocU8>; NUM_DATA_BUFFERED],
    result_cm: [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; 2],
    pub waiters: u8,
    eof_present_in_result: CommandResult, // retriever should try to get everything
}
impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> SerialWorker<AllocU8, AllocCommand> {
    pub fn result_ready(&self) -> bool {
        self.result.1 != 0
    }
    pub fn result_space_ready(&self) -> bool {
        self.result.0.len() > self.result.1
    }
    pub fn result_multi_space_ready(&self, space_needed:usize) -> bool {
        self.result.0.len() - self.result.1 >= space_needed
    }
    pub fn cm_space_ready(&self) -> bool {
        self.cm_len != self.cm.len()
    }
    pub fn cm_ready(&self) -> bool {
        self.cm_len != 0
    }
    pub fn data_ready(&self) -> bool {
        self.data_len != 0
    }
    pub fn returned_data_space_ready(&self) -> bool {
        self.result_data[NUM_DATA_BUFFERED -1].0.len() == 0
    }
    pub fn set_eof_hint(&mut self) {
        if let CommandResult::Ok = self.eof_present_in_result {
            self.eof_present_in_result = CommandResult::Eof; // don't want to override errors here
        }
    }
    // returns the old space
    pub fn insert_results(&mut self,
                          cmds:&mut AllocatedMemoryPrefix<StaticCommand, AllocCommand>,
                          cm:Option<&mut PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>>) -> usize {
        let old_len = self.result.1;
        if self.result.1 == 0 {
            core::mem::swap(&mut self.result, cmds);
            self.result.1 = 0;
        } else {
            self.result.0.slice_mut().split_at_mut(old_len).1.split_at_mut(cmds.len()).0.clone_from_slice(cmds.slice());
        }
        cmds.1 = 0;
        if let Some(context_map) = cm {
            assert_eq!(self.result_cm[1].has_context_speeds(), false);
            if self.result_cm[0].has_context_speeds() {
                core::mem::swap(&mut self.result_cm[1], context_map);
            } else {
                core::mem::swap(&mut self.result_cm[0], context_map);
            }
        }
        old_len
    }
}
impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> SerialWorker<AllocU8, AllocCommand> {
    pub fn new(mc:&mut AllocCommand) -> Self {
        SerialWorker::<AllocU8, AllocCommand> {
            waiters: 0,
            eof_present_in_result: CommandResult::Ok,
            data_len: 0,
            data:[ThreadData::<AllocU8>::default(),
                  ThreadData::<AllocU8>::default()],
            cm_len: 0,
            cm: [empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>(),
                 empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>()],
            result:AllocatedMemoryPrefix::<StaticCommand, AllocCommand>::realloc(mc.alloc_cell(NUM_SERIAL_COMMANDS_BUFFERED),0),
            result_cm: [empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>(),
                        empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>()],
            result_data:[AllocatedMemoryRange::<u8, AllocU8>::default(),
                         AllocatedMemoryRange::<u8, AllocU8>::default()],
        }
    }
    pub fn free(&mut self, m8: &mut RepurposingAlloc<u8, AllocU8>, mc:&mut AllocCommand) {
        mc.free_cell(core::mem::replace(&mut self.result.0, AllocCommand::AllocatedMemory::default()));
        for item in self.cm.iter_mut() {
            let cur_item = core::mem::replace(item, empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>());
            free_cmd(&mut Command::PredictionMode(cur_item),
                     &mut m8.use_cached_allocation::<UninitializedOnAlloc>());
        }
        for item in self.result_cm.iter_mut() {
            let cur_item = core::mem::replace(item, empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>());
            free_cmd(&mut Command::PredictionMode(cur_item),
                     &mut m8.use_cached_allocation::<UninitializedOnAlloc>());
        }
    }
}

impl<AllocU8:Allocator<u8>, AllocCommand: Allocator<StaticCommand>> PullAllocatedCommand<AllocU8, AllocCommand> for SerialWorker<AllocU8, AllocCommand> {
    fn pull_command_buf(&mut self) -> (&mut AllocatedMemoryPrefix<StaticCommand, AllocCommand>,
                                       &mut [AllocatedMemoryRange<u8, AllocU8>;NUM_DATA_BUFFERED],
                                       &mut [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; NUM_DATA_BUFFERED], CommandResult) {
        self.pull()
    }
}

impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> MainToThread<AllocU8> for SerialWorker<AllocU8, AllocCommand> {
    const COOPERATIVE_MAIN:bool = true;
    type CommandOutputType = AllocatedMemoryPrefix<StaticCommand, AllocCommand>;
    #[inline(always)]
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()> {
        if self.cm_len == self.cm.len() {
            return Err(());
        }
        self.cm[self.cm_len] = cm;
        self.cm_len += 1;
        Ok(())
    }
    #[inline(always)]
    fn push(&mut self, data: &mut AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()> {
        if self.data_len == self.data.len() || data.slice().len() == 0 {
            return Err(());
        }
        self.data[self.data_len] = ThreadData::Data(core::mem::replace(data, AllocatedMemoryRange::<u8, AllocU8>::default()));
        self.data_len += 1;
        Ok(())        
    }
    #[inline(always)]
    fn pull(&mut self) -> (&mut Self::CommandOutputType,
                           &mut [AllocatedMemoryRange<u8, AllocU8>;NUM_DATA_BUFFERED],
                           &mut [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; NUM_DATA_BUFFERED], CommandResult) {
        if self.result.len() == 0 {
            assert_eq!(self.result_cm[0].has_context_speeds(), false);
            assert_eq!(self.result_cm[1].has_context_speeds(), false);
        }
        (&mut self.result, &mut self.result_data, &mut self.result_cm, self.eof_present_in_result)
    }
}
type NopUsize = usize;
pub struct ThreadToMainDemuxer<AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>>{
    pub worker: WorkerInterface,
    slice: AllocatedMemoryRange<u8, AllocU8>,
    unused: NopUsize,
    eof: bool,
}
impl<AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>+Default> Default for ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    fn default() -> Self {
        Self::new(WorkerInterface::default())
    }
}
impl <AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>> ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    #[inline(always)]
    pub fn new(w:WorkerInterface) -> Self {
        Self{
            worker:w,
            slice: AllocatedMemoryRange::<u8, AllocU8>::default(),
            unused: NopUsize::default(),
            eof: false,
        }
    }
    #[inline(always)]
    fn send_any_empty_data_buffer_to_main(&mut self) -> DivansOutputResult {
            if self.slice.slice().len() == 0 && self.slice.0.slice().len() != 0 {
                return self.worker.push_consumed_data(&mut self.slice, None);
            }
            DivansOutputResult::Success
        }
    #[inline(always)]
    fn pull_if_necessary(&mut self) -> DivansOutputResult{
        if self.slice.slice().len() == 0 {
            let ret = self.send_any_empty_data_buffer_to_main();
            match ret {
                DivansOutputResult::Success => {},
                need_something => return need_something,
            }
            match self.worker.pull_data() {
                ThreadData::Eof => {
                    self.eof = true;
                },
                ThreadData::Data(array) => {
                    self.slice = array
                },
                ThreadData::Yield => {},
            }
        }
        DivansOutputResult::Success
    }
}
impl <AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>+MainToThread<AllocU8>> ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    #[inline(always)]
    pub fn get_main_to_thread(&mut self) -> &mut WorkerInterface {
        &mut self.worker
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
        if stream_id != 0 {
            return 0;
        }
        self.slice.slice().len()
    }
    #[inline(always)]
    fn peek(&self, stream_id: StreamID) -> &[u8] {
        assert_eq!(stream_id, 0);
        self.slice.slice()
    }
    #[inline(always)]
    fn edit(&mut self, stream_id: StreamID) -> &mut AllocatedMemoryRange<u8, AllocU8> {
        assert_eq!(stream_id, 0);
        self.pull_if_necessary();
        &mut self.slice
    }
    #[inline(always)]
    fn consume(&mut self, stream_id: StreamID, count: usize) {
        assert_eq!(stream_id, 0);
        self.slice.1.start += count;
        self.send_any_empty_data_buffer_to_main();
    }
    #[inline(always)]
    fn consumed_all_streams_until_eof(&self) -> bool {
        self.eof && self.slice.slice().len() == 0
    }
    #[inline(always)]
    fn encountered_eof(&self) -> bool {
        self.eof && self.slice.slice().len() == 0
    }
    #[inline(always)]
    fn free_demux(&mut self, _m8: &mut AllocU8){
        if self.slice.0.slice().len() != 0 {
            self.worker.push_consumed_data(&mut self.slice, None);
        }
    }
}

impl <AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>> ThreadToMain<AllocU8> for ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    const COOPERATIVE:bool = WorkerInterface::COOPERATIVE;
    const ISOLATED:bool = WorkerInterface::ISOLATED;
    #[inline(always)]
    fn pull_data(&mut self) -> ThreadData<AllocU8> {
        self.worker.pull_data()
    }
    #[inline(always)]
    fn pull_context_map(&mut self,
                        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> Result<PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>, ()> {
        self.worker.pull_context_map(m8)
    }
    #[inline(always)]
    fn push_cmd<Specialization:EncoderOrDecoderRecoderSpecialization>(
        &mut self, cmd:&mut Command<AllocatedMemoryPrefix<u8, AllocU8>>,
        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
        recoder: Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
        specialization:&mut Specialization,
        output:&mut [u8],
        output_offset: &mut usize,
    ) -> DivansOutputResult {
        self.worker.push_cmd(cmd, m8, recoder, specialization, output, output_offset)
    }
    #[inline(always)]
    fn push_consumed_data(
        &mut self, data:&mut AllocatedMemoryRange<u8, AllocU8>,
        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
    ) -> DivansOutputResult {
        self.worker.push_consumed_data(data, m8)
    }
    #[inline(always)]
    fn push_eof(
        &mut self,
    ) -> DivansOutputResult {
        self.worker.push_eof()
    }
}

impl <AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>+MainToThread<AllocU8>> MainToThread<AllocU8> for ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    const COOPERATIVE_MAIN:bool = WorkerInterface::COOPERATIVE_MAIN;
    type CommandOutputType = WorkerInterface::CommandOutputType;
    #[inline(always)]
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()> {
        self.worker.push_context_map(cm)
    }
    #[inline(always)]
    fn push(&mut self, data: &mut AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()> {
        self.worker.push(data)
    }
    #[inline(always)]
    fn pull(&mut self) -> (&mut Self::CommandOutputType, &mut [AllocatedMemoryRange<u8, AllocU8>;NUM_DATA_BUFFERED], &mut [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; NUM_DATA_BUFFERED], CommandResult) {
        self.worker.pull()
    }

}
#[inline(always)]
pub fn downcast_command<AllocU8:Allocator<u8>>(cmd: &mut Command<AllocatedMemoryPrefix<u8, AllocU8>>) -> (StaticCommand, Option<&mut PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>>) {
    match cmd {
        &mut Command::PredictionMode(ref mut pm) => return (Command::PredictionMode(empty_prediction_mode_context_map()), Some(pm)),
        &mut Command::BlockSwitchCommand(mcc) => return (Command::BlockSwitchCommand(mcc), None),
        &mut Command::BlockSwitchDistance(mcc) => return (Command::BlockSwitchDistance(mcc), None),
        &mut Command::BlockSwitchLiteral(mcc) => return (Command::BlockSwitchLiteral(mcc), None),
        &mut Command::Dict(d) => return (Command::Dict(d), None),
        &mut Command::Copy(c) => return (Command::Copy(c), None),
        &mut Command::Literal(ref l) => return (Command::Literal(LiteralCommand{
            data:SlicePlaceholder32::<u8>::new(l.data.len() as u32),
            prob:FeatureFlagSliceType::default(),
            high_entropy: l.high_entropy,
        }), None),
    }
}

impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> ThreadToMain<AllocU8> for SerialWorker<AllocU8, AllocCommand> {
    const COOPERATIVE:bool = true;
    const ISOLATED:bool = true;
    #[inline(always)]
    fn pull_data(&mut self) -> ThreadData<AllocU8> {
        if self.data_len == 0 {
            return ThreadData::Yield;
        }
        assert!(self.data_len != 0);
        assert_eq!(self.data.len(), 2);
        let first = core::mem::replace(&mut self.data[1], ThreadData::Eof);
        let ret = core::mem::replace(&mut self.data[0], first);
        self.data_len -= 1;
        ret
    }
    #[inline(always)]
    fn pull_context_map(&mut self,
                        _m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> Result<PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>, ()> {
        if self.cm_len == 0 {
            return Err(());
        }
        assert!(self.cm_len != 0);
        let ret = core::mem::replace(&mut self.cm[self.cm_len - 1], PredictionModeContextMap::<AllocatedMemoryPrefix<u8, AllocU8>> {
            literal_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
            predmode_speed_and_distance_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
        });
        self.cm_len -= 1;
        Ok(ret)
    }
    #[inline(always)]
    fn push_cmd<Specialization:EncoderOrDecoderRecoderSpecialization>(
        &mut self,
        cmd:&mut Command<AllocatedMemoryPrefix<u8, AllocU8>>,
        _m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
        _recoder: Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
        _specialization: &mut Specialization,
        _output:&mut [u8],
        _output_offset: &mut usize,
    ) -> DivansOutputResult {
        if self.result.1 < self.result.0.len() {
            let (static_cmd, mut opt_cm) = downcast_command(cmd);
            if let Some(ref mut cm) = opt_cm {
                if self.result_cm[0].has_context_speeds() {
                    if self.result_cm[1].has_context_speeds() {
                        return DivansOutputResult::NeedsMoreOutput;
                    } else {
                        core::mem::swap(*cm, &mut self.result_cm[1]);
                    }
                } else {
                    core::mem::swap(*cm, &mut self.result_cm[1]);
                }
            }
            let index = self.result.1;
            self.result.1 += 1;
            self.result[index] = static_cmd;
        }
        DivansOutputResult::Success
    }
    #[inline(always)]
    fn push_consumed_data(&mut self,
                    data:&mut AllocatedMemoryRange<u8, AllocU8>,
                    _m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
    ) -> DivansOutputResult {
        if self.result_data[0].0.len() == 0 {
           core::mem::swap(&mut self.result_data[0], data); 
        } else if self.result_data[1].0.len() == 0 {
            core::mem::swap(&mut self.result_data[1], data); 
        } else {
            return DivansOutputResult::NeedsMoreOutput;
        }
        DivansOutputResult::Success
    }
   #[inline(always)]
    fn push_eof(&mut self,
    ) -> DivansOutputResult {
        self.set_eof_hint();
        DivansOutputResult::Success
    }
}
