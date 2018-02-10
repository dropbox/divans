extern crate rdiffsig;
extern crate alloc_no_stdlib as alloc;
use std::io::{Read, Write};
use rdiffsig::{
    FixedBuffer8,
    SigFile,
};

fn main() {
    let mut sig_file = Vec::<u8>::new();
    std::io::stdin().read_to_end(&mut sig_file).unwrap();
    let mut m_fixed = alloc::HeapAlloc::<rdiffsig::Sig<FixedBuffer8>>::new(rdiffsig::Sig::<FixedBuffer8>::default());
    let sig = SigFile::<FixedBuffer8, alloc::HeapAlloc<rdiffsig::Sig<FixedBuffer8>>>::deserialize(&mut m_fixed, &sig_file[..]).unwrap();
    let hint = sig.create_sig_hint();
    let mut buf = [0u8; 4096];
    let mut input_offset = 0usize;
    loop {
        let mut output_offset = 0usize;
        let done = sig.serialize(&mut input_offset, &mut buf, &mut output_offset);
        std::io::stdout().write_all(buf.split_at(output_offset).0);
        if done {
            break;
        }
    }
}
