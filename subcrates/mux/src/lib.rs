extern crate alloc_no_stdlib as alloc;
extern crate core;

mod interface;
mod mux;
pub mod util;

pub use interface::*;
pub use mux::{DevNull, Mux, EOF_MARKER};
