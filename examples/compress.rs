extern crate divans;
#[cfg(feature="no-stdlib")]
fn main() {
    panic!("For no-stdlib examples please see the tests")
}
#[cfg(not(feature="no-stdlib"))]
fn main() {
    use std::io;
    let stdout = &mut io::stdout();
    {
        use std::io::{Read, Write};
        let mut writer = divans::DivansBrotliHybridCompressorWriter::new(
            stdout,
            divans::DivansCompressorOptions{
                literal_adaptation:None,
                window_size:Some(22),
                lgblock:None,
                quality:Some(11),
                dynamic_context_mixing:Some(2),
                use_brotli:divans::BrotliCompressionSetting::default(),
                use_context_map:true,
                force_stride_value: divans::StrideSelection::UseBrotliRec,
            },
            4096 /* buffer size */);
        let mut buf = [0u8; 4096];
        loop {
            match io::stdin().read(&mut buf[..]) {
                Err(e) => {
                    if let io::ErrorKind::Interrupted = e.kind() {
                        continue;
                    }
                    panic!(e);
                }
                Ok(size) => {
                    if size == 0 {
                        match writer.flush() {
                            Err(e) => {
                                if let io::ErrorKind::Interrupted = e.kind() {
                                    continue;
                                }
                                panic!(e)
                            }
                            Ok(_) => break,
                        }
                    }
                    match writer.write_all(&buf[..size]) {
                        Err(e) => panic!(e),
                        Ok(_) => {},
                    }
                }
            }
        }
    }
}
