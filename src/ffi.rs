#![cfg(not(feature="no-stdlib"))]
#![cfg(not(feature="safe"))]
#[no_mangle]
use core;
use brotli;
use std::vec::Vec;
use std::boxed::Box;
use std::os::raw::c_void;
use core::slice;
use brotli::BrotliResult;
use super::alloc;
use super::DivansCompressorFactory;
use super::DivansDecompressorFactory;
use super::interface::Compressor;
use super::interface::Decompressor;
pub extern "C" fn hello_rust() -> *const u8 {
    "Hello, world!\0".as_ptr()
}

#[derive(Debug)]
pub struct MemoryBlock<Ty:Sized+Default>(Box<[Ty]>);
impl<Ty:Sized+Default> Default for MemoryBlock<Ty> {
    fn default() -> Self {
        MemoryBlock(Vec::<Ty>::new().into_boxed_slice())
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

impl<Ty:Sized+Default> Drop for MemoryBlock<Ty> {
    fn drop (&mut self) {
        if self.0.len() != 0 {
            print!("leaking memory block of length {} element size: {}\n", self.0.len(), core::mem::size_of::<Ty>());

            let to_forget = core::mem::replace(self, MemoryBlock::default());
            core::mem::forget(to_forget);// leak it -- it's the only safe way with custom allocators
        }
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
        if let Some(alloc_fn) = self.alloc.alloc_func {
            let ptr = alloc_fn(self.alloc.opaque, size * core::mem::size_of::<Ty>());
            let typed_ptr = unsafe {core::mem::transmute::<*mut c_void, *mut Ty>(ptr)};
            let slice_ref = unsafe {core::slice::from_raw_parts_mut(typed_ptr, size)};
            for item in slice_ref.iter_mut() {
                unsafe{core::ptr::write(item, Ty::default())};
            }
            return MemoryBlock(unsafe{Box::from_raw(slice_ref)})
        }
        MemoryBlock(vec![Ty::default();size].into_boxed_slice())
    }
    fn free_cell(&mut self, mut bv:MemoryBlock<Ty>) {
        if (*bv.0).len() != 0 {
            if let Some(_) = self.alloc.alloc_func {
                let slice_ptr = (*bv.0).as_mut_ptr();
                let _box_ptr = Box::into_raw(core::mem::replace(&mut bv.0, Vec::<Ty>::new().into_boxed_slice()));
                if let Some(free_fn) = self.alloc.free_func {
                    
                    unsafe {free_fn(self.alloc.opaque, core::mem::transmute::<*mut Ty, *mut c_void>(slice_ptr))};
                }
            } else {
                let _to_free = core::mem::replace(&mut bv.0, Vec::<Ty>::new().into_boxed_slice());
            }
        }
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
type DecompressorFactory = super::DivansDecompressorFactoryStruct<SubclassableAllocator<u8>,
                                                                  SubclassableAllocator<super::CDF2>,
                                                                  SubclassableAllocator<super::DefaultCDF16>>;
#[repr(C)]
#[no_mangle]
pub struct DivansDecompressorState {
    custom_allocator: CAllocator,
    decompressor: super::DivansDecompressor<<DecompressorFactory as super::DivansDecompressorFactory<SubclassableAllocator<u8>, SubclassableAllocator<super::CDF2>, SubclassableAllocator<super::DefaultCDF16>>>::DefaultDecoder,
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
pub extern fn divans_new_compressor() -> *mut DivansCompressorState{
    unsafe {
        divans_new_compressor_with_custom_alloc(CAllocator{
            alloc_func:None,
            free_func:None,
            opaque: core::ptr::null_mut(),
        })
    }
}

#[no_mangle]
pub unsafe extern fn divans_new_compressor_with_custom_alloc(allocators:CAllocator) -> *mut DivansCompressorState{
    let opts = DivansCompressOptions::default();
    let to_box = DivansCompressorState{
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
    };
    if let Some(alloc_fn) = allocators.alloc_func {
        let ptr = alloc_fn(allocators.opaque, core::mem::size_of::<DivansCompressorState>());
        let divans_compressor_state_ptr = core::mem::transmute::<*mut c_void, *mut DivansCompressorState>(ptr);
        core::ptr::write(divans_compressor_state_ptr, to_box);
        divans_compressor_state_ptr
    } else {
        Box::<DivansCompressorState>::into_raw(Box::<DivansCompressorState>::new(to_box))
    }
}


#[no_mangle]
pub type DivansResult = u8;
pub const DIVANS_SUCCESS: DivansResult = 0;
pub const DIVANS_NEEDS_MORE_INPUT: DivansResult = 1;
pub const DIVANS_NEEDS_MORE_OUTPUT: DivansResult = 2;
pub const DIVANS_FAILURE: DivansResult = 3;


#[no_mangle]
pub unsafe extern fn divans_encode(state_ptr: *mut DivansCompressorState,
                                   input_buf_ptr: *const u8, input_size: usize, input_offset_ptr: *mut usize,
                                   output_buf_ptr: *mut u8, output_size: usize, output_offset_ptr: *mut usize) -> DivansResult {
    let input_buf = slice::from_raw_parts(input_buf_ptr, input_size);
    let output_buf = slice::from_raw_parts_mut(output_buf_ptr, output_size);
    match input_offset_ptr.as_mut() {
        None => return DIVANS_FAILURE,
        Some(input_offset) => {
            match output_offset_ptr.as_mut() {
                None => return DIVANS_FAILURE,
                Some(output_offset) => {
                    match state_ptr.as_mut() {
                        None => return DIVANS_FAILURE,
                        Some(state_ref) => {
                            match state_ref.compressor.encode(input_buf, input_offset, output_buf, output_offset) {
                                BrotliResult::ResultSuccess => return DIVANS_SUCCESS,
                                BrotliResult::ResultFailure => return DIVANS_FAILURE,
                                BrotliResult::NeedsMoreInput => return DIVANS_NEEDS_MORE_INPUT,
                                BrotliResult::NeedsMoreOutput => return DIVANS_NEEDS_MORE_OUTPUT,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern fn divans_encode_flush(state_ptr: *mut DivansCompressorState,
                                         output_buf_ptr: *mut u8, output_size: usize, output_offset_ptr: *mut usize) -> DivansResult {
    let output_buf = slice::from_raw_parts_mut(output_buf_ptr, output_size);
    match output_offset_ptr.as_mut() {
        None => return DIVANS_FAILURE,
        Some(output_offset) => {
            match state_ptr.as_mut() {
                None => return DIVANS_FAILURE,
                Some(state_ref) => {
                    match state_ref.compressor.flush(output_buf, output_offset) {
                        BrotliResult::ResultSuccess => return DIVANS_SUCCESS,
                        BrotliResult::ResultFailure => return DIVANS_FAILURE,
                        BrotliResult::NeedsMoreInput => return DIVANS_NEEDS_MORE_INPUT,
                        BrotliResult::NeedsMoreOutput => return DIVANS_NEEDS_MORE_OUTPUT,
                    }
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern fn divans_free_compressor(state_ptr: *mut DivansCompressorState) {
    if let Some(_) = (*state_ptr).custom_allocator.alloc_func {
        if let Some(free_fn) = (*state_ptr).custom_allocator.free_func {
            let _to_free = core::ptr::read(state_ptr);
            let ptr = core::mem::transmute::<*mut DivansCompressorState, *mut c_void>(state_ptr);
            free_fn((*state_ptr).custom_allocator.opaque, ptr);
        }
    } else {
        let _state = Box::from_raw(state_ptr);
    }
}












#[no_mangle]
pub extern fn divans_new_decompressor() -> *mut DivansDecompressorState{
    unsafe {
        divans_new_decompressor_with_custom_alloc(CAllocator{
            alloc_func:None,
            free_func:None,
            opaque: core::ptr::null_mut(),
        })
    }
}

#[no_mangle]
pub unsafe extern fn divans_new_decompressor_with_custom_alloc(allocators:CAllocator) -> *mut DivansDecompressorState{
    let to_box = DivansDecompressorState{
        custom_allocator:allocators.clone(),
        decompressor:DecompressorFactory::new(
            SubclassableAllocator::<u8>::new(allocators.clone()),
            SubclassableAllocator::<super::CDF2>::new(allocators.clone()),
            SubclassableAllocator::<super::DefaultCDF16>::new(allocators.clone()),
        ),
    };
    if let Some(alloc_fn) = allocators.alloc_func {
        let ptr = alloc_fn(allocators.opaque, core::mem::size_of::<DivansDecompressorState>());
        let divans_decompressor_state_ptr = core::mem::transmute::<*mut c_void, *mut DivansDecompressorState>(ptr);
        core::ptr::write(divans_decompressor_state_ptr, to_box);
        divans_decompressor_state_ptr
    } else {
        Box::<DivansDecompressorState>::into_raw(Box::<DivansDecompressorState>::new(to_box))
    }
}


#[no_mangle]
pub unsafe extern fn divans_decode(state_ptr: *mut DivansDecompressorState,
                                   input_buf_ptr: *const u8, input_size: usize, input_offset_ptr: *mut usize,
                                   output_buf_ptr: *mut u8, output_size: usize, output_offset_ptr: *mut usize) -> DivansResult {
    let input_buf = slice::from_raw_parts(input_buf_ptr, input_size);
    let output_buf = slice::from_raw_parts_mut(output_buf_ptr, output_size);
    match input_offset_ptr.as_mut() {
        None => return DIVANS_FAILURE,
        Some(input_offset) => {
            match output_offset_ptr.as_mut() {
                None => return DIVANS_FAILURE,
                Some(output_offset) => {
                    match state_ptr.as_mut() {
                        None => return DIVANS_FAILURE,
                        Some(state_ref) => {
                            match state_ref.decompressor.decode(input_buf, input_offset, output_buf, output_offset) {
                                BrotliResult::ResultSuccess => return DIVANS_SUCCESS,
                                BrotliResult::ResultFailure => return DIVANS_FAILURE,
                                BrotliResult::NeedsMoreInput => return DIVANS_NEEDS_MORE_INPUT,
                                BrotliResult::NeedsMoreOutput => return DIVANS_NEEDS_MORE_OUTPUT,
                            }
                        }
                    }
                }
            }
        }
    }
}


#[no_mangle]
pub unsafe extern fn divans_free_decompressor(state_ptr: *mut DivansDecompressorState) {
    if let Some(_) = (*state_ptr).custom_allocator.alloc_func {
        if let Some(free_fn) = (*state_ptr).custom_allocator.free_func {
            //(*state_ptr).drop();
            let _to_free = core::ptr::read(state_ptr);
            let ptr = core::mem::transmute::<*mut DivansDecompressorState, *mut c_void>(state_ptr);
            free_fn((*state_ptr).custom_allocator.opaque, ptr);
        }
    } else {
        let _state = Box::from_raw(state_ptr);
    }
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
