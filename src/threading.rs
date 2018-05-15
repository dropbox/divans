use core;
#[allow(unused_imports)]
use interface::{DivansCompressorFactory, BlockSwitch, LiteralBlockSwitch, Command, Compressor, CopyCommand, Decompressor, DictCommand, LiteralCommand, Nop, NewWithAllocator, ArithmeticEncoderOrDecoder, LiteralPredictionModeNibble, PredictionModeContextMap, free_cmd, FeatureFlagSliceType, StreamDemuxer, ReadableBytes, StreamID, NUM_STREAMS, EncoderOrDecoderRecoderSpecialization};
use ::interface::DivansOutputResult;
use slice_util::{AllocatedMemoryRange, AllocatedMemoryPrefix};
use alloc::{SliceWrapper, Allocator};
use alloc_util::RepurposingAlloc;
use cmd_to_raw::DivansRecodeState;
pub enum ThreadData<AllocU8:Allocator<u8>> {
    Data(AllocatedMemoryRange<u8, AllocU8>),
    Eof,
}

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

pub enum CommandResult<AllocU8: Allocator<u8>, SliceType:SliceWrapper<u8>> {
    Cmd(Command<SliceType>),
    Eof,
    ProcessedData(AllocatedMemoryRange<u8, AllocU8>),
}
pub trait MainToThread<AllocU8:Allocator<u8>> {
    const COOPERATIVE_MAIN: bool;
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()>;
    fn push(&mut self, data: &mut AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()>;
    fn pull(&mut self) -> CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>;
}

pub trait ThreadToMain<AllocU8:Allocator<u8>> {
    const COOPERATIVE: bool;
    fn pull_data(&mut self) -> ThreadData<AllocU8>;
    fn pull_context_map(&mut self, m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>;
    //fn alloc_literal(&mut self, len: usize, m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> LiteralCommand<AllocatedMemoryPrefix<u8, AllocU8>>;
    fn push_command<Specialization:EncoderOrDecoderRecoderSpecialization>(
        &mut self,
        cmd:CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>,
        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
        recoder: Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
        specialization: &mut Specialization,
        output:&mut [u8],
        output_offset: &mut usize,
    ) -> (DivansOutputResult, Option<Command<AllocatedMemoryPrefix<u8, AllocU8>>>);
}

pub struct SerialWorker<AllocU8:Allocator<u8>> {
    data_len: usize,
    data: [ThreadData<AllocU8>;2],
    cm_len: usize,
    cm: [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; 2],
    result_len: usize,
    result:[CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>;3],
}
impl<AllocU8:Allocator<u8>> Default for SerialWorker<AllocU8> {
    fn default() -> Self {
        SerialWorker::<AllocU8> {
            data_len: 0,
            data:[ThreadData::<AllocU8>::default(),
                  ThreadData::<AllocU8>::default()],
            cm_len: 0,
            cm: [empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>(),
                 empty_prediction_mode_context_map::<AllocatedMemoryPrefix<u8, AllocU8>>()],
            result_len: 0,
            result: [
                CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),
                ],
        }
    }
}


impl<AllocU8:Allocator<u8>> MainToThread<AllocU8> for SerialWorker<AllocU8> {
    const COOPERATIVE_MAIN:bool = true;
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()> {
        if self.cm_len == self.cm.len() {
            eprint!("MAIN::(FAIL)_CONTEXT_MAP\n");
            return Err(());
        }
        eprint!("MAIN::PUSH_CONTEXT_MAP\n");
        self.cm[self.cm_len] = cm;
        self.cm_len += 1;
        Ok(())
    }
    fn push(&mut self, data: &mut AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()> {
        if self.data_len == self.data.len() || data.slice().len() == 0 {
            eprint!("MAIN::PUSH_0_DATA\n");
            return Err(());
        }
        eprint!("MAIN::PUSH_DATA {} [{}]\n", data.slice().len(), data.0.slice().len());
        self.data[self.data_len] = ThreadData::Data(core::mem::replace(data, AllocatedMemoryRange::<u8, AllocU8>::default()));
        self.data_len += 1;
        Ok(())        
    }
    fn pull(&mut self) -> CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>{
        assert!(self.result_len != 0);
        assert_eq!(self.result.len(), 3);
        let second = core::mem::replace(&mut self.result[2], CommandResult::Eof);
        let first = core::mem::replace(&mut self.result[1], second);
        let ret = core::mem::replace(&mut self.result[0], first);
        match ret {
            CommandResult::Eof => {
                eprint!("MAIN::RECV_EOF\n")
            },
            CommandResult::Cmd(_) => {
                eprint!("MAIN::RECV_CMD\n")
            },
            CommandResult::ProcessedData(ref dat) => {
                eprint!("MAIN::RECV_PROCESSED_DATA {}\n", dat.0.slice().len())
            },
        }
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
impl<AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>+Default> Default for ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    fn default() -> Self {
        Self::new(WorkerInterface::default())
    }
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
impl <AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>+MainToThread<AllocU8>> ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    pub fn get_main_to_thread(&mut self) -> &mut WorkerInterface {
        &mut self.worker
    }
}

struct NopEncoderOrDecoderRecoderSpecialization {}
impl EncoderOrDecoderRecoderSpecialization for NopEncoderOrDecoderRecoderSpecialization {
    fn get_recoder_output<'a>(&'a mut self, _passed_in_output_bytes: &'a mut [u8]) -> &'a mut[u8] {
        &mut []
    }
    fn get_recoder_output_offset<'a>(&self,
                                     _passed_in_output_bytes: &'a mut usize,
                                     backing: &'a mut usize) -> &'a mut usize {
        backing
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
        if self.slice.slice().len() == 0 && self.slice.0.slice().len() != 0 {
            let mut unused = 0usize;
            self.worker.push_command(CommandResult::ProcessedData(
                core::mem::replace(&mut self.slice, AllocatedMemoryRange::<u8, AllocU8>::default())), None, None, &mut NopEncoderOrDecoderRecoderSpecialization{}, &mut [], &mut unused); //FIXME(threading): I think passing None here is fine since the receiver will free it
        }
    }
    #[inline(always)]
    fn encountered_eof(&self) -> bool {
        self.eof && self.slice.slice().len() == 0
    }
    #[inline(always)]
    fn free_demux(&mut self, _m8: &mut AllocU8){
        if self.slice.0.slice().len() != 0 {
            let mut unused = 0usize;
            self.worker.push_command(CommandResult::ProcessedData(
                core::mem::replace(&mut self.slice, AllocatedMemoryRange::<u8, AllocU8>::default())), None, None, &mut NopEncoderOrDecoderRecoderSpecialization{}, &mut [], &mut unused);
        }
    }
}

impl <AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>> ThreadToMain<AllocU8> for ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    const COOPERATIVE:bool = WorkerInterface::COOPERATIVE;
    #[inline(always)]
    fn pull_data(&mut self) -> ThreadData<AllocU8> {
        self.worker.pull_data()
    }
    #[inline(always)]
    fn pull_context_map(&mut self,
                        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>> {
        self.worker.pull_context_map(m8)
    }
    #[inline(always)]
    fn push_command<Specialization:EncoderOrDecoderRecoderSpecialization>(
        &mut self, cmd:CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>,
        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
        recoder: Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
        specialization:&mut Specialization,
        output:&mut [u8],
        output_offset: &mut usize,
    ) -> (DivansOutputResult, Option<Command<AllocatedMemoryPrefix<u8, AllocU8>>>) {
        self.worker.push_command(cmd, m8, recoder, specialization, output, output_offset)
    }
}

impl <AllocU8:Allocator<u8>, WorkerInterface:ThreadToMain<AllocU8>+MainToThread<AllocU8>> MainToThread<AllocU8> for ThreadToMainDemuxer<AllocU8, WorkerInterface> {
    const COOPERATIVE_MAIN:bool = WorkerInterface::COOPERATIVE_MAIN;
    #[inline(always)]
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()> {
        self.worker.push_context_map(cm)
    }
    #[inline(always)]
    fn push(&mut self, data: &mut AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()> {
        self.worker.push(data)
    }
    #[inline(always)]
    fn pull(&mut self) -> CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>> {
        self.worker.pull()
    }

}

impl<AllocU8:Allocator<u8>> ThreadToMain<AllocU8> for SerialWorker<AllocU8> {
    const COOPERATIVE:bool = true;
    fn pull_data(&mut self) -> ThreadData<AllocU8> {
        assert!(self.data_len != 0);
        assert_eq!(self.data.len(), 2);
        let first = core::mem::replace(&mut self.data[1], ThreadData::Eof);
        let ret = core::mem::replace(&mut self.data[0], first);
        self.data_len -= 1;
        match ret {
            ThreadData::Eof => eprint!("THREAD::PULL_EOF\n"),
            ThreadData::Data(ref v) => eprint!("THREAD::PULL_DATA {} [{}]\n", v.slice().len(), v.0.slice().len()),
        }
        ret
    }
    fn pull_context_map(&mut self,
                        _m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>> {
        assert!(self.cm_len != 0);
        let ret = core::mem::replace(&mut self.cm[self.cm_len - 1], PredictionModeContextMap::<AllocatedMemoryPrefix<u8, AllocU8>> {
            literal_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
            predmode_speed_and_distance_context_map:AllocatedMemoryPrefix::<u8, AllocU8>::default(),
        });
        eprint!("THREAD::PULL_CONTEXT_MAP\n");
        self.cm_len -= 1;
        ret
    }
    fn push_command<Specialization:EncoderOrDecoderRecoderSpecialization>(&mut self,
                    cmd:CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>,
                    _m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
                    _recoder:Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
                    _specialization:&mut Specialization,
                    _output:&mut [u8],
                    _output_offset: &mut usize,
    ) -> (DivansOutputResult, Option<Command<AllocatedMemoryPrefix<u8, AllocU8>>>) {
        if self.result_len == self.result.len() {
            if let CommandResult::Cmd(command) = cmd {
                eprint!("THREAD::(FAILED) PROCESSED_DATA CMD\n");
                return (DivansOutputResult::NeedsMoreOutput, Some(command));
            } else {
                match cmd{
                    CommandResult::Cmd(command) => unreachable!(),
                    CommandResult::Eof => eprint!("THREAD::(FAILED) PROCESSED_EOF ??\n"),
                    CommandResult::ProcessedData(ref dat) => eprint!("THREAD::(FAILED) PROCESSED_DATA {}\n", dat.0.slice().len()),
                }
                return (DivansOutputResult::NeedsMoreOutput, None);
            }
        }
        match cmd{
            CommandResult::Cmd(ref command) => eprint!("THREAD::PUSH_PROCESSED_CMD\n"),
            CommandResult::Eof => eprint!("THREAD::PUSH PROCESSED_EOF ??\n"),
            CommandResult::ProcessedData(ref dat) => eprint!("THREAD::PUSH PROCESSED_DATA {}\n", dat.0.slice().len()),
        }
        
        self.result[self.result_len] = cmd;
        self.result_len += 1;
        (DivansOutputResult::Success, None)
    }
}
