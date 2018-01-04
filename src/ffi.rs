#![cfg(not(feature="safe"))]

#[no_mangle]
use core;
use brotli;
#[cfg(not(feature="no-stdlib"))]
use std::vec::Vec;
#[cfg(not(feature="no-stdlib"))]
use std::boxed::Box;
use core::slice;
use brotli::BrotliResult;
use super::alloc;
use super::DivansCompressorFactory;
use super::DivansDecompressorFactory;
//use super::DivansDecompressorFactoryStruct;
use super::DivansCompressorFactoryStruct;
use super::probability::Speed;
use super::interface::{Compressor, Decompressor, DivansCompressorOptions, BrotliCompressionSetting, StrideSelection};

#[allow(non_camel_case_types)]
#[repr(u8)]
enum c_void{
    _Nothing = 0,
}

#[no_mangle]
pub type DivansResult = u8;
pub const DIVANS_SUCCESS: DivansResult = 0;
pub const DIVANS_NEEDS_MORE_INPUT: DivansResult = 1;
pub const DIVANS_NEEDS_MORE_OUTPUT: DivansResult = 2;
pub const DIVANS_FAILURE: DivansResult = 3;



type DivansOptionSelect = u8;

pub const DIVANS_OPTION_QUALITY:DivansOptionSelect = 1;
pub const DIVANS_OPTION_WINDOW_SIZE:DivansOptionSelect = 2;
pub const DIVANS_OPTION_LGBLOCK:DivansOptionSelect = 3;
pub const DIVANS_OPTION_DYNAMIC_CONTEXT_MIXING:DivansOptionSelect = 4;
pub const DIVANS_OPTION_USE_BROTLI_COMMAND_SELECTION:DivansOptionSelect = 5;
pub const DIVANS_OPTION_USE_BROTLI_BITSTREAM:DivansOptionSelect = 6;
pub const DIVANS_OPTION_USE_CONTEXT_MAP:DivansOptionSelect = 7;
pub const DIVANS_OPTION_FORCE_STRIDE_VALUE:DivansOptionSelect = 8;
pub const DIVANS_OPTION_LITERAL_ADAPTATION:DivansOptionSelect = 9;


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
type InternalCompressorFactory = DivansCompressorFactoryStruct<SubclassableAllocator<u8>,
                                                         SubclassableAllocator<super::CDF2>,
                                                         SubclassableAllocator<super::DefaultCDF16>>;
enum CompressorState {
    OptionStage(DivansCompressorOptions),
    BrotliCompressor(super::BrotliDivansHybridCompressor<super::DefaultCDF16,
                                                         <BrotliFactory as super::DivansCompressorFactory<SubclassableAllocator<u8>,
                                                                                                          SubclassableAllocator<u32>,
                                                                                                           SubclassableAllocator<super::CDF2>,
                                                                                                          SubclassableAllocator<super::DefaultCDF16>>
                                                          >::DefaultEncoder,
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
                                                         SubclassableAllocator<brotli::enc::entropy_encode::HuffmanTree>>),
    InternalCompressor(super::DivansCompressor<<InternalCompressorFactory as super::DivansCompressorFactory<SubclassableAllocator<u8>,
                                                                                                             SubclassableAllocator<u32>,
                                                                                                             SubclassableAllocator<super::CDF2>,
                                                                                                             SubclassableAllocator<super::DefaultCDF16>>
                                               >::DefaultEncoder,
                       SubclassableAllocator<u8>,
                       SubclassableAllocator<u32>,
                       SubclassableAllocator<super::CDF2>,
                       SubclassableAllocator<super::DefaultCDF16>>),
}

