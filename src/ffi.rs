#![cfg(not(feature="no-stdlib"))]
#![cfg(not(feature="safe"))]
#[no_mangle]
use core;
use brotli;
use std::vec::Vec;
use std::boxed::Box;
use std::os::raw::c_void;
use super::alloc;
use super::DivansCompressorFactory;
pub extern "C" fn hello_rust() -> *const u8 {
    "Hello, world!\0".as_ptr()
}

#[derive(Debug)]
pub struct MemoryBlock<Ty:Sized+Default>(Vec<Ty>);
impl<Ty:Sized+Default> Default for MemoryBlock<Ty> {
    fn default() -> Self {
        MemoryBlock(Vec::<Ty>::new())
    }
}
impl<Ty:Sized+Default> alloc::SliceWrapper<Ty> for MemoryBlock<Ty> {
    fn slice(&self) -> &[Ty] {
        &self.0[..]
    }
}

impl<Ty:Sized+Default> alloc::SliceWrapperMut<Ty> for MemoryBlock<Ty> {
    fn slice_mut(&mut self) -> &mut [Ty] {
        &mut self.0[..]
    }
}

impl<Ty:Sized+Default> core::ops::Index<usize> for MemoryBlock<Ty> {
    type Output = Ty;
    fn index(&self, index:usize) -> &Ty {
        &self.0[index]
    }
}

impl<Ty:Sized+Default> core::ops::IndexMut<usize> for MemoryBlock<Ty> {

    fn index_mut(&mut self, index:usize) -> &mut Ty {
        &mut self.0[index]
    }
}

struct SubclassableAllocator<Ty:Sized+Default> {
    _ty: core::marker::PhantomData<Ty>,
    alloc: CAllocator
    // have alternative ty here
}
impl<Ty:Sized+Default+Clone> SubclassableAllocator<Ty> {
    fn new(sub_alloc:CAllocator) -> Self {
        SubclassableAllocator::<Ty>{
            _ty:core::marker::PhantomData::<Ty>::default(),
            alloc:sub_alloc,
        }
    }
}
impl<Ty:Sized+Default+Clone> alloc::Allocator<Ty> for SubclassableAllocator<Ty> {
    type AllocatedMemory = MemoryBlock<Ty>;
    fn alloc_cell(&mut self, size:usize) ->MemoryBlock<Ty>{
        MemoryBlock(vec![Ty::default();size])
    }
    fn free_cell(&mut self, _bv:MemoryBlock<Ty>) {

    }
}


#[repr(C)]
#[no_mangle]
#[derive(Clone)]
pub struct CAllocator {
    alloc_func: Option<extern "C" fn(data: *mut c_void, size: usize) -> *mut c_void>,
    free_func: Option<extern "C" fn(data: *mut c_void, ptr: *mut c_void) -> ()>,
    opaque: *mut c_void,
}

#[repr(C)]
#[no_mangle]
pub struct DivansDecompressorState {
    custom_allocator: CAllocator,
    decompressor: super::DivansDecompressor<<super::DivansDecompressorFactoryStruct<SubclassableAllocator<u8>,
                                                                            SubclassableAllocator<super::CDF2>,
                                                                            SubclassableAllocator<super::DefaultCDF16>> as super::DivansDecompressorFactory<SubclassableAllocator<u8>, SubclassableAllocator<super::CDF2>, SubclassableAllocator<super::DefaultCDF16>>>::DefaultDecoder,
                                     SubclassableAllocator<u8>,
                                     SubclassableAllocator<super::CDF2>,
                                     SubclassableAllocator<super::DefaultCDF16>>,
}
impl Drop for DivansDecompressorState {
    fn drop(&mut self) {
        self.decompressor.free_ref();
    }
}

type BrotliFactory = super::BrotliDivansHybridCompressorFactory<SubclassableAllocator<u8>,
                                                         SubclassableAllocator<u16>,
                                                         SubclassableAllocator<u32>,
                                                         SubclassableAllocator<i32>,
                                                         SubclassableAllocator<brotli::enc::command::Command>,
                                                         SubclassableAllocator<super::CDF2>,
                                                         SubclassableAllocator<super::DefaultCDF16>,
                                                         SubclassableAllocator<brotli::enc::util::floatX>,
                                                         SubclassableAllocator<brotli::enc::vectorization::Mem256f>,
                                                         SubclassableAllocator<brotli::enc::histogram::HistogramLiteral>,
                                                         SubclassableAllocator<brotli::enc::histogram::HistogramCommand>,
                                                         SubclassableAllocator<brotli::enc::histogram::HistogramDistance>,
                                                         SubclassableAllocator<brotli::enc::cluster::HistogramPair>,
                                                         SubclassableAllocator<brotli::enc::histogram::ContextType>,
                                                         SubclassableAllocator<brotli::enc::entropy_encode::HuffmanTree>>;

