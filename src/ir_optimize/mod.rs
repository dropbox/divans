use brotli;
use core;
use codec;
use super::mux::{Mux,DevNull};
use super::probability::CDF16;
use codec::io::DemuxerAndRingBuffer;
pub use super::cmd_to_divans::EncoderSpecialization;
use brotli::interface::{Command, CopyCommand, Nop, PredictionModeContextMap};
use alloc_util;
use alloc::{SliceWrapper, Allocator};
pub use super::interface::{ArithmeticEncoderOrDecoder, NewWithAllocator, DivansResult, ErrMsg};
mod statistics_tracking_codec;
use self::statistics_tracking_codec::{TallyingArithmeticEncoder, OneCommandThawingArray,
                                      total_billing_cost, take_billing_snapshot, billing_snapshot_delta,reset_billing_snapshot};
pub fn ir_optimize<SelectedCDF:CDF16,
                   ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                   AllocU8:Allocator<u8>,
                   AllocCDF16:Allocator<SelectedCDF>,
                   >(pm:&mut brotli::interface::PredictionModeContextMap<brotli::InputReferenceMut>,
                     a:&mut [brotli::interface::Command<brotli::SliceOffset>],
                     mb:brotli::InputPair,
                     codec:&mut codec::DivansCodec<ChosenEncoder,
                                                   EncoderSpecialization,
                                                   DemuxerAndRingBuffer<AllocU8,
                                                                        DevNull<AllocU8>>,
                                                   Mux<AllocU8>,
                                                   SelectedCDF,
                                                   AllocU8,
                                                   AllocCDF16>,
                     window_size: u8,
                     opt: super::interface::DivansCompressorOptions,
) -> Result<usize, ErrMsg> {
    let mut unused = 0usize;
    let mut unused2 = 0usize;
    if a.len() == 0 {
        return Ok(0);
    }
    let (re_m8, mcdf16, remainder) = match core::mem::replace(&mut codec.cross_command_state.thread_ctx, codec::ThreadContext::Worker) {
        codec::ThreadContext::MainThread(main) => main.dismantle(),
        codec::ThreadContext::Worker => panic!("Main Thread was none during encode"),
    };
    let (m8, cache) = re_m8.disassemble();
    let mut actuary = codec::DivansCodec::<TallyingArithmeticEncoder,
                                           EncoderSpecialization,
                                           DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>,
                                           DevNull<AllocU8>,
                                           SelectedCDF,
                                           AllocU8,
                                           AllocCDF16>::new(m8,
                                                            mcdf16,
                                                            TallyingArithmeticEncoder::default(),
                                                            TallyingArithmeticEncoder::default(),
                                                            EncoderSpecialization::new(),
                                                            DemuxerAndRingBuffer::<AllocU8, DevNull<AllocU8>>::default(),
                                                            usize::from(window_size),
                                                            opt.dynamic_context_mixing.unwrap_or(0),
                                                            opt.prior_depth,
                                                            opt.literal_adaptation,
                                                            opt.use_context_map,
                                                            opt.force_stride_value,
                                                            false);
    /*
    {
        let immutable_pm = Command::PredictionMode(PredictionModeContextMap::<brotli::InputReference>{
            literal_context_map:brotli::InputReference::from(&pm.literal_context_map),
            predmode_speed_and_distance_context_map:brotli::InputReference::from(&pm.predmode_speed_and_distance_context_map),
        });
        let mut cmd_offset = 0usize;
        match actuary.encode_or_decode(&[], &mut unused, &mut[], &mut unused2,
                                            &codec::CommandSliceArray(&[immutable_pm]),&mut cmd_offset) {
            DivansResult::NeedsMoreInput | DivansResult::NeedsMoreOutput => {
                return Err(ErrMsg::DrainOrFillNeedsInput(2));
            },
            DivansResult::Failure(e) => {
                return Err(e);
            }
            DivansResult::Success => {
                if cmd_offset != 1 {
                    return Err(ErrMsg::DrainOrFillNeedsInput(3));                    
                }
            }
        }
    }
    
    let mut eligible_index = 0usize;
    for index in 1..a.len() {
        let (eligible_a, item_a) = a.split_at_mut(index);
        let mut step_command = false;
        let mut eligible = eligible_a[eligible_index];
        if let Command::Literal(ref mut lit) = eligible {
            if let Command::Copy(ref mut copy) = item_a[0] {
                let start = lit.data.offset();
                let fin = start + lit.data.len() + copy.num_bytes as usize;
                let mut should_merge = false;
                if should_merge && !(start < mb.0.len() && fin > mb.0.len()) {
                    lit.data.1 += copy.num_bytes;
                    core::mem::replace(copy, CopyCommand::nop());
                } else {
                    step_command = true;
                }
            } else {
                step_command = true;
            }
        } else {
            step_command = true;
        }
        if step_command {
            eligible_index = index;
            let mut cmd_offset = 0usize;
            match actuary.encode_or_decode(&[], &mut unused, &mut[], &mut unused2,
                                           &OneCommandThawingArray(&eligible, &mb),&mut cmd_offset) {
                DivansResult::NeedsMoreInput | DivansResult::NeedsMoreOutput => {
                    return Err(ErrMsg::DrainOrFillNeedsInput(4));
                },
                DivansResult::Failure(e) => {
                    return Err(e);
                }
                DivansResult::Success => {
                    if cmd_offset != 1 {
                        return Err(ErrMsg::DrainOrFillNeedsInput(5));
                    }
                }
            }
        }
    }
    for index in eligible_index..a.len() {
        let mut cmd_offset = 0usize;
        let cmd = &a[index];
        match actuary.encode_or_decode(&[], &mut unused, &mut[], &mut unused2,
                                       &OneCommandThawingArray(&cmd, &mb),&mut cmd_offset) {
            DivansResult::NeedsMoreInput | DivansResult::NeedsMoreOutput => {
                return Err(ErrMsg::DrainOrFillNeedsInput(4));
            },
            DivansResult::Failure(e) => {
                return Err(e);
            }
            DivansResult::Success => {
                if cmd_offset != 1 {
                    return Err(ErrMsg::DrainOrFillNeedsInput(5));
                }
            }
        }        
    }
*/
    eprintln!("Actuary estimate: total cost {} bits; {} bytes\n", total_billing_cost(&actuary), total_billing_cost(&actuary)/ 8.0);
    let (retrieved_m8, retrieved_mcdf16) = actuary.free();
    codec.cross_command_state.thread_ctx = codec::ThreadContext::MainThread(
        codec::MainThreadContext::<SelectedCDF,
                                   AllocU8,
                                   AllocCDF16,
                                   ChosenEncoder>::reassemble((alloc_util::RepurposingAlloc::reassemble((retrieved_m8, cache)),
                                                               retrieved_mcdf16,
                                                               remainder)));
    Ok(a.len())
}
