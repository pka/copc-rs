use laz::laszip::LazVlr;
use laz::record::{LayeredPointRecordDecompressor, RecordDecompressor};
use std::io::{Read, Seek, SeekFrom};

/// LasZip decompressor.
pub(crate) struct CopcDecompressor<'a, R: Read + Seek> {
    start: u64,
    vlr: &'a LazVlr,
    record_decompressor: LayeredPointRecordDecompressor<'a, R>,
}

// Stripped down variant of laz::LasZipDecompressor
// without ChunkTable reading as enough info is stored in COPC-evlr
impl<'a, R: Read + Seek> CopcDecompressor<'a, R> {
    /// Creates a new instance from a data source of compressed points
    /// and the LazVlr describing the compressed data
    pub(crate) fn new(mut source: R, vlr: &'a LazVlr) -> laz::Result<Self> {
        // the read was seeked to the beginning of the las file in the read stream before calling new
        let start = source.stream_position()?;
        let mut record_decompressor = LayeredPointRecordDecompressor::new(source);

        // an early fail-check to avoid a potential panic when PointIter.next() unwraps a call to source_seek
        record_decompressor.set_fields_from(vlr.items())?;

        Ok(Self {
            start,
            vlr,
            record_decompressor,
        })
    }

    #[inline]
    pub(crate) fn source_seek(&mut self, offset: u64) -> laz::Result<()> {
        self.record_decompressor
            .get_mut()
            .seek(SeekFrom::Start(offset + self.start))?;

        self.record_decompressor.reset();
        self.record_decompressor.set_fields_from(self.vlr.items())
    }

    /// Decompress the next point and write the uncompressed data to the out buffer.
    ///
    /// - The buffer should have at least enough byte to store the decompressed data
    /// - The data is written in the buffer exactly as it would have been in a LAS File
    ///   in Little Endian order,
    #[inline]
    pub(crate) fn decompress_one(&mut self, out: &mut [u8]) -> laz::Result<()> {
        self.record_decompressor
            .decompress_next(out)
            .map_err(laz::errors::LasZipError::IoError)
    }
}