#[repr(C)]
#[no_mangle]
pub struct DivansCompressorState {
    pub custom_allocator: CAllocator,
    compressor: super::BrotliDivansHybridCompressor<super::DefaultCDF16,
                                                    <BrotliFactory as super::DivansCompressorFactory<SubclassableAllocator<u8>,
                                                                                                     SubclassableAllocator<u32>,
                                                                                                     SubclassableAllocator<super::CDF2>,
                                                                                                     SubclassableAllocator<super::DefaultCDF16>>>::DefaultEncoder,
                                                    SubclassableAllocator<u8>,
                                                    SubclassableAllocator<u16>,
                                                    SubclassableAllocator<u32>,
                                                    SubclassableAllocator<i32>,
                                                    SubclassableAllocator<brotli::enc::command::Command>,
                                                    SubclassableAllocator<super::CDF2>,
                                                    SubclassableAllocator<super::DefaultCDF16>,
                                                    SubclassableAllocator<brotli::enc::util::floatX>,
                                                    SubclassableAllocator<brotli::enc::vectorization::Mem256f>,
                                                    SubclassableAllocator<brotli::enc::histogram::HistogramLiteral>,
                                                    SubclassableAllocator<brotli::enc::histogram::HistogramCommand>,
                                                    SubclassableAllocator<brotli::enc::histogram::HistogramDistance>,
                                                    SubclassableAllocator<brotli::enc::cluster::HistogramPair>,
                                                    SubclassableAllocator<brotli::enc::histogram::ContextType>,
                                                    SubclassableAllocator<brotli::enc::entropy_encode::HuffmanTree>>,
}

impl Drop for DivansCompressorState {
    fn drop(&mut self) {
        self.compressor.free_ref();
    }
}

pub struct DivansCompressOptions {
   pub quality: Option<u16>,
   pub window_size: Option<i32>,
   pub lgblock: Option<u32>,
   pub do_context_map: bool,
   pub force_stride_value: super::StrideSelection,
   pub literal_adaptation_speed: Option<super::Speed>,
   pub dynamic_context_mixing: Option<u8>
}
impl Default for DivansCompressOptions {
    fn default() -> Self {
        DivansCompressOptions{
            quality:None,
            window_size:None,
            lgblock:None,
            do_context_map:true,
            force_stride_value: super::StrideSelection::UseBrotliRec,
            literal_adaptation_speed:None,
            dynamic_context_mixing:None,
        }
    }
}


#[no_mangle]
pub extern fn new_compressor_with_custom_alloc(allocators:CAllocator) -> *mut DivansCompressorState{
    let opts = DivansCompressOptions::default();
    Box::<DivansCompressorState>::into_raw(Box::<DivansCompressorState>::new(DivansCompressorState{
        custom_allocator:allocators.clone(),
        compressor:BrotliFactory::new(
            SubclassableAllocator::<u8>::new(allocators.clone()),
            SubclassableAllocator::<u32>::new(allocators.clone()),
            SubclassableAllocator::<super::CDF2>::new(allocators.clone()),
            SubclassableAllocator::<super::DefaultCDF16>::new(allocators.clone()),
            opts.window_size.unwrap_or(21) as usize,
            opts.dynamic_context_mixing.unwrap_or(0),
            opts.literal_adaptation_speed,
            opts.do_context_map,
            opts.force_stride_value,
            (
                SubclassableAllocator::<u8>::new(allocators.clone()),
             SubclassableAllocator::<u16>::new(allocators.clone()),
             SubclassableAllocator::<i32>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::command::Command>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::util::floatX>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::vectorization::Mem256f>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::histogram::HistogramLiteral>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::histogram::HistogramCommand>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::histogram::HistogramDistance>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::cluster::HistogramPair>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::histogram::ContextType>::new(allocators.clone()),
             SubclassableAllocator::<brotli::enc::entropy_encode::HuffmanTree>::new(allocators.clone()),
             opts.quality,
             opts.lgblock,),
            
        ),
    }))
}

#[no_mangle]
pub unsafe extern fn free_compressor(state_ptr: *mut DivansCompressorState) {
    let _state = Box::from_raw(state_ptr);
}

/*
    let mut m8 = SubclassableAllocator::<u8>::default();
    let mut ibuffer = m8.alloc_cell(buffer_size);
    let mut obuffer = m8.alloc_cell(buffer_size);
    let mut state = DivansDecompressorFactoryStruct::<ItemVecAllocator<u8>,
                                         ItemVecAllocator<divans::CDF2>,
                                         ItemVecAllocator<divans::DefaultCDF16>>::new(m8,
                                                                ItemVecAllocator::<divans::CDF2>::default(),
  
}*/
/*
#[no_mangle]
pub extern fn decompressor_init() -> *DivansDecompressorState {
    
}

*/
