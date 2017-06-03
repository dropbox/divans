extern crate alloc_no_stdlib as alloc;

pub use alloc::{AllocatedStackMemory, Allocator, SliceWrapper, SliceWrapperMut, StackAllocator};

#[derive(Debug)]
pub struct CopyCommand {
    pub distance: usize,
    pub num_bytes: usize,
}

#[derive(Debug)]
pub struct DictCommand {
    pub word_size: u8,
    pub transform: u8,
    pub final_size: u8,
    pub _empty: u8,
    pub word_id: u32,
}

#[derive(Debug)]
pub struct LiteralCommand<SliceType:alloc::SliceWrapper<u8>> {
    pub data: SliceType,
}

#[derive(Debug)]
pub enum Command<SliceType:alloc::SliceWrapper<u8> > {
    Copy(CopyCommand),
    Dict(DictCommand),
    Literal(LiteralCommand<SliceType>),
}

pub struct DivansRecodeState<AllocU8: alloc::Allocator<u8> >{
    input_sub_offset :usize,
    ring_buffer: AllocU8::AllocatedMemory,
    ring_buffer_index: u32,
    ring_buffer_output: u32,
    m8: AllocU8,
}
impl<AllocU8: alloc::Allocator<u8>> DivansRecodeState {
    fn new(window_size: u32, m8: AllocU8) -> Self {
        assert!(window_size >= 1008 && window_size <= (1 << 24));
        let ring_buffer: m8.alloc_cell(1usize << (32 - (window_size as u32).count_leading_zeros()));
        DivansRecodeState {
            m8: m8,
            ring_buffer: ring_buffer,
            ring_buffer_index: 2,
            ring_buffer_output: 2,
            input_sub_offset: 0,
        }
    }
    pub fn flush(output :&[u8], output_offset: &mut usize) {
        
    }
    pub fn encode(&mut self,
                  input:&[&Command],
                  input_offset : &mut usize,
                  output :&[u8],
                  output_offset: &mut usize) {
        state.flush(output, output_offset);
    }
}
