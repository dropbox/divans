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

pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};

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


