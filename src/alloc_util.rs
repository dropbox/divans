// Copyright 2017 Dropbox, Inc
//
//   Licensed under the Apache License, Version 2.0 (the "License");
//   you may not use this file except in compliance with the License.
//   You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
//   Unless required by applicable law or agreed to in writing, software
//   distributed under the License is distributed on an "AS IS" BASIS,
//   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//   See the License for the specific language governing permissions and
//   limitations under the License.
use core;
pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};
pub use super::slice_util::AllocatedMemoryPrefix;

/// This struct can be configured in a mode where a single allocation is reused for ever larger allocations
pub struct RepurposingAlloc<T, AllocT: Allocator<T>> {
    alloc: AllocT,
    cached_allocation: AllocT::AllocatedMemory,
}
/*
pub struct LimitedAllocatedMemory32<T, AllocT:Allocator<T>> {
    mem: AllocT::AllocatedMemory,
    size: IndexType,
}
impl<T, AllocT:Allocator<T> > Default for LimitedAllocatedMemory32<T, AllocT> {
    fn default() -> Self {
       LimitedAllocatedMemory32::<T, AllocT> {
          mem: AllocT::AllocatedMemory::default(),
          size:0,
       }
    }
}
impl<T, AllocT:Allocator<T>> SliceWrapper<T> for LimitedAllocatedMemory32<T, AllocT>{
    fn slice(&self) -> &[T] {
        self.mem.slice().split_at(u64::from(self.size) as usize).0
    }
}


impl<T, AllocT:Allocator<T> > SliceWrapperMut<T> for LimitedAllocatedMemory32<T, AllocT> {
    fn slice_mut (&mut self) -> &mut [T] {
        self.mem.slice_mut().split_at_mut(self.size as usize).0
    }
}

impl<T, AllocT:Allocator<T> > core::ops::Index<usize> for LimitedAllocatedMemory32<T, AllocT> {
   type Output = T;
   fn index(&self, index: usize) -> &T {
      &self.mem.slice()[index]
   }
}
impl<T, AllocT:Allocator<T> > core::ops::IndexMut<usize> for LimitedAllocatedMemory32<T, AllocT> {
   fn index_mut(&mut self, index: usize) -> &mut T {
      &mut self.mem.slice_mut()[index]
   }
}
*/

pub trait ShouldClearCacheOnAlloc<T> {
    fn should_clear() -> Option<T>;
}

#[derive(Default)]
pub struct ClearCacheOnAlloc<T> {
    marker: core::marker::PhantomData<T>,
}

impl<T: Default> ShouldClearCacheOnAlloc<T> for ClearCacheOnAlloc<T> {
    fn should_clear() -> Option<T> {
        Some(T::default())
    }
}

pub struct UninitializedOnAlloc {}

impl<T> ShouldClearCacheOnAlloc<T> for UninitializedOnAlloc {
    fn should_clear() -> Option<T> {
        None
    }
}

pub struct CachedAllocator<'a, T: 'a, AllocT: 'a + Allocator<T>, ShouldClear: ShouldClearCacheOnAlloc<T>> {
    alloc: &'a mut RepurposingAlloc<T, AllocT>,
    marker: core::marker::PhantomData<ShouldClear>,
}

impl<'a, T: 'a, AllocT: 'a + Allocator<T>, ShouldClear: ShouldClearCacheOnAlloc<T>> Allocator<T> for CachedAllocator<'a, T, AllocT, ShouldClear> {
    type AllocatedMemory = AllocatedMemoryPrefix<T, AllocT>;
    fn alloc_cell(&mut self, s: usize) -> AllocatedMemoryPrefix<T, AllocT> {
        // this saves in practice about 3 ms per megabyte for a typical file
        if self.alloc.cached_allocation.slice().len() >= s {
            let mut retval = core::mem::replace(
                &mut self.alloc.cached_allocation,
                AllocT::AllocatedMemory::default(),
            );
            if ShouldClear::should_clear().is_some() {
                for item in retval.slice_mut().iter_mut() {
                    *item = ShouldClear::should_clear().unwrap();
                }
            }
            return AllocatedMemoryPrefix::<T, AllocT>::realloc(retval, s);
        }
        AllocatedMemoryPrefix::<T, AllocT>::new(&mut self.alloc.get_base_alloc(), s)
    }
    fn free_cell(&mut self, mut cell: AllocatedMemoryPrefix<T, AllocT>) {
        if cell.mem().slice().len() > self.alloc.cached_allocation.slice().len() {
            self.alloc.alloc.free_cell(core::mem::replace(&mut self.alloc.cached_allocation, cell.components().0))
        } else {
            self.alloc.alloc.free_cell(cell.components().0)
        }
    }
}

impl<T, AllocT: Allocator<T>> RepurposingAlloc<T, AllocT> {
    pub fn new(alloc: AllocT) -> Self {
        Self {
            alloc: alloc,
            cached_allocation: AllocT::AllocatedMemory::default(),
        }
    }
    pub fn use_cached_allocation<'a, ClearCacheDecision: ShouldClearCacheOnAlloc<T>>(
        &'a mut self,
    ) -> CachedAllocator<'a, T, AllocT, ClearCacheDecision> {
        CachedAllocator::<T, AllocT, ClearCacheDecision> {
            alloc: self,
            marker: core::marker::PhantomData::<ClearCacheDecision>::default(),
        }
    }
    pub fn get_base_alloc(&mut self) -> &mut AllocT {
        &mut self.alloc
    }
    pub fn free_ref(&mut self) {
        self.alloc.free_cell(core::mem::replace(&mut self.cached_allocation, AllocT::AllocatedMemory::default()));
    }
    pub fn free(mut self) -> AllocT {
        self.free_ref();
        self.alloc
    }
}

