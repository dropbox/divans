use core;
use super::{SliceWrapperMut,SliceWrapper};
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

macro_rules! define_static_heap_buffer {
    ($name : tt, $size: expr) => {
        pub struct $name<T:Sized+Default+Copy>(Box<[T;$size]>);
        impl<T:Sized+Default+Copy> core::default::Default for $name<T> {
            fn default() -> Self {
                $name::<T>(Box::<[T;$size]>::new([T::default();$size]))
            }
        }
        impl<T:Sized+Default+Copy> SliceWrapper<T> for $name<T> {
            fn slice(&self) -> &[T] {
                &*self.0
            }
        }

        impl<T:Sized+Default+Copy> SliceWrapperMut<T> for $name<T> {
            fn slice_mut(&mut self) -> &mut [T] {
                &mut *self.0
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