impl Default for CompressorState {
    fn default() -> Self {
        CompressorState::OptionStage(DivansCompressorOptions::default())
    }
}
impl CompressorState {
    fn set_option(&mut self, selector: DivansOptionSelect, value: u32) -> DivansResult {
        if let CompressorState::OptionStage(ref mut opts) = *self {
            match selector {
                DIVANS_OPTION_QUALITY => {opts.quality = Some(value as u16);},
                DIVANS_OPTION_WINDOW_SIZE => {opts.window_size = Some(value as i32);},
                DIVANS_OPTION_LGBLOCK => {opts.lgblock = Some(value);},
                DIVANS_OPTION_DYNAMIC_CONTEXT_MIXING => {opts.dynamic_context_mixing = Some(value as u8);},
                DIVANS_OPTION_USE_BROTLI_COMMAND_SELECTION => {opts.use_brotli = match value {
                    0 => BrotliCompressionSetting::UseInternalCommandSelection,
                    1 => BrotliCompressionSetting::UseBrotliCommandSelection,
                    2 => BrotliCompressionSetting::UseBrotliBitstream,
                    _ => return DIVANS_FAILURE,
                };},
                DIVANS_OPTION_USE_BROTLI_BITSTREAM => {opts.use_brotli = match value {
                    1 => BrotliCompressionSetting::UseBrotliBitstream,
                    _ => return DIVANS_FAILURE,
                };},
                DIVANS_OPTION_USE_CONTEXT_MAP => {opts.use_context_map = match value {
                    1 => true,
                    0 => false,
                    _ => return DIVANS_FAILURE,
                };},
                DIVANS_OPTION_FORCE_STRIDE_VALUE => {opts.force_stride_value = match value {
                    0 => StrideSelection::PriorDisabled,
                    1 => StrideSelection::Stride1,
                    2 => StrideSelection::Stride2,
                    3 => StrideSelection::Stride3,
                    4 => StrideSelection::Stride4,
                    5 => StrideSelection::Stride5,
                    6 => StrideSelection::Stride6,
                    7 => StrideSelection::Stride7,
                    8 => StrideSelection::Stride8,
                    _ => return DIVANS_FAILURE,
                };},
                DIVANS_OPTION_LITERAL_ADAPTATION => {
                    opts.literal_adaptation = Some(match value {
                        0 => Speed::GEOLOGIC,
                        1 => Speed::GLACIAL,
                        2 => Speed::MUD,
                        3 => Speed::SLOW,
                        4 => Speed::MED,
                        5 => Speed::FAST,
                        6 => Speed::PLANE,
                        7 => Speed::ROCKET,
                        _ => return DIVANS_FAILURE,
                    });
                },
                _ => return DIVANS_FAILURE,
            }
            return DIVANS_SUCCESS;
        }
        DIVANS_FAILURE
    }
    fn start(&mut self, allocators: &CAllocator, opts:DivansCompressorOptions) {
        match opts.use_brotli {
            BrotliCompressionSetting::UseInternalCommandSelection => {
                core::mem::replace(self,
                                   CompressorState::InternalCompressor(
                                       InternalCompressorFactory::new(
                                           SubclassableAllocator::<u8>::new(allocators.clone()),
                                           SubclassableAllocator::<u32>::new(allocators.clone()),
                                           SubclassableAllocator::<super::CDF2>::new(allocators.clone()),
                                           SubclassableAllocator::<super::DefaultCDF16>::new(allocators.clone()),
                                           opts.window_size.unwrap_or(21) as usize,
                                           opts.dynamic_context_mixing.unwrap_or(0),
                                           opts.literal_adaptation,
                                           opts.use_context_map,
                                           opts.force_stride_value,
                                           ())));
            },
            _ => {
                core::mem::replace(self,
                                   CompressorState::BrotliCompressor(
                                       BrotliFactory::new(
                                           SubclassableAllocator::<u8>::new(allocators.clone()),
                                           SubclassableAllocator::<u32>::new(allocators.clone()),
                                           SubclassableAllocator::<super::CDF2>::new(allocators.clone()),
                                           SubclassableAllocator::<super::DefaultCDF16>::new(allocators.clone()),
                                           opts.window_size.unwrap_or(21) as usize,
                                           opts.dynamic_context_mixing.unwrap_or(0),
                                           opts.literal_adaptation,
                                           opts.use_context_map,
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
                                               opts.lgblock
                                           ))));
            
            }

        }
    }
    fn encode(&mut self,
              input_buf:&[u8],
              input_offset: &mut usize,
              output_buf:&mut[u8],
              output_offset: &mut usize,
              allocators: &CAllocator) -> DivansResult {
        if let CompressorState::OptionStage(opts) = *self {
            self.start(allocators, opts);
        }
        let res = match *self {
            CompressorState::OptionStage(_) => unreachable!(),
            CompressorState::BrotliCompressor(ref mut compressor) => {
                compressor.encode(input_buf, input_offset, output_buf, output_offset)
            },
            CompressorState::InternalCompressor(ref mut compressor) => {
                compressor.encode(input_buf, input_offset, output_buf, output_offset)
            },
        };
        match res {
            BrotliResult::ResultSuccess => DIVANS_SUCCESS,
            BrotliResult::ResultFailure => DIVANS_FAILURE,
            BrotliResult::NeedsMoreInput => DIVANS_NEEDS_MORE_INPUT,
            BrotliResult::NeedsMoreOutput => DIVANS_NEEDS_MORE_OUTPUT,
        }
    }
    fn flush(&mut self,
              output_buf:&mut[u8],
             output_offset: &mut usize,
             allocators: &CAllocator) -> DivansResult {
        if let CompressorState::OptionStage(opts) = *self {
            self.start(allocators, opts);
        }
        let res = match *self {
            CompressorState::OptionStage(_) => unreachable!(),
            CompressorState::BrotliCompressor(ref mut compressor) => {
                compressor.flush(output_buf, output_offset)
            },
            CompressorState::InternalCompressor(ref mut compressor) => {
                compressor.flush(output_buf, output_offset)
            },
        };
        match res {
            BrotliResult::ResultSuccess => DIVANS_SUCCESS,
            BrotliResult::ResultFailure => DIVANS_FAILURE,
            BrotliResult::NeedsMoreInput => DIVANS_NEEDS_MORE_INPUT,
            BrotliResult::NeedsMoreOutput => DIVANS_NEEDS_MORE_OUTPUT,
        }
    }
}


