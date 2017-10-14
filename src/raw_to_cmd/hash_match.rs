use core;
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};


pub struct HashMatch<AllocU32:Allocator<u32> > {
    ht: AllocU32::AllocatedMemory,
}
impl<AllocU32:Allocator<u32> > HashMatch<AllocU32> {
    pub fn new(m32: &mut AllocU32) -> Self {
        HashMatch {
          ht:m32.alloc_cell(128),
        }
    }
    pub fn free(&mut self, m32: &mut AllocU32) {
       m32.free_cell(core::mem::replace(&mut self.ht, AllocU32::AllocatedMemory::default()));
    }
}

