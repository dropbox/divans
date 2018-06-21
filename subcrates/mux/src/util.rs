use alloc::{Allocator, SliceWrapper, SliceWrapperMut};
use core;

pub struct AllocatedMemoryRange<T, AllocT: Allocator<T>> {
    pub mem: AllocT::AllocatedMemory,
    pub range: core::ops::Range<usize>,
}

impl<T, AllocT: Allocator<T>> Default for AllocatedMemoryRange<T, AllocT> {
    fn default() -> Self {
        AllocatedMemoryRange {
            mem: AllocT::AllocatedMemory::default(),
            range: 0..0,
        }
    }
}

impl<T, AllocT: Allocator<T>> core::ops::Index<usize> for AllocatedMemoryRange<T, AllocT> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        &self.mem.slice()[self.range.start + index]
    }
}

impl<T, AllocT: Allocator<T>> core::ops::IndexMut<usize> for AllocatedMemoryRange<T, AllocT> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        let i = self.range.start + index;
        &mut self.mem().slice_mut()[i]
    }
}

impl<T, AllocT: Allocator<T>> SliceWrapperMut<T> for AllocatedMemoryRange<T, AllocT> {
    fn slice_mut(&mut self) -> &mut [T] {
        &mut self.mem.slice_mut()[self.range.clone()]
    }
}

impl<T, AllocT: Allocator<T>> SliceWrapper<T> for AllocatedMemoryRange<T, AllocT> {
    fn slice(&self) -> &[T] {
        &self.mem.slice()[self.range.clone()]
    }
}

impl<T, AllocT: Allocator<T>> AllocatedMemoryRange<T, AllocT> {
    pub fn mem(&mut self) -> &mut AllocT::AllocatedMemory {
        &mut self.mem
    }
    pub fn components(self) -> (AllocT::AllocatedMemory, core::ops::Range<usize>) {
        (self.mem, self.range.clone())
    }

    pub fn new(alloc_u8: &mut AllocT, len: usize) -> Self {
        AllocatedMemoryRange::<T, AllocT> {
            mem: alloc_u8.alloc_cell(len),
            range: 0..len,
        }
    }
    pub fn realloc(mem: AllocT::AllocatedMemory, range: core::ops::Range<usize>) -> Self {
        debug_assert!(
            range.end <= mem.slice().len(),
            "Must realloc to a smaller size for AllocatedMemoryRange"
        );
        debug_assert!(range.start <= range.end);
        AllocatedMemoryRange::<T, AllocT> { mem, range }
    }
}
