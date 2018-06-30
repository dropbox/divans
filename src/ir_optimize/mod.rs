use brotli;
use core;
use codec;
use super::mux::{Mux,DevNull};
use super::probability::CDF16;
use codec::io::DemuxerAndRingBuffer;
pub use super::cmd_to_divans::EncoderSpecialization;
use brotli::interface::{Command, LiteralCommand, CopyCommand, Nop, PredictionModeContextMap, StaticCommand};
use alloc_util;
use alloc::{SliceWrapper, Allocator};
pub use super::interface::{ArithmeticEncoderOrDecoder, NewWithAllocator, DivansResult, ErrMsg};
mod statistics_tracking_codec;
mod cache;
use self::statistics_tracking_codec::{TallyingArithmeticEncoder, OneCommandThawingArray, TwoCommandThawingArray, ToggleProbabilityBlend,
                                      take_billing_snapshot, billing_snapshot_delta,reset_billing_snapshot};



// sometimes we can't simulate a future cache hit by putting a value N less into the cache
// because that value would make for an impossible (negative) recently cached distance
// instead we simply add the value--which is not a perfectly accurate representation, but likely
// follows a similar distribution in aggregate
fn sub_or_add(input: u32, val: u32) -> u32 {
    if input > val {
        input - val
    } else {
        input + val
    }
}

