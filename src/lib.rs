//! Library for reading and writing Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.

#[cfg(feature = "writer")]
const MIN_NODE_SIZE_DEFAULT: i32 = 256;
#[cfg(feature = "writer")]
const MAX_NODE_SIZE_DEFAULT: i32 = 16384;
#[cfg(feature = "writer")]
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "writer")]
mod compressor;
mod copc;
mod decompressor;
mod error;
mod reader;
#[cfg(feature = "writer")]
mod writer;

pub use error::*;
pub use las::{Bounds, Vector};
pub use reader::*;
#[cfg(feature = "writer")]
pub use writer::*;
