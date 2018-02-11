use super::alloc::{SliceWrapper, SliceWrapperMut};

pub trait CryptoSigTrait : SliceWrapper<u8>+SliceWrapperMut<u8>+Copy+Clone+Default{
    const SIZE:usize;
}


macro_rules! define_fixed_buffer {
    ($name: tt, $size: expr) => {
        #[derive(Clone, Copy, Default)]
        pub struct $name(pub [u8;$size]);
        impl SliceWrapper<u8> for $name {
            fn slice(&self) -> &[u8] {
                &self.0[..]
            }
        }
        impl SliceWrapperMut<u8> for $name {
            fn slice_mut(&mut self) -> &mut [u8] {
                &mut self.0[..]
            }
        }
        impl CryptoSigTrait for $name {
            const SIZE: usize = $size;
        }
    };
}

define_fixed_buffer!(FixedBuffer1, 1);
define_fixed_buffer!(FixedBuffer2, 2);
define_fixed_buffer!(FixedBuffer3, 3);
define_fixed_buffer!(FixedBuffer4, 4);
define_fixed_buffer!(FixedBuffer5, 5);
define_fixed_buffer!(FixedBuffer6, 6);
define_fixed_buffer!(FixedBuffer7, 7);
define_fixed_buffer!(FixedBuffer8, 8);
define_fixed_buffer!(FixedBuffer12, 12);
define_fixed_buffer!(FixedBuffer16, 16);
define_fixed_buffer!(FixedBuffer24, 24);
define_fixed_buffer!(FixedBuffer32, 32);
