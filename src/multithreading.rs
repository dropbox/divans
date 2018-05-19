#![cfg(not(feature="no-stdlib"))]
use core;

use std::sync::{Arc, Mutex, Condvar};
use threading::{SerialWorker, MainToThread, ThreadToMain, CommandResult, ThreadData, NUM_SERIAL_COMMANDS_BUFFERED};
use slice_util::{AllocatedMemoryRange, AllocatedMemoryPrefix};
use alloc::{Allocator, SliceWrapper};
use alloc_util::RepurposingAlloc;
use cmd_to_raw::DivansRecodeState;
use interface::{PredictionModeContextMap, EncoderOrDecoderRecoderSpecialization, Command, DivansOutputResult, Nop};
use std::time::{SystemTime, Duration};

#[cfg(feature="threadlog")]
const MAX_LOG_SIZE: usize = 8192;
#[cfg(not(feature="threadlog"))]
const MAX_LOG_SIZE: usize = 0;


pub struct MultiWorker<AllocU8:Allocator<u8>> {
    start: SystemTime,
    queue: Arc<(Mutex<SerialWorker<AllocU8>>, Condvar)>,
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
    W_WAIT_PUSH_EOF
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
        //eprintln!("{:?} {} {:?}", $en, $quant, $proc.start.elapsed().unwrap_or(Duration::new(0,0)));
    };
}

#[cfg(not(feature="threadlog"))]
macro_rules! thread_debug {
    ($a: expr, $b: expr, $c: expr, $d: expr) => {
    };
}

impl<AllocU8:Allocator<u8>> Clone for MultiWorker<AllocU8> {
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
impl<AllocU8:Allocator<u8>> Drop for MultiWorker<AllocU8> {
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



impl<AllocU8:Allocator<u8>> Default for MultiWorker<AllocU8> {
    fn default() -> Self {
        MultiWorker::<AllocU8> {
            log:[ThreadEvent(ThreadEventType::M_PUSH_EMPTY_DATA, 0, Duration::new(0,0)); MAX_LOG_SIZE],
            log_offset:0,
            start: SystemTime::now(),
            queue: Arc::new((Mutex::new(SerialWorker::<AllocU8>::default()), Condvar::new())),
        }
    }
}
impl<AllocU8:Allocator<u8>> MainToThread<AllocU8> for MultiWorker<AllocU8> {
    const COOPERATIVE_MAIN:bool = false;
    #[inline(always)]
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()> {
        
        loop { // FIXME: should this loop forever? We should never run out of context map room
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.cm_space_ready() {
                thread_debug!(ThreadEventType::M_PUSH_CONTEXT_MAP, 1, self, _elapsed);
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
    fn pull(&mut self, output: &mut [CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>]) -> usize {
        loop {
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_ready() {
                if worker.waiters != 0 {
                    cvar.notify_one(); // FIXME: do we want to signal here?
                }
                let ret = worker.pull(&mut output[..]);
                thread_debug!(ThreadEventType::M_PULL_COMMAND_RESULT, ret, self, _elapsed);
                return ret;
            } else {
                thread_debug!(ThreadEventType::M_WAIT_PULL_COMMAND_RESULT, 0, self, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -= 1;
                //return CommandResult::ProcessedData(AllocatedMemoryRange::<u8, AllocU8>::default()); // FIXME: busy wait
            } 
        }
    }
}

impl<AllocU8:Allocator<u8>> ThreadToMain<AllocU8> for MultiWorker<AllocU8> {
    const COOPERATIVE:bool = false;
    const ISOLATED:bool = true;
    #[inline(always)]
    fn pull_data(&mut self) -> ThreadData<AllocU8> {
        loop {
            let _elapsed = unguarded_debug_time!(self);
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.data_ready() {
                thread_debug!(ThreadEventType::W_PULL_DATA, 1, self, _elapsed);
                return worker.pull_data();
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
}

pub struct BufferedMultiWorker<AllocU8:Allocator<u8>> {
    pub worker: MultiWorker<AllocU8>,
    buffer: [CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>;NUM_SERIAL_COMMANDS_BUFFERED],
    buffer_len: usize,
    min_buffer_push_len: usize,
}
impl<AllocU8:Allocator<u8>> Default for BufferedMultiWorker<AllocU8> {
    fn default() -> Self {
        Self::new(MultiWorker::default())
    }
}
impl<AllocU8:Allocator<u8>> BufferedMultiWorker<AllocU8> {
    pub fn new(worker:MultiWorker<AllocU8>)->Self{
        Self {
            min_buffer_push_len: 2,
            worker:worker,
            buffer: [
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),


                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),



                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),


                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),

                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
                CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),CommandResult::Cmd(Command::nop()),
            ],
            buffer_len:0,
        }
    }
    fn force_push(&mut self, eof_inside: bool) {
        if self.min_buffer_push_len * 2 < self.buffer.len(){
            self.min_buffer_push_len <<= 2;
        }
        loop {
            let _elapsed = unguarded_debug_time!(self.worker);
            let &(ref lock, ref cvar) = &*self.worker.queue;
            let mut worker = lock.lock().unwrap();
            if eof_inside {
                worker.set_eof_hint(); // so other side gets more aggressive about pulling
            }
            if worker.result_multi_space_ready(self.buffer_len) {
                thread_debug!(ThreadEventType::W_PUSH_CMD, self.buffer_len, self.worker, _elapsed);
                if worker.waiters != 0 {
                    cvar.notify_one();
                }
                let extant_space = worker.insert_results(self.buffer.split_at_mut(self.buffer_len).0);
                if extant_space <= 16 {
                    self.min_buffer_push_len = core::cmp::max(self.min_buffer_push_len >> 1, 4);
                    
                }
                self.buffer_len = 0;
                return;
            } else {
                thread_debug!(ThreadEventType::W_WAIT_PUSH_CMD, self.buffer_len, self.worker, _elapsed);
                worker.waiters += 1;
                let _ign = cvar.wait(worker);
                _ign.unwrap().waiters -= 1;
            }
        }
    }
}
impl<AllocU8:Allocator<u8>> ThreadToMain<AllocU8> for BufferedMultiWorker<AllocU8> {
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
        let force_push = if let &mut Command::PredictionMode(_) = cmd {
            true
        } else {
            false
        };
        self.buffer[self.buffer_len] =  CommandResult::Cmd(core::mem::replace(cmd, Command::nop()));
        self.buffer_len += 1;
        if force_push || self.buffer_len == self.buffer.len() || self.buffer_len == self.min_buffer_push_len {
            self.force_push(false);
        }
        DivansOutputResult::Success
    }
    #[inline(always)]
    fn push_consumed_data(&mut self,
                    data:&mut AllocatedMemoryRange<u8, AllocU8>,
                    _m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
    ) -> DivansOutputResult {
        self.buffer[self.buffer_len] = CommandResult::ProcessedData(core::mem::replace(data, AllocatedMemoryRange::<u8, AllocU8>::default()));
        self.buffer_len += 1;
        self.force_push(false);
        DivansOutputResult::Success
    }
   #[inline(always)]
    fn push_eof(&mut self,
    ) -> DivansOutputResult {
        self.buffer[self.buffer_len] = CommandResult::Eof;
        self.buffer_len += 1;
        self.force_push(true);
        DivansOutputResult::Success
    }
}
