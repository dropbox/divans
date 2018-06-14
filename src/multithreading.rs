#![cfg(not(feature="no-stdlib"))]
use core;

use std::sync::{Arc, Mutex, Condvar};
use threading::{SerialWorker, MainToThread, ThreadToMain, CommandResult, ThreadData, NUM_SERIAL_COMMANDS_BUFFERED, NUM_DATA_BUFFERED,};
use slice_util::{AllocatedMemoryRange, AllocatedMemoryPrefix};
use alloc::{Allocator, SliceWrapper, SliceWrapperMut};
use alloc_util::RepurposingAlloc;
use cmd_to_raw::DivansRecodeState;
use interface::{PredictionModeContextMap, EncoderOrDecoderRecoderSpecialization, Command, DivansOpResult, DivansOutputResult, ErrMsg};
use std::time::{SystemTime, Duration};
use threading::{StaticCommand, PullAllocatedCommand, downcast_command};
#[cfg(feature="threadlog")]
const MAX_LOG_SIZE: usize = 8192;
#[cfg(not(feature="threadlog"))]
const MAX_LOG_SIZE: usize = 0;


pub struct MultiWorker<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> {
    start: SystemTime,
    queue: Arc<(Mutex<SerialWorker<AllocU8, AllocCommand>>, Condvar)>,
    log: [ThreadEvent; MAX_LOG_SIZE],
    log_offset: u32,
}
#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
enum ThreadEventType {
    M_PUSH_CONTEXT_MAP,
    M_WAIT_PUSH_CONTEXT_MAP,
    M_PUSH_DATA,
    M_PUSH_EMPTY_DATA,
    M_FAIL_PUSH_DATA,
    M_PULL_COMMAND_RESULT,
    M_BROADCAST_ERR,
    M_WAIT_PULL_COMMAND_RESULT,
    W_PULL_DATA,
    W_WAIT_PULL_DATA,
    W_PULL_CONTEXT_MAP,
    W_WAIT_PULL_CONTEXT_MAP,
    W_PUSH_CMD,
    W_WAIT_PUSH_CMD,
    W_PUSH_BATCH_CMD,
    W_WAIT_PUSH_BATCH_CMD,
    W_PUSH_CONSUMED_DATA,
    W_WAIT_PUSH_CONSUMED_DATA,
    W_PUSH_EOF,
    W_WAIT_PUSH_EOF,
    W_BROADCAST_ERR,
}
#[derive(Debug, Clone, Copy)]
struct ThreadEvent(ThreadEventType, u32, Duration);
#[cfg(feature="threadlog")]
macro_rules! unguarded_debug_time {
    ($proc: expr) => {
        $proc.start.elapsed().unwrap_or(Duration::new(0,0))
    }
        
}
#[cfg(not(feature="threadlog"))]
macro_rules! unguarded_debug_time {
    ($proc: expr) => {
        ()
    }
}
#[cfg(feature="threadlog")]
macro_rules! thread_debug {
    ($en: expr, $quant: expr, $proc: expr, $timevar: expr) => {
        if $proc.log_offset as usize != $proc.log.len() {
            $proc.log[$proc.log_offset as usize] = ThreadEvent($en, $quant as u32, $timevar);
            $proc.log_offset += 1;
        }
        eprintln!("{:?} {} {:?}", $en, $quant, $proc.start.elapsed().unwrap_or(Duration::new(0,0)));
    };
}

#[cfg(not(feature="threadlog"))]
macro_rules! thread_debug {
    ($a: expr, $b: expr, $c: expr, $d: expr) => {
    };
}

impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> Clone for MultiWorker<AllocU8, AllocCommand> {
    fn clone(&self) -> Self {
        Self {
            log:self.log.clone(),
            log_offset:self.log_offset.clone(),
            start:self.start,
            queue:self.queue.clone(),
        }
    }
}