pub fn should_merge<SelectedCDF:CDF16,
                    AllocU8:Allocator<u8>,
                    AllocCDF16:Allocator<SelectedCDF>>(lit: &LiteralCommand<brotli::SliceOffset>,
                                                       copy: &CopyCommand,
                                                       copy_index: usize,
                                                       mb: brotli::InputPair,
                                                       actuary:&mut codec::DivansCodec<TallyingArithmeticEncoder,
                                                                                       ToggleProbabilityBlend,
                                                                                       DemuxerAndRingBuffer<AllocU8,
                                                                                                            DevNull<AllocU8>>,
                                                                                       DevNull<AllocU8>,
                                                                                       SelectedCDF,
                                                                                       AllocU8,
                                                                                       AllocCDF16>,
                                                       cache: &mut cache::Cache<AllocU8>) -> Result<bool, ErrMsg> {
    if lit.data.0 + lit.data.1 as usize + copy.num_bytes as usize > mb.0.orig_offset + mb.0.len() as usize && lit.data.0 < mb.0.orig_offset + mb.0.len() as usize {
        return Ok(false); // can't merge: would wrap the metablock
    }
    let codec_snapshot = actuary.cross_command_state.snapshot_literal_or_copy_state();
    actuary.cross_command_state.specialization.will_it_blend = false;

    // lets see if the copy would hit the distance_lru cache
    let code = actuary.cross_command_state.bk.distance_mnemonic_code(copy.distance, copy.num_bytes);
    //let mut future_hit = -i32::from(code);
    let entry = cache.get_cache_hit_log(copy_index);
    if !entry.miss() { // this copy actually populated the cache for a future copy... lets setup the cache as if this copy would be serviced by it
        if code == 15 { // this was a cache miss... lets see if this copy populates the cache for a future hit
            let entry_id = entry.entry_id();
            //future_hit = i32::from(entry_id);
            match entry_id {
                0 | 1 | 2 | 3 => actuary.cross_command_state.bk.distance_lru[entry_id as usize] = copy.distance,
                4 => actuary.cross_command_state.bk.distance_lru[0] = sub_or_add(copy.distance, 1),
                5 => actuary.cross_command_state.bk.distance_lru[0] = copy.distance + 1,
                6 => actuary.cross_command_state.bk.distance_lru[1] = sub_or_add(copy.distance, 1),
                7 => actuary.cross_command_state.bk.distance_lru[1] = copy.distance + 1,
                8 => actuary.cross_command_state.bk.distance_lru[0] = sub_or_add(copy.distance, 2),
                9 => actuary.cross_command_state.bk.distance_lru[0] = copy.distance + 2,
                10 => actuary.cross_command_state.bk.distance_lru[1] = sub_or_add(copy.distance, 2),
                11 => actuary.cross_command_state.bk.distance_lru[1] = copy.distance + 2,
                12 => actuary.cross_command_state.bk.distance_lru[0] = sub_or_add(copy.distance, 3),
                13 => actuary.cross_command_state.bk.distance_lru[0] = copy.distance + 3,
                14 => actuary.cross_command_state.bk.distance_lru[1] = sub_or_add(copy.distance, 3),
                _ => {},
            }
        }
    }
    


    let mut cmd_offset = 0usize;
    let mut unused = 0usize;
    let mut unused2 = 0usize;
    let mut combined_lit = lit.clone();
    combined_lit.data.1 += copy.num_bytes;
    take_billing_snapshot(actuary);
    match actuary.encode_or_decode(&[], &mut unused, &mut[], &mut unused2,
                                   &OneCommandThawingArray(&Command::Literal(combined_lit), &mb), &mut cmd_offset) {
        DivansResult::NeedsMoreOutput => {
            return Err(ErrMsg::DrainOrFillNeedsInput(6));
        },
        DivansResult::Failure(e) => {
            return Err(e);
        }
        DivansResult::NeedsMoreInput | DivansResult::Success => {
            if cmd_offset != 1 {
                return Err(ErrMsg::DrainOrFillNeedsInput(7));
            }
        }
    }
    let combined_cost = billing_snapshot_delta(actuary);
    reset_billing_snapshot(actuary);
    actuary.cross_command_state.restore_literal_or_copy_snapshot(codec_snapshot.clone());
    cmd_offset = 0;
    match actuary.encode_or_decode(&[], &mut unused, &mut[], &mut unused2,
                                   &TwoCommandThawingArray([&Command::Literal(*lit), &Command::Copy(*copy)], &mb), &mut cmd_offset) {
        DivansResult::NeedsMoreOutput => {
            return Err(ErrMsg::DrainOrFillNeedsInput(6));
        },
        DivansResult::Failure(e) => {
            return Err(e);
        }
        DivansResult::NeedsMoreInput | DivansResult::Success => {
            if cmd_offset != 2 {
                return Err(ErrMsg::DrainOrFillNeedsInput(7));
            }
        }
    }
    let cur_cost = billing_snapshot_delta(actuary);
    actuary.cross_command_state.specialization.will_it_blend = true;
    actuary.cross_command_state.restore_literal_or_copy_snapshot(codec_snapshot);
    reset_billing_snapshot(actuary);
    /*let full_cost = total_billing_cost(actuary);
    eprintln!("{}) At {} bits: Checking cost of Copy of d:{} l:{} (cur hit: {} future hit: {}) = {} vs combined longer literal at {}\n",
              copy_index, full_cost, copy.distance, copy.num_bytes, code, future_hit, cur_cost, combined_cost);*/
    Ok(combined_cost < cur_cost)
}
pub fn ir_optimize<'a, SelectedCDF:CDF16,
                   ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                   AllocU8:Allocator<u8>,
                   AllocCDF16:Allocator<SelectedCDF>,
                   AllocCommand: Allocator<StaticCommand>
                   >(pm:&mut brotli::interface::PredictionModeContextMap<brotli::InputReferenceMut>,
                     orig_buf:&'a mut [brotli::interface::Command<brotli::SliceOffset>],
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
                     _mc: &'a mut AllocCommand,
                     _buf: &'a mut AllocCommand::AllocatedMemory,
) -> Result<&'a [brotli::interface::Command<brotli::SliceOffset>], ErrMsg> {
    let mut unused = 0usize;
    let mut unused2 = 0usize;
    if orig_buf.len() == 0 {
        return Ok(orig_buf);
    }
    let (re_m8, mcdf16, remainder) = match core::mem::replace(&mut codec.cross_command_state.thread_ctx, codec::ThreadContext::Worker) {
        codec::ThreadContext::MainThread(main) => main.dismantle(),
        codec::ThreadContext::Worker => panic!("Main Thread was none during encode"),
    };
    let (mut m8, reallocation_item) = re_m8.disassemble();
    let mut distance_cache = cache::Cache::<AllocU8>::new(&codec.cross_command_state.bk.distance_lru, orig_buf.len(), &mut m8);
    let mut actuary = codec::DivansCodec::<TallyingArithmeticEncoder,
                                           ToggleProbabilityBlend,
                                           DemuxerAndRingBuffer<AllocU8, DevNull<AllocU8>>,
                                           DevNull<AllocU8>,
                                           SelectedCDF,
                                           AllocU8,
                                           AllocCDF16>::new(m8,
                                                            mcdf16,
                                                            TallyingArithmeticEncoder::default(),
                                                            TallyingArithmeticEncoder::default(),
                                                            ToggleProbabilityBlend::default(),
                                                            DemuxerAndRingBuffer::<AllocU8, DevNull<AllocU8>>::default(),
                                                            usize::from(window_size),
                                                            opt.dynamic_context_mixing.unwrap_or(0),
                                                            opt.prior_algorithm,
                                                            opt.literal_adaptation,
                                                            opt.use_context_map,
                                                            opt.force_stride_value,
                                                            false);
    {
        let immutable_pm = Command::PredictionMode(PredictionModeContextMap::<brotli::InputReference>{
            literal_context_map:brotli::InputReference::from(&pm.literal_context_map),
            predmode_speed_and_distance_context_map:brotli::InputReference::from(&pm.predmode_speed_and_distance_context_map),
        });
        let mut cmd_offset = 0usize;
        match actuary.encode_or_decode(&[], &mut unused, &mut[], &mut unused2,
                                            &codec::CommandSliceArray(&[immutable_pm]),&mut cmd_offset) {
            DivansResult::NeedsMoreOutput => {
                return Err(ErrMsg::DrainOrFillNeedsInput(2));
            },
            DivansResult::Failure(e) => {
                return Err(e);
            }
            DivansResult::NeedsMoreInput | DivansResult::Success => {
                if cmd_offset != 1 {
                    return Err(ErrMsg::DrainOrFillNeedsInput(3));                    
                }
            }
        }
    }
    for (index, cmd) in orig_buf.iter().enumerate() {
        if let Command::Copy(ref copy) = *cmd {
            distance_cache.populate(copy.distance, copy.num_bytes, index);
        }
    }
    let mut eligible_index = 0usize;
    for index in 1..orig_buf.len() {
        let (eligible_a, item_a) = orig_buf.split_at_mut(index);
        let mut step_command = false;
        let eligible = &mut eligible_a[eligible_index];
        if let Command::Literal(ref mut lit) = eligible {
            if let Command::Copy(ref mut copy) = item_a[0] {
                let start = lit.data.offset();
                let fin = start + lit.data.len() + copy.num_bytes as usize;
                let mut should_merge = match should_merge(lit, copy, index, mb, &mut actuary, &mut distance_cache) {
                    Ok(should) => should,
                    Err(msg) => return Err(msg),
                };
                if should_merge && !(start < mb.0.len() && fin > mb.0.len()) {
                    //eprintln!("Merging {},{} into {},{} with mb.0 {} and mb.1 {}", lit.data.0, lit.data.1, lit.data.0, lit.data.1 + copy.num_bytes,mb.0.len(), mb.1.len());
                    lit.data.1 += copy.num_bytes;
                    core::mem::replace(copy, CopyCommand::nop());
                } else {
                    step_command = true;
                }
            } else if let Command::Literal(cont_lit) = item_a[0] {
                let start = lit.data.offset();
                let fin = start + lit.data.len() + cont_lit.data.len();
                if start < mb.0.len() && fin > mb.0.len() {
                    step_command = true; // we span a macroblock boundary
                } else { // always merge adjacent literals if possible. There's rarely a benefit to keeping them apart
                    assert_eq!(lit.data.0 + lit.data.1 as usize, cont_lit.data.0);
                    //eprintln!("Merging {},{} into {},{} with mb.0 {} and mb.1 {}", lit.data.0, lit.data.1, lit.data.0, lit.data.1 + cont_lit.data.1,mb.0.len(), mb.1.len());

                    lit.data.1 += cont_lit.data.1;
                    core::mem::replace(&mut item_a[0], Command::Copy(CopyCommand::nop())); // replace with a copy
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
                DivansResult::NeedsMoreOutput => {
                    return Err(ErrMsg::DrainOrFillNeedsInput(4));
                },
                DivansResult::Failure(e) => {
                    return Err(e);
                }
                DivansResult::NeedsMoreInput | DivansResult::Success => {
                    if cmd_offset != 1 {
                        return Err(ErrMsg::DrainOrFillNeedsInput(5));
                    }
                }
            }
        }
    }
    for index in eligible_index..orig_buf.len() {
        let mut cmd_offset = 0usize;
        let cmd = &orig_buf[index];
        match actuary.encode_or_decode(&[], &mut unused, &mut[], &mut unused2,
                                       &OneCommandThawingArray(&cmd, &mb),&mut cmd_offset) {
            DivansResult::NeedsMoreOutput => {
                return Err(ErrMsg::DrainOrFillNeedsInput(4));
            },
            DivansResult::Failure(e) => {
                return Err(e);
            }
            DivansResult::NeedsMoreInput | DivansResult::Success => {
                if cmd_offset != 1 {
                    return Err(ErrMsg::DrainOrFillNeedsInput(5));
                }
            }
        }        
    }
    eligible_index = 0;
    for index in 0..orig_buf.len() {
        let cmd = orig_buf[index].clone();
        if let Command::Copy(ref copy) = cmd {
            if copy.num_bytes == 0 {
                continue;
            }
        }
        orig_buf[eligible_index] = cmd;
        eligible_index += 1;
    }
    //eprintln!("Actuary estimate: total cost {} bits; {} bytes\n", total_billing_cost(&actuary), total_billing_cost(&actuary)/ 8.0);
    let (mut retrieved_m8, retrieved_mcdf16) = actuary.free();
    distance_cache.free(&mut retrieved_m8);
    codec.cross_command_state.thread_ctx = codec::ThreadContext::MainThread(
        codec::MainThreadContext::<SelectedCDF,
                                   AllocU8,
                                   AllocCDF16,
                                   ChosenEncoder>::reassemble((alloc_util::RepurposingAlloc::reassemble((retrieved_m8, reallocation_item)),
                                                               retrieved_mcdf16,
                                                               remainder)));
    Ok(&orig_buf[..eligible_index])
}
