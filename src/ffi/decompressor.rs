use super::alloc_util::SubclassableAllocator;
use super::interface::CAllocator;
//use ::interface::DivansDecompressorFactory;
pub type DecompressorFactory = ::DivansDecompressorFactoryStruct<SubclassableAllocator<u8>,
                                                                  SubclassableAllocator<::DefaultCDF16>>;
#[repr(C)]
#[no_mangle]
pub struct DivansDecompressorState {
    pub custom_allocator: CAllocator,
    pub decompressor: ::DivansDecompressor<<DecompressorFactory as ::DivansDecompressorFactory<SubclassableAllocator<u8>,  SubclassableAllocator<::DefaultCDF16>>>::DefaultDecoder,
                                     SubclassableAllocator<u8>,
                                     SubclassableAllocator<::DefaultCDF16>>,
}
impl Drop for DivansDecompressorState {
    fn drop(&mut self) {
        self.decompressor.free_ref();
    }
}

