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
use std::io;
use super::{SliceWrapperMut,SliceWrapper};
use super::alloc;
pub struct DynBuffer(Box<[u8]>);

impl core::default::Default for DynBuffer {
  fn default() -> Self {
    let v: Vec<u8> = Vec::new();
    let b = v.into_boxed_slice();
    DynBuffer(b)
  }
}


impl DynBuffer {
    #[allow(unused)]
    pub fn new(size:usize) -> DynBuffer {
        DynBuffer(vec![0u8;size].into_boxed_slice())
    }
}

impl SliceWrapper<u8> for DynBuffer {
  fn slice(&self) -> &[u8] {
    &*self.0
  }
}

impl SliceWrapperMut<u8> for DynBuffer {
  fn slice_mut(&mut self) -> &mut [u8] {
    &mut *self.0
  }
}

#[cfg(feature="inplace-new")]
macro_rules! define_static_heap_buffer {
    ($name : ident, $size: expr) => {
        pub struct $name(Box<[u8;$size]>);
        impl core::default::Default for $name {
            fn default() -> Self {
                static DEFAULT_VALUE: [u8;$size] = [0u8;$size];
                $name(Box::<[u8;$size]>::new(DEFAULT_VALUE))
            }
        }
        impl SliceWrapper<u8> for $name {
            fn slice(&self) -> &[u8] {
                &*self.0
            }
        }

        impl SliceWrapperMut<u8> for $name {
            fn slice_mut(&mut self) -> &mut [u8] {
                &mut *self.0
            }
        }
    }
}

#[cfg(not(feature="inplace-new"))]
macro_rules! define_static_heap_buffer {
    ($name : ident, $size: expr) => {
        pub struct $name(DynBuffer);
        impl core::default::Default for $name {
            fn default() -> Self {
                $name(DynBuffer((vec![0u8;$size]).into_boxed_slice()))
            }
        }
        impl SliceWrapper<u8> for $name {
            fn slice(&self) -> &[u8] {
                (&*(self.0).0).split_at($size).0
            }
        }

        impl SliceWrapperMut<u8> for $name {
            fn slice_mut(&mut self) -> &mut [u8] {
                (&mut *(self.0).0).split_at_mut($size).0
            }
        }
    }
}

define_static_heap_buffer!(StaticHeapBuffer10, 1<<10);
define_static_heap_buffer!(StaticHeapBuffer11, 1<<11);
define_static_heap_buffer!(StaticHeapBuffer12, 1<<12);
define_static_heap_buffer!(StaticHeapBuffer13, 1<<13);
define_static_heap_buffer!(StaticHeapBuffer14, 1<<14);
define_static_heap_buffer!(StaticHeapBuffer15, 1<<15);
define_static_heap_buffer!(StaticHeapBuffer16, 1<<16);
define_static_heap_buffer!(StaticHeapBuffer17, 1<<17);
define_static_heap_buffer!(StaticHeapBuffer18, 1<<18);
define_static_heap_buffer!(StaticHeapBuffer19, 1<<19);
define_static_heap_buffer!(StaticHeapBuffer20, 1<<20);
define_static_heap_buffer!(StaticHeapBuffer21, 1<<21);
define_static_heap_buffer!(StaticHeapBuffer22, 1<<22);
define_static_heap_buffer!(StaticHeapBuffer23, 1<<23);
define_static_heap_buffer!(StaticHeapBuffer24, 1<<24);


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



fn hex_to_nibble(byte: u8) -> Result<u8, ()> {
    if byte >= b'A' && byte <= b'F' {
        Ok(byte - b'A' + 10)
    } else if byte >= b'a' && byte <= b'f' {
        Ok(byte - b'a' + 10)
    } else if byte >= b'0' && byte <= b'9' {
        Ok(byte - b'0')
    } else {
        Err(())
    }
}
fn quoted_slice_to_vec(s: &[u8]) -> Result<Vec<u8>, io::Error> {
    if s.len() < 2 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, core::str::from_utf8(s).unwrap()));
    }
    let mut output = Vec::<u8>::with_capacity(s.len() - 2);
    let mut must_end = false;
    let mut escaped = false;
    let mut hexed = false;
    let mut upper: Option<u8> = None;
    
    for byte_ref in s.iter().skip(1) {
        let byte = *byte_ref;
        if must_end {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, core::str::from_utf8(s).unwrap())); 
        }
        if byte ==  b'\"' && !escaped {
            must_end = true;
            continue;
        }
        
        if byte == b'\\' && !escaped {
            escaped = true;
            continue;
        }
        if escaped {
            if hexed {
                if let Ok(nib) = hex_to_nibble(byte) {
                    if let Some(unib) = upper {
                        output.push((unib << 4) | nib);
                        hexed = false;
                        escaped = false;
                        upper = None;
                    } else {
                        upper = Some(nib);
                    }
                } else {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, core::str::from_utf8(s).unwrap())); 
                }
            } else if byte == b'x' {
                hexed = true;
            } else if byte == b'n' {
                output.push(b'\n');
                escaped = false;
            } else if byte == b'r' {
                output.push(b'\r');
                escaped = false;
            } else if byte == b't' {
                output.push(b'\t');
                escaped = false;
            } else if byte == b'\\' {
                output.push(b'\\');
                escaped = false;
            } else if byte == b'\'' {
                output.push(b'\'');
                escaped = false;
            } else if byte == b'\"' {
                output.push(b'\"');
                escaped = false;
            } else if byte == b'?' {
                output.push(b'?');
                escaped = false;
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, core::str::from_utf8(s).unwrap()));
            }
        } else {
            output.push(byte);
        }
    }
    if hexed || escaped || !must_end {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, core::str::from_utf8(s).unwrap()));
    }
    return Ok(output);
}

pub fn literal_slice_to_vec(s: &[u8]) -> Result<Vec<u8>, io::Error> {
    if s.len() == 0 {
        return Ok(Vec::<u8>::new());
    }
    if *s.iter().next().unwrap() == b'\"' {
        quoted_slice_to_vec(s)
    } else {
        hex_slice_to_vec(s)
    }
}
pub fn hex_slice_to_vec(s: &[u8]) -> Result<Vec<u8>, io::Error> {
    let mut output = Vec::with_capacity(s.len() >> 1);
    let mut rem = 0;
    let mut buf : u8 = 0;
    for byte_ref in s.iter() {
        let byte = *byte_ref;
        if let Ok(b) = hex_to_nibble(byte) {
            buf <<= 4;
            buf |= b;
        } else if byte == b'\n'|| byte == b'\t'|| byte == b'\r' {
            continue;
        } else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, core::str::from_utf8(s).unwrap()));
        }
        rem += 1;
        if rem == 2 {
            rem = 0;
            output.push(buf);
        }
    }
    if rem != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                  "String must have an even number of digits"));
    }
    Ok(output)
}
