#![cfg(not(feature="no-stdlib"))]
use std::sync::{Arc, Mutex, Condvar};
use threading::{SerialWorker, MainToThread, ThreadToMain, CommandResult, ThreadData};
use slice_util::{AllocatedMemoryRange, AllocatedMemoryPrefix};
use alloc::{Allocator, SliceWrapper};
use alloc_util::RepurposingAlloc;
use cmd_to_raw::DivansRecodeState;
use interface::{PredictionModeContextMap, EncoderOrDecoderRecoderSpecialization, Command, DivansOutputResult};
pub struct MultiWorker<AllocU8:Allocator<u8>> {
    queue: Arc<(Mutex<SerialWorker<AllocU8>>, Condvar)>,
}
impl<AllocU8:Allocator<u8>> Clone for MultiWorker<AllocU8> {
    fn clone(&self) -> Self {
        Self {
            queue:self.queue.clone(),
        }
    }
}


impl<AllocU8:Allocator<u8>> Default for MultiWorker<AllocU8> {
    fn default() -> Self {
        MultiWorker::<AllocU8> {
            queue: Arc::new((Mutex::new(SerialWorker::<AllocU8>::default()), Condvar::new())),
        }
    }
}
impl<AllocU8:Allocator<u8>> MainToThread<AllocU8> for MultiWorker<AllocU8> {
    const COOPERATIVE_MAIN:bool = false;
    #[inline(always)]
    fn push_context_map(&mut self, cm: PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>) -> Result<(),()> {
        loop { // FIXME: should this loop forever? We should never run out of context map room
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.cm_space_ready() {
                //eprintln!("M:PUSH_CONTEXT_MAP");
                return worker.push_context_map(cm);
            } else {
                //eprintln!("M:WAIT_PUSH_CONTEXT_MAP");
                let _ign = cvar.wait(worker); // always safe to loop around again
            }
        }
    }
    #[inline(always)]
    fn push(&mut self, data: &mut AllocatedMemoryRange<u8, AllocU8>) -> Result<(),()> {
        let &(ref lock, ref cvar) = &*self.queue;
        match lock.lock().unwrap().push(data) {
            Ok(()) => {
                //eprintln!("M:PUSH_{}_DATA", data.len());
                cvar.notify_one();
                return Ok(());
            },
            err => {
                if data.len() == 0 {
                    //eprintln!("M:PUSH_0_DATA");
                } else {
                    //eprintln!("M:FAIL_PUSH_DATA");
                }
                return err
            },
        }
    }
    #[inline(always)]
    fn pull(&mut self) -> CommandResult<AllocU8, AllocatedMemoryPrefix<u8, AllocU8>>{
        loop {
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_ready() {
                cvar.notify_one(); // FIXME: do we want to signal here?
                //eprintln!("M:PULL_COMMAND_RESULT");
                return worker.pull();
            } else {
                //eprintln!("M:WAIT_PULL_COMMAND_RESULT");
                let _ign = cvar.wait(worker);
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
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.data_ready() {
                //eprintln!("W:PULL_DATA");
                return worker.pull_data();
            } else {
                //eprintln!("W:WAIT_DATA");
                let _ign = cvar.wait(worker);
            }
        }
    }
    #[inline(always)]
    fn pull_context_map(&mut self,
                        m8: Option<&mut RepurposingAlloc<u8, AllocU8>>) -> Result<PredictionModeContextMap<AllocatedMemoryPrefix<u8, AllocU8>>, ()> {
        loop {
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.cm_ready() {
                cvar.notify_one();
                //eprintln!("W:PULL_CONTEXT_MAP");
                return worker.pull_context_map(m8);
            } else {
                //eprintln!("W:WAIT_PULL_CONTEXT_MAP");
                let _ign = cvar.wait(worker);
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
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_space_ready() {
                //eprintln!("W:PUSH_CMD");
                cvar.notify_one();
                return worker.push_cmd(cmd, m8, recoder, specialization, output, output_offset);
            } else {
                //eprintln!("W:WAIT_PUSH_CMD");
                let _ign = cvar.wait(worker);
            }
        }
    }
    #[inline(always)]
    fn push_consumed_data(&mut self,
                    data:&mut AllocatedMemoryRange<u8, AllocU8>,
                    m8: Option<&mut RepurposingAlloc<u8, AllocU8>>,
    ) -> DivansOutputResult {
        loop {
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_space_ready() {
                cvar.notify_one();
                //eprintln!("W:PUSH_CONSUMED_DATA");
                return worker.push_consumed_data(data, m8);
            } else {
                //eprintln!("W:WAIT_PUSH_CONSUMED_DATA");
                let _ign = cvar.wait(worker);
            }
        }
    }
   #[inline(always)]
    fn push_eof(&mut self,
    ) -> DivansOutputResult {
        loop {
            let &(ref lock, ref cvar) = &*self.queue;
            let mut worker = lock.lock().unwrap();
            if worker.result_space_ready() {
                cvar.notify_one();
                //eprintln!("W:PUSH_EOF");
                return worker.push_eof();
            } else {
                //eprintln!("W:WAIT_PUSH_EOF");
                let _ign = cvar.wait(worker);
            }
        }
    }
}
