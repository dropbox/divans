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
use super::slice_util::AllocatedMemoryPrefix;

/// This struct can be configured in a mode where a single allocation is reused for ever larger allocations
pub struct RepurposingAlloc<T, AllocT:Allocator<T>> {
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
   fn should_clear(&self) -> Option<T>;
}

pub struct ClearCacheOnAlloc<T> {
   marker: core::marker::PhantomData<T>,
}
impl<T> Default for ClearCacheOnAlloc<T> {
   fn default() -> Self {
      ClearCacheOnAlloc::<T> {
         marker:core::marker::PhantomData::<T>::default()
      }
   }
}

impl<T:Default> ShouldClearCacheOnAlloc<T> for ClearCacheOnAlloc<T> {
   fn should_clear(&self) -> Option<T> {
      Some(T::default())
   }
}
pub struct UninitializedOnAlloc {}
impl<T> ShouldClearCacheOnAlloc<T> for UninitializedOnAlloc {
   fn should_clear(&self) -> Option<T> {
     None
   }
}


pub struct CachedAllocator<'a, T:'a, AllocT:'a+Allocator<T>, ShouldClear:ShouldClearCacheOnAlloc<T>> {
    alloc: &'a mut RepurposingAlloc<T, AllocT>,
    clear_on_alloc: ShouldClear,
}
impl<'a, T:'a, AllocT:'a+Allocator<T>, ShouldClear:ShouldClearCacheOnAlloc<T> > Allocator<T> for CachedAllocator<'a, T, AllocT, ShouldClear> {
   type AllocatedMemory = AllocatedMemoryPrefix<T, AllocT>;
   fn alloc_cell(&mut self, s: usize) -> AllocatedMemoryPrefix<T, AllocT> {
       if self.alloc.cached_allocation.slice().len() >= s {
           let mut retval = core::mem::replace(&mut self.alloc.cached_allocation, AllocT::AllocatedMemory::default());
           if self.clear_on_alloc.should_clear().is_some() {
               for item in retval.slice_mut().iter_mut() {
                  *item = self.clear_on_alloc.should_clear().unwrap();
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



impl<T, AllocT:Allocator<T> > RepurposingAlloc<T, AllocT> {
  pub fn new(alloc:AllocT) -> Self {
     Self {
       alloc:alloc,
       cached_allocation:AllocT::AllocatedMemory::default(),
     }
  }
  pub fn use_cached_allocation<'a, ClearCacheDecision:ShouldClearCacheOnAlloc<T>> (&'a mut self, clear_on_alloc: ClearCacheDecision) -> CachedAllocator<'a, T, AllocT, ClearCacheDecision> {
      CachedAllocator::<T, AllocT, ClearCacheDecision>{
          alloc: self,
          clear_on_alloc: clear_on_alloc,
      }
  }
  pub fn get_base_alloc(&mut self) -> &mut AllocT {
      &mut self.alloc
  }
  pub fn free(mut self) -> AllocT {
      self.alloc.free_cell(self.cached_allocation);
      self.alloc
  }
  
}

impl<T, AllocT:Allocator<T> > Allocator<T> for RepurposingAlloc<T, AllocT> {
    type AllocatedMemory = AllocT::AllocatedMemory;
    fn alloc_cell(&mut self, size:usize) -> Self::AllocatedMemory {
        self.alloc.alloc_cell(size)
    }
    fn free_cell(&mut self, bv:Self::AllocatedMemory) {
        self.alloc.free_cell(bv)
    }
}
