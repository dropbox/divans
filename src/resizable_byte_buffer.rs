use core;
pub use super::alloc::{Allocator, SliceWrapper, SliceWrapperMut};


struct ResizableByteBuffer<T:Sized+Default, AllocT: Allocator<T>> {
    data: AllocT::AllocatedMemory,
    size: usize,
}

impl<T:Sized+Default+Clone, AllocT: Allocator<T>> ResizableByteBuffer<T, AllocT>{
    pub fn new() -> Self {
        ResizableByteBuffer::<T, AllocT> {
            data: AllocT::AllocatedMemory::default(),
            size: 0,
        }
    }
    fn bump_buffer(&mut self, allocator: &mut AllocT) {
        if self.data.slice().len() == 0 {
            self.data = allocator.alloc_cell(66000); // some slack room to deal with worst case compression sizes
        } else {
            if self.size == self.data.slice().len() {
                let mut cell = allocator.alloc_cell(self.size * 2);
                cell.slice_mut().split_at_mut(self.size).0.clone_from_slice(self.data.slice());
                allocator.free_cell(core::mem::replace(&mut self.data, cell));
            }
        }
    }
    pub fn checkout_next_buffer(&mut self, allocator: &mut AllocT) -> &mut [T] {
        self.bump_buffer(allocator);
        return self.data.slice_mut();
    }
    pub fn commit_next_buffer(&mut self, size:usize) {
        self.size += size;
    }
    pub fn len(&self) -> usize {
        self.size
    }
    pub fn slice(&self) -> &[T] {
        self.data.slice().split_at(self.size).0
    }
}
