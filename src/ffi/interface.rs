#[allow(non_camel_case_types)]
#[repr(u8)]
pub enum c_void{
    _Nothing = 0,
}

#[no_mangle]
pub type DivansResult = u8;
pub const DIVANS_SUCCESS: DivansResult = 0;
pub const DIVANS_NEEDS_MORE_INPUT: DivansResult = 1;
pub const DIVANS_NEEDS_MORE_OUTPUT: DivansResult = 2;
pub const DIVANS_FAILURE: DivansResult = 3;



pub type DivansOptionSelect = u8;

pub const DIVANS_OPTION_QUALITY:DivansOptionSelect = 1;
pub const DIVANS_OPTION_WINDOW_SIZE:DivansOptionSelect = 2;
pub const DIVANS_OPTION_LGBLOCK:DivansOptionSelect = 3;
pub const DIVANS_OPTION_DYNAMIC_CONTEXT_MIXING:DivansOptionSelect = 4;
pub const DIVANS_OPTION_USE_BROTLI_COMMAND_SELECTION:DivansOptionSelect = 5;
pub const DIVANS_OPTION_USE_BROTLI_BITSTREAM:DivansOptionSelect = 6;
pub const DIVANS_OPTION_USE_CONTEXT_MAP:DivansOptionSelect = 7;
pub const DIVANS_OPTION_LITERAL_ADAPTATION_CM_HIGH:DivansOptionSelect = 8;
pub const DIVANS_OPTION_FORCE_STRIDE_VALUE:DivansOptionSelect = 9;
pub const DIVANS_OPTION_STRIDE_DETECTION_QUALITY:DivansOptionSelect = 10;
pub const DIVANS_OPTION_PRIOR_DEPTH:DivansOptionSelect = 11;
pub const DIVANS_OPTION_LITERAL_ADAPTATION_STRIDE_HIGH:DivansOptionSelect = 12;
pub const DIVANS_OPTION_LITERAL_ADAPTATION_CM_LOW:DivansOptionSelect = 13;
pub const DIVANS_OPTION_LITERAL_ADAPTATION_STRIDE_LOW:DivansOptionSelect = 14;
pub const DIVANS_OPTION_BROTLI_LITERAL_BYTE_SCORE:DivansOptionSelect = 15;
pub const DIVANS_OPTION_SPEED_DETECTION_QUALITY:DivansOptionSelect = 16;
pub const DIVANS_OPTION_PRIOR_BITMASK_DETECTION:DivansOptionSelect = 17;
pub const DIVANS_OPTION_Q9_5:DivansOptionSelect = 18;


#[repr(C)]
#[no_mangle]
#[derive(Clone)]
pub struct CAllocator {
    pub alloc_func: Option<extern "C" fn(data: *mut c_void, size: usize) -> *mut c_void>,
    pub free_func: Option<extern "C" fn(data: *mut c_void, ptr: *mut c_void) -> ()>,
    pub opaque: *mut c_void,
}




