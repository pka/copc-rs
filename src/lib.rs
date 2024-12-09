/// Library for reading Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.
use thiserror::Error;

const MAX_NODE_SIZE_DEFAULT: i32 = 1024;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    /// [las::Error]
    #[error(transparent)]
    LasError(#[from] las::Error),

    /// [laz::LasZipError]
    #[error(transparent)]
    LasZipError(#[from] laz::LasZipError),

    /// A point in the iterator passed to [new] was not
    /// inside the bounds of the header passed to [new]
    ///
    /// [new]: crate::writer::CopcWriter::new
    #[error("The point to add to the octree is not inside the root bounds")]
    PointNotInBounds,

    /// The input file-path does not end in .copc.laz
    #[error("The extension of the file to write does not match .copc.laz")]
    WrongCopcExtension,

    /// Only las version 1.4 can be written to .copc.laz
    #[error("Only las version 1.4 can be written to .copc.laz. Given version: {:?}", .0)]
    WrongLasVersion(las::Version),

    /// A header of a las version 1.4 file must by 375 bytes long
    #[error("A header of a las version 1.4 file must by 375 bytes long. Given header length: {:?}", .0)]
    HeaderNot375Bytes(u16),

    /// [std::io::Error]
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The Copc Info vlr was not found, octree can not be built
    #[error("The source to be read does not contain a COPC info vlr")]
    CopcInfoVlrNotFound,

    /// The Ept hierarchy evlr was not found, octree can not be built
    #[error("The source to be read does not contain a EPT hierarchy vlr")]
    EptHierarchyVlrNotFound,

    /// The laszip vlr was not found, the points cannot be decompressed.
    #[error("laszip vlr not found")]
    LasZipVlrNotFound,

    /// The provided iterator for writing points to copc did not contain any points
    #[error("The provided iterator for writing points to copc did not contain any points")]
    EmptyIterator,

    /// Should not be possible
    #[error("The point could not be added to any node in the octree")]
    PointNotAddedToAnyNode,
}

mod compressor;
mod copc;
mod decompressor;
mod reader;
mod writer;

pub use las::{Bounds, Vector};
pub use reader::*;
pub use writer::*;
