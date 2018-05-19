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

#[derive(Copy,Clone,Default,Debug)]
pub struct SlicePlaceholder32<T> {
    len:u32,
    ph: core::marker::PhantomData<T>,
}
impl<T> SlicePlaceholder32<T> {
    pub fn new(len: u32) -> Self {
        SlicePlaceholder32::<T>{
            len: len,
            ph: core::marker::PhantomData::<T>::default(),
        }
    }
}

impl<T> SliceWrapper<T> for SlicePlaceholder32<T> {
    fn slice(&self) -> &[T]{
        &[]
    }
    fn len(&self) -> usize {
        self.len as usize
    }
}





#[derive(Copy,Clone)]
pub struct SliceReference<'a, T:'a> {
    data: &'a[T],
    start: usize,
    len: usize,
}

impl<'a, T:'a> SliceReference<'a, T> {
    pub fn new(input: &'a[T], start: usize, len: usize) -> SliceReference<'a, T> {
        SliceReference::<T> {
            data: input.split_at(start).1.split_at(len).0,
            start: start,
            len: len,
        }
    }
    pub fn freeze_dry(&self) -> SliceReference<'static, T> {
        SliceReference::<T> {
            data: &[],
            start: self.start,
            len: self.len,
        }        
    }
    pub fn thaw(&self, slice:&'a [T]) -> SliceReference<'a, T> {
        SliceReference::<'a, T> {
            data: slice.split_at(self.start).1.split_at(self.len).0,
            start: self.start,
            len: self.len,
        }        
    }
}

impl<'a, T:'a> SliceWrapper<T> for SliceReference<'a, T> {
    fn slice(&self) -> &[T]{
        self.data
    }
}

impl<'a, T> Default for SliceReference<'a, T> {
    fn default() ->SliceReference<'a, T> {
        SliceReference::<T> {
            data:&[],
            start:0,
            len:0,
        }
    }
}

pub struct AllocatedMemoryPrefix<T, AllocT:Allocator<T>>(pub AllocT::AllocatedMemory, pub usize);

impl<T, AllocT: Allocator<T>> core::ops::Index<usize> for AllocatedMemoryPrefix<T, AllocT> {
   type Output = T;
   fn index(&self, index: usize) -> &T {
      &self.0.slice()[index]
   }
}

impl<T, AllocT: Allocator<T>> core::ops::IndexMut<usize> for AllocatedMemoryPrefix<T, AllocT> {
   fn index_mut(&mut self, index: usize) -> &mut T {
      &mut self.mem().slice_mut()[index]
   }
}

impl<T, AllocT:Allocator<T>> Default for AllocatedMemoryPrefix<T, AllocT> {
    fn default() -> Self {
        AllocatedMemoryPrefix(AllocT::AllocatedMemory::default(), 0usize)
    }
}
impl<T, AllocT:Allocator<T>> AllocatedMemoryPrefix<T, AllocT> {
    pub fn mem(&mut self) -> &mut AllocT::AllocatedMemory {
        &mut self.0
    }
    pub fn components(self) -> (AllocT::AllocatedMemory, usize) {
        (self.0, self.1)
    }
}

impl<T, AllocT:Allocator<T>> SliceWrapperMut<T> for AllocatedMemoryPrefix<T, AllocT> {
    fn slice_mut(&mut self) -> &mut [T] {
        self.0.slice_mut().split_at_mut(self.1).0
    }
}
impl<T, AllocT:Allocator<T>> SliceWrapper<T> for AllocatedMemoryPrefix<T, AllocT> {
    fn slice(&self) -> &[T] {
        self.0.slice().split_at(self.1).0
    }
    fn len(&self) -> usize {
        self.1
    }
}
impl <T, AllocT:Allocator<T>> AllocatedMemoryPrefix<T, AllocT> {
    pub fn new(m8 : &mut AllocT, len: usize) -> Self {
        AllocatedMemoryPrefix::<T, AllocT>(m8.alloc_cell(len), len)
    }
    pub fn realloc(mem : AllocT::AllocatedMemory, len: usize) -> Self {
        debug_assert!(len <= mem.slice().len(), "Must realloc to a smaller size for AllocatedMemoryPrefix");
        AllocatedMemoryPrefix::<T, AllocT>(mem, len)
    }
}



pub struct AllocatedMemoryRange<T, AllocT:Allocator<T>>(pub AllocT::AllocatedMemory, pub core::ops::Range<usize>);

impl<T, AllocT: Allocator<T>> core::ops::Index<usize> for AllocatedMemoryRange<T, AllocT> {
   type Output = T;
   fn index(&self, index: usize) -> &T {
      &self.0.slice()[self.1.start + index]
   }
}

impl<T, AllocT: Allocator<T>> core::ops::IndexMut<usize> for AllocatedMemoryRange<T, AllocT> {
   fn index_mut(&mut self, index: usize) -> &mut T {
      let i = self.1.start + index;
      &mut self.mem().slice_mut()[i]
   }
}

impl<T, AllocT:Allocator<T>> Default for AllocatedMemoryRange<T, AllocT> {
    fn default() -> Self {
        AllocatedMemoryRange(AllocT::AllocatedMemory::default(), 0..0)
    }
}
impl<T, AllocT:Allocator<T>> AllocatedMemoryRange<T, AllocT> {
    pub fn mem(&mut self) -> &mut AllocT::AllocatedMemory {
        &mut self.0
    }
    pub fn components(self) -> (AllocT::AllocatedMemory, core::ops::Range<usize>) {
        (self.0, self.1.clone())
    }
}

impl<T, AllocT:Allocator<T>> SliceWrapperMut<T> for AllocatedMemoryRange<T, AllocT> {
    fn slice_mut(&mut self) -> &mut [T] {
        &mut self.0.slice_mut()[self.1.clone()]
    }
}
impl<T, AllocT:Allocator<T>> SliceWrapper<T> for AllocatedMemoryRange<T, AllocT> {
    fn slice(&self) -> &[T] {
        &self.0.slice()[self.1.clone()]
    }
}
impl <T, AllocT:Allocator<T>> AllocatedMemoryRange<T, AllocT> {
    pub fn new(m8 : &mut AllocT, len: usize) -> Self {
        AllocatedMemoryRange::<T, AllocT>(m8.alloc_cell(len), 0..len)
    }
    pub fn realloc(mem : AllocT::AllocatedMemory, range: core::ops::Range<usize>) -> Self {
        debug_assert!(range.end <= mem.slice().len(), "Must realloc to a smaller size for AllocatedMemoryRange");
        debug_assert!(range.start <= range.end);
        AllocatedMemoryRange::<T, AllocT>(mem, range)
    }
}



