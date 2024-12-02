/// Library for reading Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.

const COPC: [u8; 4] = [67, 79, 80, 67];

mod copc;
mod decompressor;
mod reader;

pub use las::{Bounds, Vector};
pub use reader::*;
