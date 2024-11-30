use laz::laszip::{ChunkTable, LazVlr};
use laz::record::{LayeredPointRecordDecompressor, RecordDecompressor};
use laz::LasZipError::MissingChunkTable;
use std::io::{Read, Seek, SeekFrom};

/// LasZip decompressor.
pub struct LasZipDecompressor<'a, R: Read + Seek + 'a> {
    vlr: &'a LazVlr,
    record_decompressor: Box<dyn RecordDecompressor<R> + Send + 'a>,
    data_start: u64,
    chunk_table: Option<ChunkTable>,
    current_chunk: usize,
    chunk_points_read: u64,
    num_points_in_chunk: u64,
}

// Variant of laz::LasZipDecompressor with optional ChunkTable reading
impl<'a, R: Read + Seek + Send + 'a> LasZipDecompressor<'a, R> {
    /// Creates a new instance from a data source of compressed points
    /// and the LazVlr describing the compressed data
    pub fn new(mut source: R, data_start: Option<u64>, vlr: &'a LazVlr) -> laz::Result<Self> {
        let (chunk_table, data_start) = if let Some(data_start) = data_start {
            (None, data_start)
        } else {
            let chunk_table = match ChunkTable::read_from(&mut source, vlr) {
                Ok(chunk_table) => Some(chunk_table),
                Err(e) => {
                    return Err(e);
                }
            };
            let data_start = source.stream_position()?;
            (chunk_table, data_start)
        };

        let decompressor = LayeredPointRecordDecompressor::new(source);
        let record_decompressor = Box::new(decompressor) as Box<dyn RecordDecompressor<R> + Send>;

        Ok(Self {
            vlr,
            record_decompressor,
            data_start,
            chunk_table,
            current_chunk: 0,
            chunk_points_read: 0,
            num_points_in_chunk: 1,
        })
    }

    /// Decompress the next point and write the uncompressed data to the out buffer.
    ///
    /// - The buffer should have at least enough byte to store the decompressed data
    /// - The data is written in the buffer exactly as it would have been in a LAS File
    ///   in Little Endian order,
    pub fn decompress_one(&mut self, out: &mut [u8]) -> std::io::Result<()> {
        if self.chunk_points_read == self.num_points_in_chunk {
            self.reset_for_new_chunk();
            self.current_chunk += 1;
        }

        self.record_decompressor.decompress_next(out)?;
        self.chunk_points_read += 1;

        if self.chunk_points_read == 1 {
            if self.vlr.uses_variable_size_chunks() {
                self.num_points_in_chunk = match &self.chunk_table {
                    Some(chunk_table) => chunk_table[self.current_chunk].point_count,
                    None => self.record_decompressor.record_count(),
                }
            } else {
                self.num_points_in_chunk = self.vlr.chunk_size().into();
            }
        }
        Ok(())
    }

    /// Decompress as many points as the `out` slice can hold
    ///
    /// # Note
    ///
    /// If the `out` slice contains more space than there are points
    /// the function will still decompress and thus and error will occur
    pub fn decompress_many(&mut self, out: &mut [u8]) -> std::io::Result<()> {
        for point in out.chunks_exact_mut(self.vlr.items_size() as usize) {
            self.decompress_one(point)?;
        }
        Ok(())
    }

    pub fn source_seek(&mut self, offset: u64) -> std::io::Result<()> {
        self.record_decompressor
            .get_mut()
            .seek(SeekFrom::Start(offset))?;
        self.reset_for_new_chunk();
        Ok(())
    }

    /// Seeks to the point designed by the index
    ///
    /// # Important
    ///
    /// Seeking in compressed data has a higher cost than non compressed data
    /// because the stream has to be moved to the start of the chunk
    /// and then we have to decompress points in the chunk until we reach the
    /// one we want.
    pub fn seek(&mut self, point_idx: u64) -> laz::Result<()> {
        let chunk_table = self.chunk_table.as_ref().ok_or(MissingChunkTable)?;

        let chunk_info = {
            let mut chunk_of_point = 0usize;
            let mut start_of_chunk = self.data_start;
            let mut tmp_count = 0;
            for entry in chunk_table {
                tmp_count += entry.point_count;
                if tmp_count >= point_idx {
                    break;
                }
                start_of_chunk += entry.byte_count;
                chunk_of_point += 1;
            }

            if point_idx > tmp_count {
                None
            } else {
                Some((chunk_of_point, start_of_chunk))
            }
        };

        if let Some((chunk_of_point, start_of_chunk)) = chunk_info {
            self.current_chunk = chunk_of_point;
            let delta = point_idx % chunk_table[self.current_chunk].point_count;
            if chunk_of_point == (chunk_table.len() - 1) {
                // the requested point fall into the last chunk,
                // but that does not mean that the point exists
                // so we have to be careful, we will do as we would normally,
                // but if we reach the chunk_table_offset that means the requested
                // point is out ouf bounds so will just seek to the end cf(the else in the if let below)
                // we do this to avoid decompressing data (ie the chunk table) thinking its a record
                let mut tmp_out = vec![0u8; self.record_decompressor.record_size()];
                self.record_decompressor
                    .get_mut()
                    .seek(SeekFrom::Start(start_of_chunk))?;

                self.reset_for_new_chunk();
                let offset_to_chunk_table = self.data_start
                    + self
                        .chunk_table
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .iter()
                        .map(|e| e.byte_count)
                        .sum::<u64>();

                for _i in 0..delta {
                    self.decompress_one(&mut tmp_out)?;
                    let current_pos = self.record_decompressor.get_mut().stream_position()?;

                    if current_pos >= offset_to_chunk_table {
                        self.record_decompressor.get_mut().seek(SeekFrom::End(0))?;
                        return Ok(());
                    }
                }
            } else {
                self.record_decompressor
                    .get_mut()
                    .seek(SeekFrom::Start(start_of_chunk))?;
                self.reset_for_new_chunk();
                let mut tmp_out = vec![0u8; self.record_decompressor.record_size()];

                for _i in 0..delta {
                    self.decompress_one(&mut tmp_out)?;
                }
            }
        } else {
            // the requested point it out of bounds (ie higher than the number of
            // points compressed)

            // Seek to the end so that the next call to decompress causes en error
            // like "Failed to fill whole buffer (seeking past end is allowed by the Seek Trait)
            self.record_decompressor.get_mut().seek(SeekFrom::End(0))?;
        }
        Ok(())
    }

    fn reset_for_new_chunk(&mut self) {
        self.chunk_points_read = 0;
        self.record_decompressor.reset();
        // we can safely unwrap here, as set_field would have failed in the ::new()
        self.record_decompressor
            .set_fields_from(self.vlr.items())
            .unwrap();
    }
}

impl<'a, R: Read + Seek + Send + 'a> laz::laszip::LazDecompressor for LasZipDecompressor<'a, R> {
    fn decompress_many(&mut self, points: &mut [u8]) -> laz::Result<()> {
        self.decompress_many(points)?;
        Ok(())
    }

    fn seek(&mut self, index: u64) -> laz::Result<()> {
        self.seek(index)?;
        Ok(())
    }
}