#[cfg(feature="threadlog")]
impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> Drop for MultiWorker<AllocU8, AllocCommand> {
    fn drop(&mut self) {
        let epoch_d = self.start.duration_since(std::time::UNIX_EPOCH).unwrap_or(Duration::new(0,0));
        let epoch = (epoch_d.as_secs()%100) * 100 + u64::from(epoch_d.subsec_nanos()) / 10000000;
        let start_log = self.start.elapsed().unwrap_or(Duration::new(0,0));
        use std;
        use std::io::Write;
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        writeln!(handle, "{:04}:{:02}:{:09}:LOG_START", epoch, start_log.as_secs(), start_log.subsec_nanos()).unwrap();
        for entry in self.log[..self.log_offset as usize].iter() {
            writeln!(handle, "{:04}:{:02}:{:09}:{:?}:{}", epoch, entry.2.as_secs(), entry.2.subsec_nanos(), entry.0, entry.1).unwrap();
        }
        let fin_log = self.start.elapsed().unwrap_or(Duration::new(0,0));
        writeln!(handle, "{:04}:{:02}:{:09}:LOG_FLUSH:{:?}ns", epoch, fin_log.as_secs(), fin_log.subsec_nanos(), fin_log - start_log).unwrap();
    }
}



impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> MultiWorker<AllocU8, AllocCommand>
{
    pub fn new(mcommand: &mut AllocCommand) -> Self {
        MultiWorker::<AllocU8, AllocCommand> {
            log:[ThreadEvent(ThreadEventType::M_PUSH_EMPTY_DATA, 0, Duration::new(0,0)); MAX_LOG_SIZE],
            log_offset:0,
            start: SystemTime::now(),
            queue: Arc::new((Mutex::new(SerialWorker::<AllocU8, AllocCommand>::new(mcommand)), Condvar::new())),
        }
    }
    fn broadcast_err_internal(&mut self, err: ErrMsg, _thread_event_type: ThreadEventType) {
        let _elapsed = unguarded_debug_time!(self);
        let &(ref lock, ref cvar) = &*self.queue;
        let mut worker = lock.lock().unwrap();
        if worker.waiters != 0 {
            cvar.notify_one();
        }
        let ret = worker.broadcast_err_internal(err);
        thread_debug!(_thread_event_type, output.len(), self, _elapsed);
        return ret;        
    }
    pub fn free(&mut self, m8: &mut RepurposingAlloc<u8, AllocU8>, mcommand: &mut AllocCommand) {
        let &(ref lock, ref cvar) = &*self.queue;
        let mut worker = lock.lock().unwrap();
        if worker.waiters != 0 {
            worker.broadcast_err_internal(ErrMsg::UnexpectedEof);
            cvar.notify_one();
        }
        worker.free(m8, mcommand);
    }
}
impl<AllocU8:Allocator<u8>, AllocCommand: Allocator<StaticCommand>> PullAllocatedCommand<AllocU8, AllocCommand> for MultiWorker<AllocU8, AllocCommand> {
    fn pull_command_buf(&mut self,
                        output:&mut AllocatedMemoryPrefix<StaticCommand, AllocCommand>,
                        consumed_data:&mut [AllocatedMemoryRange<u8, AllocU8>;NUM_DATA_BUFFERED],
                        pm:&mut [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; 2]) -> CommandResult {
        self.pull(output, consumed_data, pm)
    }
}

impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> MainToThread<AllocU8> for MultiWorker<AllocU8, AllocCommand> {
    const COOPERATIVE_MAIN:bool = false;
    type CommandOutputType= <SerialWorker<AllocU8, AllocCommand> as MainToThread<AllocU8>>::CommandOutputType;
    #[inline(always)]
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()> {
        
        loop { // FIXME: should this loop forever? We should never run out of context map room
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.cm_space_ready() {
                thread_debug!(ThreadEventType::M_PUSH_CONTEXT_MAP, 1, self, _elapsed);
                if worker.waiters != 0 {
                    cvar.notify_one();
                }
                return worker.push_context_map(cm);
            } else {
                thread_debug!(ThreadEventType::M_WAIT_PUSH_CONTEXT_MAP, 0, self, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker); // always safe to loop around again
                _ign.unwrap().waiters -= 1;
            }
        }
    }
    #[inline(always)]
    fn push(&mut self, data: &mut AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()> {
        let _elapsed = unguarded_debug_time!(self);
        let _len = data.len();
        let &(ref lock, ref cvar) = &*self.queue;
        let mut worker = lock.lock().unwrap();
        match worker.push(data) {
            Ok(()) => {
                thread_debug!(ThreadEventType::M_PUSH_DATA, _len, self, _elapsed);
                if worker.waiters != 0 {
                    cvar.notify_one();
                }
                return Ok(());
            },
            err => {
                if data.len() == 0 {
                    thread_debug!(ThreadEventType::M_PUSH_EMPTY_DATA, 0, self, _elapsed);
                } else {
                    thread_debug!(ThreadEventType::M_FAIL_PUSH_DATA, 0, self, _elapsed);
                }
                return err
            },
        }
    }

    #[inline(always)]
    fn pull(&mut self,
            output:&mut Self::CommandOutputType,
            consumed_data:&mut [AllocatedMemoryRange<u8, AllocU8>;NUM_DATA_BUFFERED],
            pm:&mut [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; 2]) -> CommandResult {
        loop {
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_ready() {
                if worker.waiters != 0 {
                    cvar.notify_one(); // FIXME: do we want to signal here?
                }
                let ret = worker.pull(output, consumed_data, pm);
                thread_debug!(ThreadEventType::M_PULL_COMMAND_RESULT, output.len(), self, _elapsed);
                return ret;
            } else if worker.err.is_none() {
                thread_debug!(ThreadEventType::M_WAIT_PULL_COMMAND_RESULT, 0, self, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -= 1;
                //return CommandResult::ProcessedData(AllocatedMemoryRange::<u8, AllocU8>::default()); // FIXME: busy wait
            } else {
                return CommandResult::Err(worker.err.unwrap());
            }
        }
    }
    fn broadcast_err(&mut self,
                     err:ErrMsg) {
        self.broadcast_err_internal(err, ThreadEventType::M_BROADCAST_ERR);
    }
}

impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> ThreadToMain<AllocU8> for MultiWorker<AllocU8, AllocCommand> {
    const COOPERATIVE:bool = false;
    const ISOLATED:bool = true;
    #[inline(always)]
    fn pull_data(&mut self) -> ThreadData<AllocU8> {
        loop {
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.data_ready() {
                let ret = worker.pull_data();
                thread_debug!(ThreadEventType::W_PULL_DATA, match ret {ThreadData::Data(ref d) => d.len(), ThreadData::Yield => 0, ThreadData::Eof=> 99999999,}, self, _elapsed);
                return ret;
            } else {
                thread_debug!(ThreadEventType::W_WAIT_PULL_DATA, 0, self, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -= 1;
            }
        }
    }
    #[inline(always)]
    fn pull_context_map(&mut self,
                        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> Result<PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>, ()> {
        loop {
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.cm_ready() {
                if worker.waiters != 0 {
                    cvar.notify_one();
                }
                thread_debug!(ThreadEventType::W_PULL_CONTEXT_MAP, 1, self, _elapsed);
                return worker.pull_context_map(m8);
            } else {
                thread_debug!(ThreadEventType::W_WAIT_PULL_CONTEXT_MAP, 0, self, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -= 1;
            }
        }
    }
    #[inline(always)]
    fn push_cmd<Specialization:EncoderOrDecoderRecoderSpecialization>(
        &mut self,
        cmd:&mut Command<AllocatedMemoryPrefix<u8, AllocU8>>,
        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
        recoder: Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
        specialization: &mut Specialization,
        output:&mut [u8],
        output_offset: &mut usize,
    ) -> DivansOutputResult {
        loop {
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_space_ready() {
                thread_debug!(ThreadEventType::W_PUSH_CMD, 1, self, _elapsed);
                if worker.waiters != 0 {
                    cvar.notify_one();
                }
                return worker.push_cmd(cmd, m8, recoder, specialization, output, output_offset);
            } else {
                thread_debug!(ThreadEventType::W_WAIT_PUSH_CMD, 0, self, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -= 1;
            }
        }
    }
    #[inline(always)]
    fn push_consumed_data(&mut self,
                    data:&mut AllocatedMemoryRange<u8, AllocU8>,
                    m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
    ) -> DivansOutputResult {
        let _len = data.len();
        loop {
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_space_ready() {
                if worker.waiters != 0 {
                    cvar.notify_one();
                }
                thread_debug!(ThreadEventType::W_PUSH_CONSUMED_DATA, _len, self, _elapsed);
                return worker.push_consumed_data(data, m8);
            } else {
                thread_debug!(ThreadEventType::W_WAIT_PUSH_CONSUMED_DATA, 0, self, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -= 1;
            }
        }
    }
   #[inline(always)]
    fn push_eof(&mut self,
    ) -> DivansOutputResult {
        loop {
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_space_ready() {
                if worker.waiters != 0 {
                    cvar.notify_one();
                }
                thread_debug!(ThreadEventType::W_PUSH_EOF, 1, self, _elapsed);
                return worker.push_eof();
            } else {
                thread_debug!(ThreadEventType::W_WAIT_PUSH_EOF, 1, self, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -=1;
            }
        }
    }
    fn broadcast_err(&mut self, err: ErrMsg
    ) {
        self.broadcast_err_internal(err, ThreadEventType::W_BROADCAST_ERR);
    }
}

pub struct BufferedMultiWorker<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> {
    pub worker: MultiWorker<AllocU8, AllocCommand>,
    buffer: AllocatedMemoryPrefix<StaticCommand, AllocCommand>,
    min_buffer_push_len: usize,
}
/*
impl<AllocU8:Allocator<u8>, AllocCommand: Allocator<StaticCommand>> PullAllocatedCommand<AllocU8, AllocCommand> for BufferedMultiWorker<AllocU8, AllocCommand> {
    fn pull_command_buf(&mut self) -> (&mut AllocatedMemoryPrefix<StaticCommand, AllocCommand>,
                                       &mut [AllocatedMemoryRange<u8, AllocU8>;NUM_DATA_BUFFERED],
                                       &mut [PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>; NUM_DATA_BUFFERED], CommandResult) {
        self.pull()
    }
}*/


impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> BufferedMultiWorker<AllocU8, AllocCommand> {
    pub fn new(mc: &mut AllocCommand)->Self{
        let worker = MultiWorker::<AllocU8, AllocCommand>::new(mc);
        Self {
            min_buffer_push_len: 2,
            worker:worker,
            buffer: AllocatedMemoryPrefix::realloc(mc.alloc_cell(NUM_SERIAL_COMMANDS_BUFFERED), 0),
        }
    }
    fn force_push(&mut self, eof_inside: bool, data: &mut AllocatedMemoryRange<u8, AllocU8>, pm: Option<&mut PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>>) -> DivansOpResult {
        if self.min_buffer_push_len * 2 < self.buffer.max_len(){
            self.min_buffer_push_len <<= 2;
        }
        loop {
            let _elapsed = unguarded_debug_time!(self.worker);
            let &(ref lock, ref cvar) = &*self.worker.queue;
            let mut worker = lock.lock().unwrap();
            let mut did_notify = false;
            if data.0.len() != 0 { // before we get to sending commands, lets make sure data is taken care of
                match worker.push_consumed_data(data, None) {
                    DivansOutputResult::Success => {
                        thread_debug!(ThreadEventType::W_PUSH_CONSUMED_DATA, data.0.len() as u32, self.worker, _elapsed);
                    },
                    DivansOutputResult::NeedsMoreOutput => {
                        thread_debug!(ThreadEventType::W_WAIT_PUSH_CONSUMED_DATA, data.0.len(), self.worker, _elapsed);
                        worker.waiters += 1;
                        let _ign = cvar.wait(worker);
                        _ign.unwrap().waiters -= 1;
                        continue;
                    }
                    DivansOutputResult::Failure(e) => {
                        worker.set_error(e);
                    }
                }
                if worker.waiters != 0 && !did_notify{
                    cvar.notify_one();
                    did_notify = true;
                }
            }
            if worker.result_multi_space_ready(self.buffer.1 as usize) {
                thread_debug!(ThreadEventType::W_PUSH_CMD, self.buffer.1, self.worker, _elapsed);
                if eof_inside {
                    worker.set_eof_hint(); // so other side gets more aggressive about pulling
                }
                if worker.waiters != 0 && !did_notify{
                    cvar.notify_one();
                }
                let extant_space = worker.insert_results(&mut self.buffer, pm);
                if extant_space <= 16 {
                    self.min_buffer_push_len = core::cmp::max(self.min_buffer_push_len >> 1, 4);
                }
                self.buffer.1 = 0;
                return DivansOpResult::Success;
            } else if worker.err.is_none() {
                thread_debug!(ThreadEventType::W_WAIT_PUSH_CMD, self.buffer.1, self.worker, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -= 1;
            } else {
                return DivansOpResult::Failure(worker.err.unwrap());
            }
        }
    }
    pub fn free(&mut self, m8: &mut RepurposingAlloc<u8, AllocU8>, mc: &mut AllocCommand) {
        self.worker.free(m8, mc);
    }
}
impl<AllocU8:Allocator<u8>, AllocCommand:Allocator<StaticCommand>> ThreadToMain<AllocU8> for BufferedMultiWorker<AllocU8, AllocCommand> {
    const COOPERATIVE:bool = false;
    const ISOLATED:bool = true;
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
        &mut self,
        cmd:&mut Command<AllocatedMemoryPrefix<u8, AllocU8>>,
        _m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
        _recoder: Option<&mut DivansRecodeState<AllocU8::AllocatedMemory>>,
        _specialization: &mut Specialization,
        _output:&mut [u8],
        _output_offset: &mut usize,
    ) -> DivansOutputResult {
        let (static_command, pm) = downcast_command(cmd);
        self.buffer.0.slice_mut()[self.buffer.1 as usize] = static_command;
        self.buffer.1 += 1;
        if pm.is_some() {
            DivansOutputResult::from(self.force_push(false, &mut AllocatedMemoryRange::<u8, AllocU8>::default(), pm))
        } else if self.buffer.1 as usize == self.buffer.0.len() || self.buffer.1 as usize == self.min_buffer_push_len {
            DivansOutputResult::from(self.force_push(false, &mut AllocatedMemoryRange::<u8, AllocU8>::default(), None))
        } else {
            //FIXME: why does this case not do anything
            DivansOutputResult::Success
        }
    }
    #[inline(always)]
    fn push_consumed_data(&mut self,
                    data:&mut AllocatedMemoryRange<u8, AllocU8>,
                    _m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
    ) -> DivansOutputResult {
        DivansOutputResult::from(self.force_push(false, data, None))
    }
   #[inline(always)]
    fn push_eof(&mut self,
    ) -> DivansOutputResult {
        DivansOutputResult::from(self.force_push(true, &mut AllocatedMemoryRange::<u8, AllocU8>::default(), None))
    }
   #[inline(always)]
    fn broadcast_err(&mut self, err: ErrMsg) {
        self.worker.broadcast_err_internal(err, ThreadEventType::W_BROADCAST_ERR)
    }
}
