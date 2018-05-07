#![cfg(not(feature="safe"))]

#[no_mangle]
use core;
use core::slice;

use super::DivansDecompressorFactory;
use super::interface::Decompressor;
pub mod interface;
pub mod alloc_util;
use self::alloc_util::SubclassableAllocator;
mod compressor;
mod decompressor;
use self::compressor::DivansCompressorState;
use self::decompressor::DivansDecompressorState;
use self::interface::{CAllocator, c_void, DivansOptionSelect, DivansReturnCode, DIVANS_FAILURE, DIVANS_SUCCESS, DIVANS_NEEDS_MORE_INPUT, DIVANS_NEEDS_MORE_OUTPUT};
#[no_mangle]
pub extern fn divans_new_compressor() -> *mut compressor::DivansCompressorState{
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
    alloc_util::Box::<DivansCompressorState>::into_raw(alloc_util::Box::<DivansCompressorState>::new(to_box))
}
#[no_mangle]
pub unsafe extern fn divans_new_compressor_with_custom_alloc(allocators:CAllocator) -> *mut DivansCompressorState{
    let to_box = DivansCompressorState{
        custom_allocator:allocators.clone(),
        compressor:compressor::CompressorState::default(),
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
                                       value: u32) -> DivansReturnCode {
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
                                   output_buf_ptr: *mut u8, output_size: usize, output_offset_ptr: *mut usize) -> DivansReturnCode {
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
                                         output_buf_ptr: *mut u8, output_size: usize, output_offset_ptr: *mut usize) -> DivansReturnCode {
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

#[no_mangle]
pub unsafe extern fn divans_compressor_malloc_u8(state_ptr: *mut DivansCompressorState, size: usize) -> *mut u8 {
    if let Some(alloc_fn) = (*state_ptr).custom_allocator.alloc_func {
            return core::mem::transmute::<*mut c_void, *mut u8>(alloc_fn((*state_ptr).custom_allocator.opaque, size));
    } else {
        return alloc_util::alloc_stdlib(size);
    }
}

#[no_mangle]
pub unsafe extern fn divans_compressor_free_u8(state_ptr: *mut DivansCompressorState, data: *mut u8, size: usize) {
    if let Some(free_fn) = (*state_ptr).custom_allocator.free_func {
        free_fn((*state_ptr).custom_allocator.opaque, core::mem::transmute::<*mut u8, *mut c_void>(data));
    } else {
        alloc_util::free_stdlib(data, size);
    }
}


#[no_mangle]
pub unsafe extern fn divans_compressor_malloc_usize(state_ptr: *mut DivansCompressorState, size: usize) -> *mut usize {
    if let Some(alloc_fn) = (*state_ptr).custom_allocator.alloc_func {
        return core::mem::transmute::<*mut c_void, *mut usize>(alloc_fn((*state_ptr).custom_allocator.opaque,
                                                                         size * core::mem::size_of::<usize>()));
    } else {
        return alloc_util::alloc_stdlib(size);
    }
}
#[no_mangle]
pub unsafe extern fn divans_compressor_free_usize(state_ptr: *mut DivansCompressorState, data: *mut usize, size: usize) {
    if let Some(free_fn) = (*state_ptr).custom_allocator.free_func {
        free_fn((*state_ptr).custom_allocator.opaque, core::mem::transmute::<*mut usize, *mut c_void>(data));
    } else {
        alloc_util::free_stdlib(data, size);
    }
}


#[cfg(not(feature="no-stdlib"))]
unsafe fn free_compressor_no_custom_alloc(state_ptr: *mut DivansCompressorState) {
    let _state = alloc_util::Box::from_raw(state_ptr);
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
        }, 0)
    }
}


#[cfg(feature="no-stdlib")]
fn divans_new_decompressor_without_custom_alloc(_to_box: DivansDecompressorState) -> *mut DivansDecompressorState{
    panic!("Must supply allocators if calling divans when compiled with features=no-stdlib");
}

#[cfg(not(feature="no-stdlib"))]
fn divans_new_decompressor_without_custom_alloc(to_box: DivansDecompressorState) -> *mut DivansDecompressorState{
    alloc_util::Box::<DivansDecompressorState>::into_raw(alloc_util::Box::<DivansDecompressorState>::new(to_box))
}


#[no_mangle]
pub unsafe extern fn divans_new_decompressor_with_custom_alloc(allocators:CAllocator, skip_crc:u8) -> *mut DivansDecompressorState{
    let to_box = DivansDecompressorState{
        custom_allocator:allocators.clone(),
        decompressor:decompressor::DecompressorFactory::new(
            SubclassableAllocator::<u8>::new(allocators.clone()),
            SubclassableAllocator::<super::CDF2>::new(allocators.clone()),
            SubclassableAllocator::<super::DefaultCDF16>::new(allocators.clone()),
            skip_crc != 0,
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
                                   output_buf_ptr: *mut u8, output_size: usize, output_offset_ptr: *mut usize) -> DivansReturnCode {
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
                                ::interface::DivansResult::Success => return DIVANS_SUCCESS,
                                ::interface::DivansResult::Failure(_) => return DIVANS_FAILURE,
                                ::interface::DivansResult::NeedsMoreInput => return DIVANS_NEEDS_MORE_INPUT,
                                ::interface::DivansResult::NeedsMoreOutput => return DIVANS_NEEDS_MORE_OUTPUT,
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
    let _state = alloc_util::Box::from_raw(state_ptr);
}

#[cfg(feature="no-stdlib")]
unsafe fn free_decompressor_no_custom_alloc(_state_ptr: *mut DivansDecompressorState) {
    unreachable!();
}


#[no_mangle]
pub unsafe extern fn divans_decompressor_malloc_u8(state_ptr: *mut DivansDecompressorState, size: usize) -> *mut u8 {
    if let Some(alloc_fn) = (*state_ptr).custom_allocator.alloc_func {
        return core::mem::transmute::<*mut c_void, *mut u8>(alloc_fn((*state_ptr).custom_allocator.opaque, size));
    } else {
        return alloc_util::alloc_stdlib(size);
    }
}

#[no_mangle]
pub unsafe extern fn divans_decompressor_free_u8(state_ptr: *mut DivansDecompressorState, data: *mut u8, size: usize) {
    if let Some(free_fn) = (*state_ptr).custom_allocator.free_func {
        free_fn((*state_ptr).custom_allocator.opaque, core::mem::transmute::<*mut u8, *mut c_void>(data));
    } else {
        alloc_util::free_stdlib(data, size);
    }
}

#[no_mangle]
pub unsafe extern fn divans_decompressor_malloc_usize(state_ptr: *mut DivansDecompressorState, size: usize) -> *mut usize {
    if let Some(alloc_fn) = (*state_ptr).custom_allocator.alloc_func {
        return core::mem::transmute::<*mut c_void, *mut usize>(alloc_fn((*state_ptr).custom_allocator.opaque,
                                                                         size * core::mem::size_of::<usize>()));
    } else {
        return alloc_util::alloc_stdlib(size);
    }
}
#[no_mangle]
pub unsafe extern fn divans_decompressor_free_usize(state_ptr: *mut DivansDecompressorState, data: *mut usize, size: usize) {
    if let Some(free_fn) = (*state_ptr).custom_allocator.free_func {
        free_fn((*state_ptr).custom_allocator.opaque, core::mem::transmute::<*mut usize, *mut c_void>(data));
    } else {
        alloc_util::free_stdlib(data, size);
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
        free_decompressor_no_custom_alloc(state_ptr);
    }
}