impl<T, AllocT: Allocator<T>> Allocator<T> for RepurposingAlloc<T, AllocT> {
    type AllocatedMemory = AllocT::AllocatedMemory;
    fn alloc_cell(&mut self, size: usize) -> Self::AllocatedMemory {
        self.alloc.alloc_cell(size)
    }
    fn free_cell(&mut self, bv: Self::AllocatedMemory) {
        self.alloc.free_cell(bv)
    }
}

#[cfg(test)]
#[cfg(not(feature="no-stdlib"))]
mod test {
    use core;
    use alloc::HeapAlloc;
    use super::{Allocator, AllocatedMemoryPrefix, RepurposingAlloc, UninitializedOnAlloc};
    struct LoggedAllocator<T, AllocT: Allocator<T>> {
        alloc: AllocT,
        count_alloc_cell: usize,
        count_free_cell: usize,
        marker: core::marker::PhantomData<T>,
    }
    impl<T, AllocT: Allocator<T>> LoggedAllocator<T, AllocT> {
        fn new(alloc: AllocT) -> Self {
            Self {
                alloc: alloc,
                count_alloc_cell: 0,
                count_free_cell: 0,
                marker: core::marker::PhantomData::<T>::default(),
            }
        }
    }
    impl<T, AllocT: Allocator<T>> Allocator<T> for LoggedAllocator<T, AllocT> {
        type AllocatedMemory = AllocT::AllocatedMemory;
        fn alloc_cell(&mut self, size: usize) -> Self::AllocatedMemory {
            self.count_alloc_cell += 1;
            self.alloc.alloc_cell(size)
        }
        fn free_cell(&mut self, bv: Self::AllocatedMemory) {
            self.count_free_cell += 1;
            self.alloc.free_cell(bv)
        }
    }

    #[test]
    fn test_non_reuse() {
        let base_alloc = LoggedAllocator::<u8, HeapAlloc<u8>>::new(HeapAlloc::<u8>::new(0u8));
        let mut alloc = RepurposingAlloc::<u8, LoggedAllocator<u8, HeapAlloc<u8>>>::new(base_alloc);
        let mut cells = [alloc.alloc_cell(100), alloc.alloc_cell(200), alloc.alloc_cell(300)];
        assert_eq!(alloc.get_base_alloc().count_alloc_cell, cells.len());

        type AllocatedMemory = <RepurposingAlloc<u8, LoggedAllocator<u8, HeapAlloc<u8>>>
                                as Allocator<u8>>::AllocatedMemory;

        for c in cells.iter_mut() {
            alloc.free_cell(core::mem::replace(c, AllocatedMemory::default()));
        }
        assert_eq!(alloc.get_base_alloc().count_free_cell, cells.len());
        alloc.free();
    }

    #[test]
    fn test_reuse() {
        let base_alloc = LoggedAllocator::<u8, HeapAlloc<u8>>::new(HeapAlloc::<u8>::new(0u8));
        let mut alloc = RepurposingAlloc::<u8, LoggedAllocator<u8, HeapAlloc<u8>>>::new(base_alloc);
        let mut cached_alloc = alloc.use_cached_allocation::<UninitializedOnAlloc>();

        type AllocatedMemory = AllocatedMemoryPrefix<u8, LoggedAllocator<u8, HeapAlloc<u8>>>;

        // Allocate two cells and free one.
        let mut bv1 = cached_alloc.alloc_cell(100);
        let mut bv2 = cached_alloc.alloc_cell(110);
        let mut bv3 = cached_alloc.alloc_cell(120);
        cached_alloc.free_cell(core::mem::replace(&mut bv1, AllocatedMemory::default()));
        cached_alloc.free_cell(core::mem::replace(&mut bv2, AllocatedMemory::default()));
        assert_eq!(cached_alloc.alloc.get_base_alloc().count_alloc_cell, 3);
        assert_eq!(cached_alloc.alloc.get_base_alloc().count_free_cell, 2);

        // Allocate a new one that should trigger reuse.
        let mut bv4 = cached_alloc.alloc_cell(105);
        assert_eq!(cached_alloc.alloc.get_base_alloc().count_alloc_cell, 3);
        assert_eq!(cached_alloc.alloc.get_base_alloc().count_free_cell, 2);

        // Allocate a new one that shouldn't trigger reuse.
        cached_alloc.free_cell(core::mem::replace(&mut bv3, AllocatedMemory::default()));
        let mut bv5 = cached_alloc.alloc_cell(130);
        assert_eq!(cached_alloc.alloc.get_base_alloc().count_alloc_cell, 4);
        assert_eq!(cached_alloc.alloc.get_base_alloc().count_free_cell, 3);

        cached_alloc.free_cell(core::mem::replace(&mut bv4, AllocatedMemory::default()));
        cached_alloc.free_cell(core::mem::replace(&mut bv5, AllocatedMemory::default()));
        assert_eq!(cached_alloc.alloc.get_base_alloc().count_alloc_cell, 4);
        assert_eq!(cached_alloc.alloc.get_base_alloc().count_free_cell, 5);
    }
}
