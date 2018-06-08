extern crate divans;
#[cfg(feature="no-stdlib")]
fn main() {
    panic!("For no-stdlib examples please see the tests")
}
#[cfg(not(feature="no-stdlib"))]
fn main() {
    let example_opts = divans::DivansCompressorOptions::default();
    use std::io;
    let stdout = &mut io::stdout();
    {
        use std::io::{Read, Write};
        let mut writer = divans::DivansBrotliHybridCompressorWriter::new(
            stdout,
            divans::DivansCompressorOptions{
                brotli_literal_byte_score:example_opts.brotli_literal_byte_score,
                force_literal_context_mode:example_opts.force_literal_context_mode,
                literal_adaptation:example_opts.literal_adaptation, // should we override how fast the cdfs converge for literals?
                window_size:example_opts.window_size, // log 2 of the window size
                lgblock:example_opts.lgblock, // should we override how often metablocks are created in brotli
                quality:example_opts.quality, // the quality of brotli commands
                q9_5:example_opts.q9_5,
                dynamic_context_mixing:example_opts.dynamic_context_mixing, // if we want to mix together the stride prediction and the context map
                prior_depth:example_opts.prior_depth,
                use_brotli:example_opts.use_brotli, // ignored
                use_context_map:example_opts.use_context_map, // whether we should use the brotli context map in addition to the last 8 bits of each byte as a prior
                force_stride_value: example_opts.force_stride_value, // if we should use brotli to decide on the stride
                speed_detection_quality: example_opts.speed_detection_quality,
                stride_detection_quality: example_opts.stride_detection_quality,
                prior_bitmask_detection: example_opts.prior_bitmask_detection,
                divans_ir_optimizer:example_opts.divans_ir_optimizer,
            },
            4096, // internal buffer size
        );
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
