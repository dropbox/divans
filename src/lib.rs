extern crate alloc_no_stdlib as alloc;

pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};

#[cfg(not(feature="no-stdlib"))]
pub use alloc::HeapAlloc;
#[derive(Debug)]
pub struct CopyCommand {
    pub distance: usize,
    pub num_bytes: usize,
}

#[derive(Debug)]
pub struct DictCommand {
    pub word_size: u8,
    pub transform: u8,
    pub final_size: u8,
    pub _empty: u8,
    pub word_id: u32,
}

#[derive(Debug)]
pub struct LiteralCommand {
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub enum Command {
    Copy(CopyCommand),
    Dict(DictCommand),
    Literal(LiteralCommand),
}
