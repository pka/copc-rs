//! Library for reading Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.
//!
//! Usage example:
//! ```
//! use copc_rs::{BoundsSelection, CopcReader, LodSelection};
//! # use std::fs::File;
//! # use std::io::BufReader;
//!
//! fn main() -> laz::Result<()> {
//!     let laz_file = BufReader::new(File::open("autzen-classified.copc.laz")?);
//!     let mut copc_reader = CopcReader::open(laz_file)?;
//!     for point in copc_reader
//!         .points(LodSelection::Level(0), BoundsSelection::All)
//!         .unwrap()
//!     {
//!         println!("{}, {}, {}", point.x, point.y, point.z);
//!     }
//!     Ok(())
//! }
//!```

mod copc;
mod decompressor;
pub mod header;
mod reader;
mod vlr;

pub use reader::*;
