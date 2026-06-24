use thiserror::Error;

/// crate specific Result type
pub type Result<T> = std::result::Result<T, Error>;

/// crate specific Error enum
#[derive(Error, Debug)]
pub enum Error {
    /// When trying to add points to a writer that already been closed
    #[cfg(feature = "writer")]
    #[error("This writer has already been closed")]
    ClosedWriter,

    /// When trying to close an empty copc file
    #[cfg(feature = "writer")]
    #[error("There are no points added to this file")]
    EmptyCopcFile,

    /// [las::Error]
    #[error(transparent)]
    LasError(#[from] las::Error),

    /// [laz::LasZipError]
    #[error(transparent)]
    LasZipError(#[from] laz::LasZipError),

    /// The input file-path does not end in .copc.laz
    #[cfg(feature = "writer")]
    #[error("The extension of the file to write does not match .copc.laz")]
    WrongCopcExtension,

    /// The requested resolution is either negative or not normal
    #[error("The requested error is not possible: {}", .0)]
    InvalidResolution(f64),

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
    #[cfg(feature = "writer")]
    #[error("The provided iterator for writing points to copc did not contain any points")]
    EmptyIterator,

    /// Should not be possible
    #[cfg(feature = "writer")]
    #[error("The point could not be added to any node in the octree")]
    PointNotAddedToAnyNode,

    /// If the bounds in the passed in header is invalid
    #[cfg(feature = "writer")]
    #[error("the bounds in the passed in header is not normal: {:?}", .0)]
    InvalidBounds(las::Bounds),

    /// If a point fails to be added to the copc
    #[cfg(feature = "writer")]
    #[error(transparent)]
    InvalidPoint(crate::PointAddError),

    /// If a copc writer is created with invalid max or min node cound bounds
    #[cfg(feature = "writer")]
    #[error("the set min or max sizes for point in node is invalid")]
    InvalidNodeSize,

    /// Unsupported epsg
    #[cfg(feature = "writer")]
    #[error("the found epsg-code is not defined in the crs-definitions library")]
    InvalidEPSGCode(u16),

    /// Unsupported epsg
    #[cfg(feature = "writer")]
    #[error("the lidar file have no defined crs")]
    NoCRSDefined,
}

/// crate specific Error enum related to adding points to the writer
#[cfg(feature = "writer")]
#[derive(Error, Debug)]
pub enum PointAddError {
    /// A point in the iterator passed to [write] did not
    /// match the format specified by the `header` passed to [new]
    ///
    /// [new]: crate::writer::CopcWriter::new
    /// [write]: crate::writer::CopcWriter::write
    #[error("The point attributes of a point in the iterator don't match the header: {:?}", .0)]
    PointAttributesDoNotMatch(las::point::Format),

    /// A point in the iterator passed to [write] was not
    /// inside the bounds of the header passed to [new]
    ///
    /// [new]: crate::writer::CopcWriter::new
    /// [write]: crate::writer::CopcWriter::write
    #[error("A point in the iterator was not inside the bounds of the header")]
    PointNotInBounds,
}
