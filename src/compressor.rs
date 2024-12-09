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
    pub fn new(mut write: W, vlr: LazVlr) -> laz::Result<Self> {
        let start_pos = write.stream_position()?;
        Ok(Self {
            vlr,
            record_compressor: LayeredPointRecordCompressor::new(write),
            chunk_start_pos: 0,
            start_pos,
            chunk_table: ChunkTable::default(),
            current_chunk_entry: ChunkTableEntry::default(),
        })
    }

    /// Compress the point and write the compressed data to the destination given when
    /// the compressor was constructed
    ///
    /// The data is written in the buffer is expected to be exactly
    /// as it would have been in a LAS File, that is:
    ///
    /// - The fields/dimensions are in the same order as the LAS spec says
    /// - The data in the buffer is in Little Endian order
    pub fn compress_one(&mut self, input: &[u8]) -> std::io::Result<()> {
        if self.chunk_start_pos == 0 {
            self.reserve_offset_to_chunk_table()?;
        }

        self.record_compressor.compress_next(input)?;
        self.current_chunk_entry.point_count += 1;
        Ok(())
    }

    /// Compress all the points contained in the input slice
    pub fn compress_many(&mut self, input: &[u8]) -> std::io::Result<()> {
        for point in input.chunks_exact(self.vlr.items_size() as usize) {
            self.compress_one(point)?;
        }
        Ok(())
    }

    /// Compress a single chunk
    pub fn compress_chunk<Chunk>(&mut self, chunk: Chunk) -> std::io::Result<(ChunkTableEntry, u64)>
    where
        Chunk: AsRef<[u8]>,
    {
        let chunk_points = chunk.as_ref();
        self.compress_many(chunk_points)?;
        self.finish_current_chunk()
    }

    /// Must be called when you have compressed all your points.
    pub fn done(&mut self) -> std::io::Result<()> {
        if self.chunk_start_pos == 0 {
            self.reserve_offset_to_chunk_table()?;
        }
        self.record_compressor.done()?;
        self.update_chunk_table()?;
        let stream = self.record_compressor.get_mut();

        // updates the first 8 bytes of the compressed block
        // which describes the offset to the chunk table
        // the bytes are assumed to be reserved ie no point data written there
        // also assumes the current pos is at the chunk table start
        let start_of_chunk_table_pos = stream.stream_position()?;
        stream.seek(SeekFrom::Start(self.start_pos))?;
        stream.write_i64::<LittleEndian>(start_of_chunk_table_pos as i64)?;
        stream.seek(SeekFrom::Start(start_of_chunk_table_pos))?;

        self.chunk_table.write_to(stream, &self.vlr)?;
        Ok(())
    }

    /// Finish the current chunk.
    ///
    /// All points compressed with the previous calls to [compress_one]
    /// will form one chunk. And the subsequent calls to [compress_one]
    /// will form a new chunk.
    ///
    /// [compress_one]: Self::compress_one
    pub fn finish_current_chunk(&mut self) -> std::io::Result<(ChunkTableEntry, u64)> {
        self.record_compressor.done()?;
        self.record_compressor.reset();
        self.record_compressor
            .set_fields_from(self.vlr.items())
            .unwrap();
        let old_chunk_start_pos = self.chunk_start_pos;

        self.update_chunk_table()?;
        let written_chunk_entry = self.current_chunk_entry;
        self.current_chunk_entry = ChunkTableEntry::default();
        Ok((written_chunk_entry, old_chunk_start_pos + self.start_pos))
    }

    /// The 8 first bytes of the laz data block is the offset to the chunk table
    /// This fn reserves and prepares the offset to chunk table that will be
    /// updated when [done] is called.
    ///
    /// This method will automatically be called on the first point being compressed,
    /// but for some scenarios, manually calling this might be useful.
    ///
    /// [done]: Self::done
    fn reserve_offset_to_chunk_table(&mut self) -> std::io::Result<()> {
        debug_assert_eq!(self.chunk_start_pos, 0);
        let stream = self.record_compressor.get_mut();
        self.start_pos = stream.stream_position()?;
        stream.write_i64::<LittleEndian>(-1)?;
        self.chunk_start_pos = self.start_pos + 8;
        Ok(())
    }

    pub fn get_mut(&mut self) -> &mut W {
        self.record_compressor.get_mut()
    }

    #[inline]
    fn update_chunk_table(&mut self) -> std::io::Result<()> {
        let current_pos = self.record_compressor.get_mut().stream_position()?;
        self.current_chunk_entry.byte_count = current_pos - self.chunk_start_pos;
        self.chunk_start_pos = current_pos;
        self.chunk_table.push(self.current_chunk_entry);
        Ok(())
    }
}
