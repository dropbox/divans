#![cfg(test)]
extern crate std;
use std::vec::{
    Vec,
};
use std::boxed::{
    Box,
};
use core;
use alloc;
use alloc::{
    Allocator,
    SliceWrapperMut,
    SliceWrapper,
};

pub struct Rebox<T> {
    b: Box<[T]>,
}

impl<T> core::default::Default for Rebox<T> {
      fn default() -> Self {
          let v: Vec<T> = Vec::new();
          let b = v.into_boxed_slice();
          Rebox::<T> { b: b }
      }
}

impl<T> core::ops::Index<usize> for Rebox<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        &(*self.b)[index]
    }
}

impl<T> core::ops::IndexMut<usize> for Rebox<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        &mut (*self.b)[index]
    }
}

impl<T> alloc::SliceWrapper<T> for Rebox<T> {
    fn slice(&self) -> &[T] {
        &*self.b
    }
}

impl<T> alloc::SliceWrapperMut<T> for Rebox<T> {
    fn slice_mut(&mut self) -> &mut [T] {
        &mut *self.b
    }
}

pub struct HeapAllocator<T: core::clone::Clone> {
    pub default_value: T,
}

impl<T: core::clone::Clone> alloc::Allocator<T> for HeapAllocator<T> {
    type AllocatedMemory = Rebox<T>;
    fn alloc_cell(self: &mut HeapAllocator<T>, len: usize) -> Rebox<T> {
        let v: Vec<T> = vec![self.default_value.clone();len];
        let b = v.into_boxed_slice();
        Rebox::<T> { b: b }
    }
    fn free_cell(self: &mut HeapAllocator<T>, _data: Rebox<T>) {}
}
