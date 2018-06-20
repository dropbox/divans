extern crate divans;
#[cfg(feature="no-stdlib")]
fn main() {
    panic!("For no-stdlib examples please see the tests")
}
#[cfg(not(feature="no-stdlib"))]
fn main() {
    use std::io;
    let stdin = &mut io::stdin();
    {
        let mut reader = divans::DivansDecompressorReader::new(
            stdin,
            4096, // buffer size
            false,
            true, // parallel
        );
        io::copy(&mut reader, &mut io::stdout()).unwrap();
    }   
}
