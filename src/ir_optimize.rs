use brotli;
use core;
use brotli::interface::{Command, CopyCommand, Nop};
use alloc::SliceWrapper;

pub fn ir_optimize(_pm:&mut brotli::interface::PredictionModeContextMap<brotli::InputReferenceMut>,
                   a:&mut [brotli::interface::Command<brotli::SliceOffset>],
                   mb:brotli::InputPair) -> usize {
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