#[repr(C)]
#[no_mangle]
pub struct DivansCompressorState {
    pub custom_allocator: CAllocator,
    compressor: CompressorState
}

impl Drop for DivansCompressorState {
    fn drop(&mut self) {
        match self.compressor {
            CompressorState::OptionStage(_) => {},
            CompressorState::BrotliCompressor(ref mut compressor) => {
                compressor.free_ref();
              
            },
            CompressorState::InternalCompressor(ref mut compressor) => {
                compressor.free_ref();
            }
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

#[cfg(feature="no-stdlib")]
fn divans_new_compressor_without_custom_alloc(_to_box: DivansCompressorState) -> *mut DivansCompressorState{
    panic!("Must supply allocators if calling divans when compiled with features=no-stdlib");
}
#[cfg(not(feature="no-stdlib"))]
fn divans_new_compressor_without_custom_alloc(to_box: DivansCompressorState) -> *mut DivansCompressorState{
    Box::<DivansCompressorState>::into_raw(Box::<DivansCompressorState>::new(to_box))
}
#[no_mangle]
pub unsafe extern fn divans_new_compressor_with_custom_alloc(allocators:CAllocator) -> *mut DivansCompressorState{
    let to_box = DivansCompressorState{
        custom_allocator:allocators.clone(),
        compressor:CompressorState::default(),
    };
    if let Some(alloc_fn) = allocators.alloc_func {
        let ptr = alloc_fn(allocators.opaque, core::mem::size_of::<DivansCompressorState>());
        let divans_compressor_state_ptr = core::mem::transmute::<*mut c_void, *mut DivansCompressorState>(ptr);
        core::ptr::write(divans_compressor_state_ptr, to_box);
        divans_compressor_state_ptr
    } else {
        divans_new_compressor_without_custom_alloc(to_box)
    }
}


#[no_mangle]
pub unsafe extern fn divans_set_option(state_ptr: *mut DivansCompressorState,
                                       selector: DivansOptionSelect,
                                       value: u32) -> DivansResult {
    match state_ptr.as_mut() {
        None => DIVANS_FAILURE,
        Some(state_ref) => {
            state_ref.compressor.set_option(selector, value)
        }
    }
}
     
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
                            return state_ref.compressor.encode(input_buf, input_offset, output_buf, output_offset, &state_ref.custom_allocator);
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
                    return state_ref.compressor.flush(output_buf, output_offset, &state_ref.custom_allocator);
                }
            }
        }
    }
}

#[cfg(not(feature="no-stdlib"))]
unsafe fn free_compressor_no_custom_alloc(state_ptr: *mut DivansCompressorState) {
    let _state = Box::from_raw(state_ptr);
}

