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

