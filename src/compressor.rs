use byteorder::{LittleEndian, WriteBytesExt};
use laz::laszip::{ChunkTable, ChunkTableEntry, LazVlr};
use laz::record::{LayeredPointRecordCompressor, RecordCompressor};

use std::io::{Seek, SeekFrom, Write};

pub struct CopcCompressor<'a, W: Write + Seek + 'a> {
    vlr: LazVlr,
    record_compressor: LayeredPointRecordCompressor<'a, W>,
    /// Position where LasZipCompressor started
    start_pos: u64,
    /// Table of chunks written so far
    chunk_table: ChunkTable,
    /// Entry for the chunk we are currently compressing
    current_chunk_entry: ChunkTableEntry,
    /// Position (offset from start_pos)
    /// where the current chunk started
    chunk_start_pos: u64,
}

impl<'a, W: Write + Seek + 'a> CopcCompressor<'a, W> {
    /// Creates a compressor using the provided vlr.
    pub fn new(write: W, vlr: LazVlr) -> crate::Result<Self> {
        let mut record_compressor = LayeredPointRecordCompressor::new(write);
        record_compressor.set_fields_from(vlr.items())?;
        let stream = record_compressor.get_mut();

        let start_pos = stream.stream_position()?;
        stream.write_i64::<LittleEndian>(-1)?;

        Ok(Self {
            vlr,
            record_compressor,
            chunk_start_pos: start_pos + 8,
            start_pos,
            chunk_table: ChunkTable::default(),
            current_chunk_entry: ChunkTableEntry::default(),
        })
    }

    /// Compress a single chunk
    /// Compresses every point in the chunk and
    /// writes the compressed data to the destination given when
    /// the compressor was constructed
    ///
    /// The data is written in the buffer is expected to be exactly
    /// as it would have been in a LAS File, that is:
    ///
    /// - The fields/dimensions are in the same order as the LAS spec says
    /// - The data in the buffer is in Little Endian order
    pub fn compress_chunk<Chunk: AsRef<[u8]>>(
        &mut self,
        chunk: Chunk,
    ) -> std::io::Result<(ChunkTableEntry, u64)> {
        for point in chunk.as_ref().chunks_exact(self.vlr.items_size() as usize) {
            self.record_compressor.compress_next(point)?;
            self.current_chunk_entry.point_count += 1;
        }

        // finish the chunk
        self.record_compressor.done()?;
        self.record_compressor.reset();
        self.record_compressor
            .set_fields_from(self.vlr.items())
            .unwrap();

        let old_chunk_start_pos = self.chunk_start_pos;

        self.update_chunk_table()?;
        let written_chunk_entry = self.current_chunk_entry;
        self.current_chunk_entry = ChunkTableEntry::default();
        Ok((written_chunk_entry, old_chunk_start_pos))
    }

    /// Must be called when you have compressed all your points.
    pub fn done(&mut self) -> std::io::Result<()> {
        self.record_compressor.done()?;

        // updates the first 8 bytes of the compressed block
        // which describes the offset to the chunk table
        // the bytes are assumed to be reserved ie no point data written there
        // also assumes the current pos is at the chunk table start
        let stream = self.record_compressor.get_mut();
        let start_of_chunk_table_pos = stream.stream_position()?;
        stream.seek(SeekFrom::Start(self.start_pos))?;
        stream.write_i64::<LittleEndian>(start_of_chunk_table_pos as i64)?;
        stream.seek(SeekFrom::Start(start_of_chunk_table_pos))?;

        self.chunk_table.write_to(stream, &self.vlr)
    }

    pub fn get_mut(&mut self) -> &mut W {
        self.record_compressor.get_mut()
    }

    #[inline]
    fn update_chunk_table(&mut self) -> std::io::Result<()> {
        let current_pos = self.record_compressor.get_mut().stream_position()?;
        self.current_chunk_entry.byte_count = current_pos - self.chunk_start_pos;
        self.chunk_table.push(self.current_chunk_entry);

        // reset the chunk start pos
        self.chunk_start_pos = current_pos;
        Ok(())
    }
}