#[cfg(feature="no-stdlib")]
unsafe fn free_compressor_no_custom_alloc(_state_ptr: *mut DivansCompressorState) {
    unreachable!();
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
        free_compressor_no_custom_alloc(state_ptr)
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


#[cfg(feature="no-stdlib")]
fn divans_new_decompressor_without_custom_alloc(_to_box: DivansDecompressorState) -> *mut DivansDecompressorState{
    panic!("Must supply allocators if calling divans when compiled with features=no-stdlib");
}

#[cfg(not(feature="no-stdlib"))]
fn divans_new_decompressor_without_custom_alloc(to_box: DivansDecompressorState) -> *mut DivansDecompressorState{
    Box::<DivansDecompressorState>::into_raw(Box::<DivansDecompressorState>::new(to_box))
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
        divans_new_decompressor_without_custom_alloc(to_box)
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

#[cfg(not(feature="no-stdlib"))]
unsafe fn free_decompressor_no_custom_alloc(state_ptr: *mut DivansDecompressorState) {
    let _state = Box::from_raw(state_ptr);
}

#[cfg(feature="no-stdlib")]
unsafe fn free_decompressor_no_custom_alloc(_state_ptr: *mut DivansDecompressorState) {
    unreachable!();
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
        free_decompressor_no_custom_alloc(state_ptr);
    }
}


#[cfg(not(feature="no-stdlib"))]
#[derive(Debug)]
pub struct MemoryBlock<Ty:Sized+Default>(Box<[Ty]>);
#[cfg(not(feature="no-stdlib"))]
impl<Ty:Sized+Default> Default for MemoryBlock<Ty> {
    fn default() -> Self {
        MemoryBlock(Vec::<Ty>::new().into_boxed_slice())
    }
}
#[cfg(not(feature="no-stdlib"))]
impl<Ty:Sized+Default> alloc::SliceWrapper<Ty> for MemoryBlock<Ty> {
    fn slice(&self) -> &[Ty] {
        &self.0[..]
    }
}
#[cfg(not(feature="no-stdlib"))]
impl<Ty:Sized+Default> alloc::SliceWrapperMut<Ty> for MemoryBlock<Ty> {
    fn slice_mut(&mut self) -> &mut [Ty] {
        &mut self.0[..]
    }
}
#[cfg(not(feature="no-stdlib"))]
impl<Ty:Sized+Default> core::ops::Index<usize> for MemoryBlock<Ty> {
    type Output = Ty;
    fn index(&self, index:usize) -> &Ty {
        &self.0[index]
    }
}
#[cfg(not(feature="no-stdlib"))]
impl<Ty:Sized+Default> core::ops::IndexMut<usize> for MemoryBlock<Ty> {

    fn index_mut(&mut self, index:usize) -> &mut Ty {
        &mut self.0[index]
    }
}
#[cfg(not(feature="no-stdlib"))]
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
#[cfg(not(feature="no-stdlib"))]
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












static mut G_SLICE:&mut[u8] = &mut[];
#[cfg(feature="no-stdlib")]
#[derive(Debug)]
pub struct MemoryBlock<Ty:Sized+Default>(*mut[Ty]);
#[cfg(feature="no-stdlib")]
impl<Ty:Sized+Default> Default for MemoryBlock<Ty> {
    fn default() -> Self {
        MemoryBlock(unsafe{core::mem::transmute::<*mut [u8], *mut[Ty]>(G_SLICE.as_mut())})
    }
}
#[cfg(feature="no-stdlib")]
impl<Ty:Sized+Default> alloc::SliceWrapper<Ty> for MemoryBlock<Ty> {
    fn slice(&self) -> &[Ty] {
        if unsafe{(*self.0).len()} == 0 {
            &[]
        } else {
            unsafe{core::slice::from_raw_parts(&(*self.0)[0], (*self.0).len())}
        }
    }
}
#[cfg(feature="no-stdlib")]
impl<Ty:Sized+Default> alloc::SliceWrapperMut<Ty> for MemoryBlock<Ty> {
    fn slice_mut(&mut self) -> &mut [Ty] {
        if unsafe{(*self.0).len()} == 0 {
            &mut []
        } else {
            unsafe{core::slice::from_raw_parts_mut(&mut (*self.0)[0], (*self.0).len())}
        }
    }
}

#[cfg(feature="no-stdlib")]
#[cfg(not(feature="no-stdlib-rust-binding"))]
#[lang="panic_fmt"]
extern fn panic_fmt(_: ::core::fmt::Arguments, _: &'static str, _: u32) -> ! {
    loop {}
}
#[cfg(feature="no-stdlib")]
#[cfg(not(feature="no-stdlib-rust-binding"))]
#[lang = "eh_personality"]
extern "C" fn eh_personality() {
}

#[cfg(feature="no-stdlib")]
impl<Ty:Sized+Default> core::ops::Index<usize> for MemoryBlock<Ty> {
    type Output = Ty;
    fn index(&self, index:usize) -> &Ty {
        unsafe{&(*self.0)[index]}
    }
}
#[cfg(feature="no-stdlib")]
impl<Ty:Sized+Default> core::ops::IndexMut<usize> for MemoryBlock<Ty> {

    fn index_mut(&mut self, index:usize) -> &mut Ty {
        unsafe{&mut (*self.0)[index]}
    }
}

#[cfg(feature="no-stdlib")]
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
            return MemoryBlock(slice_ref.as_mut())
        } else {
            panic!("Must provide allocators in no-stdlib code");
        }
    }
    fn free_cell(&mut self, mut bv:MemoryBlock<Ty>) {
        use alloc::SliceWrapper;
        use alloc::SliceWrapperMut;
        if bv.slice().len() != 0 {
            if let Some(_) = self.alloc.alloc_func {
                if let Some(free_fn) = self.alloc.free_func {
                    unsafe {free_fn(self.alloc.opaque, core::mem::transmute::<*mut Ty, *mut c_void>((&mut bv.slice_mut()[0])))};
                }
                core::mem::replace(&mut bv, MemoryBlock::<Ty>::default());
            } else {
                panic!("Must provide allocators in no-stdlib code");
            }
        }
    }
}
