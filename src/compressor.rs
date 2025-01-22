use byteorder::{LittleEndian, WriteBytesExt};
use laz::laszip::{ChunkTable, ChunkTableEntry, LazVlr};
use laz::record::{LayeredPointRecordCompressor, RecordCompressor};

use std::io::{Seek, SeekFrom, Write};

pub(crate) struct CopcCompressor<'a, W: Write + Seek + 'a> {
    vlr: LazVlr,
    record_compressor: LayeredPointRecordCompressor<'a, W>,
    /// Position where LasZipCompressor started
    start_pos: u64,
    /// Position where the current chunk started
    chunk_start_pos: u64,
    /// Entry for the chunk we are currently compressing
    current_chunk_entry: ChunkTableEntry,
    /// Table of chunks written so far
    chunk_table: ChunkTable,
}

impl<'a, W: Write + Seek + 'a> CopcCompressor<'a, W> {
    /// Creates a compressor using the provided vlr.
    pub(crate) fn new(write: W, vlr: LazVlr) -> crate::Result<Self> {
        let mut record_compressor = LayeredPointRecordCompressor::new(write);
        record_compressor.set_fields_from(vlr.items())?;
        let stream = record_compressor.get_mut();

        let start_pos = stream.stream_position()?;
        // reserve 8 bytes for the offset to the chunk table
        stream.write_i64::<LittleEndian>(-1)?;

        Ok(Self {
            vlr,
            record_compressor,
            chunk_start_pos: start_pos + 8, // size of the written i64
            start_pos,
            chunk_table: ChunkTable::default(),
            current_chunk_entry: ChunkTableEntry::default(),
        })
    }

    /// Compress a chunk
    pub(crate) fn compress_chunk<Chunk: AsRef<[u8]>>(
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

        // update the chunk table
        let current_pos = self.record_compressor.get_mut().stream_position()?;
        self.current_chunk_entry.byte_count = current_pos - self.chunk_start_pos;
        self.chunk_table.push(self.current_chunk_entry);

        // store chunk entry and chunk start pos for returning
        let old_chunk_start_pos = self.chunk_start_pos;
        let written_chunk_entry = self.current_chunk_entry;

        // reset the chunk
        self.chunk_start_pos = current_pos;
        self.current_chunk_entry = ChunkTableEntry::default();

        Ok((written_chunk_entry, old_chunk_start_pos))
    }

    /// Must be called when you have compressed all your points.
    pub(crate) fn done(&mut self) -> std::io::Result<()> {
        self.record_compressor.done()?;

        // update the offset to the chunk table
        let stream = self.record_compressor.get_mut();
        let start_of_chunk_table_pos = stream.stream_position()?;
        stream.seek(SeekFrom::Start(self.start_pos))?;
        stream.write_i64::<LittleEndian>(start_of_chunk_table_pos as i64)?;
        stream.seek(SeekFrom::Start(start_of_chunk_table_pos))?;

        self.chunk_table.write_to(stream, &self.vlr)
    }

    pub(crate) fn get_mut(&mut self) -> &mut W {
        self.record_compressor.get_mut()
    }
}
