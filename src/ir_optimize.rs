use brotli;
use core;
use codec;
use super::mux::{Mux,DevNull};
use super::probability::CDF16;
use codec::io::DemuxerAndRingBuffer;
pub use super::cmd_to_divans::EncoderSpecialization;
use brotli::interface::{Command, CopyCommand, Nop};
use alloc::{SliceWrapper, Allocator};
pub use super::interface::{ArithmeticEncoderOrDecoder, NewWithAllocator};

pub fn ir_optimize<SelectedCDF:CDF16,
                   ChosenEncoder: ArithmeticEncoderOrDecoder + NewWithAllocator<AllocU8>,
                   AllocU8:Allocator<u8>,
                   AllocCDF16:Allocator<SelectedCDF>,
                   >(_pm:&mut brotli::interface::PredictionModeContextMap<brotli::InputReferenceMut>,
                     a:&mut [brotli::interface::Command<brotli::SliceOffset>],
                     mb:brotli::InputPair,
                     _codec:&mut codec::DivansCodec<ChosenEncoder,
                                                   EncoderSpecialization,
                                                   DemuxerAndRingBuffer<AllocU8,
                                                                        DevNull<AllocU8>>,
                                                   Mux<AllocU8>,
                                                   SelectedCDF,
                                                   AllocU8,
                                                   AllocCDF16>) -> usize {
    if a.len() == 0 {
        return 0;
    }
    let mut eligible_index = 0usize;
    for index in 1..a.len() {
        let (eligible_a, item_a) = a.split_at_mut(index);
        if let Command::Literal(ref mut lit) = eligible_a[eligible_index] {
            if let Command::Copy(ref mut copy) = item_a[0] {
                let start = lit.data.offset();
                let fin = start + lit.data.len() + copy.num_bytes as usize;
                let mut should_merge = false;
                if should_merge && !(start < mb.0.len() && fin > mb.0.len()) {
                    lit.data.1 += copy.num_bytes;
                    core::mem::replace(copy, CopyCommand::nop());
                } else {
                    eligible_index = index;
                }
            } else {
                eligible_index = index;
            }
        } else {
            eligible_index = index;
        }
        
    }
    a.len()
}
