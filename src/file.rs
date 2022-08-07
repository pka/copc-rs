use crate::header::Header;
use byteorder::{LittleEndian, ReadBytesExt};
use laz::{LasZipDecompressor, LazVlr};
use std::fmt;
use std::io::{Read, Seek, SeekFrom};

pub struct Vlr {
    user_id: [u8; 16],
    record_id: u16,
    description: [u8; 32],
    data: Vec<u8>,
}

impl Vlr {
    pub fn read_from<R: Read>(src: &mut R) -> std::io::Result<Self> {
        src.read_u16::<LittleEndian>()?; // reserved
        let mut user_id = [0u8; 16];
        src.read_exact(&mut user_id)?;

        let record_id = src.read_u16::<LittleEndian>()?;
        let record_length = src.read_u16::<LittleEndian>()?;

        let mut description = [0u8; 32];
        src.read_exact(&mut description)?;

        let mut data = Vec::<u8>::new();
        data.resize(record_length as usize, 0);
        src.read_exact(&mut data)?;

        Ok(Self {
            user_id,
            record_id,
            description,
            data,
        })
    }
    pub fn user_id(&self) -> String {
        String::from_utf8_lossy(&self.user_id)
            .trim_end_matches(|c| c as u8 == 0)
            .to_string()
    }
    pub fn description(&self) -> String {
        String::from_utf8_lossy(&self.description)
            .trim_end_matches(|c| c as u8 == 0)
            .to_string()
    }
}

impl fmt::Debug for Vlr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vlr")
            .field("user_id", &self.user_id())
            .field("record_id", &self.record_id)
            .field("description", &self.description())
            .field("data", &format_args!("[u8; {}]", self.data.len()))
            .finish()
    }
}

pub fn read_vlrs_and_get_laszip_vlr<R: Read>(src: &mut R, header: &Header) -> Option<LazVlr> {
    let mut laszip_vlr = None;
    for _i in 0..header.number_of_variable_length_records {
        let vlr = Vlr::read_from(src).unwrap();
        dbg!(&vlr);
        if vlr.record_id == 22204
            && String::from_utf8_lossy(&vlr.user_id).trim_end_matches(|c| c as u8 == 0)
                == "laszip encoded"
        {
            laszip_vlr = Some(LazVlr::read_from(vlr.data.as_slice()).unwrap());
        }
    }
    laszip_vlr
}

pub fn read_header_and_vlrs<R: Read + Seek>(
    src: &mut R,
) -> std::io::Result<(Header, Option<LazVlr>)> {
    let hdr = Header::read_from(src).unwrap();
    src.seek(SeekFrom::Start(hdr.header_size as u64))?;
    let laz_vlr = read_vlrs_and_get_laszip_vlr(src, &hdr);
    src.seek(SeekFrom::Start(hdr.offset_to_point_data as u64))?;
    Ok((hdr, laz_vlr))
}

const IS_COMPRESSED_MASK: u8 = 0x80;
fn is_point_format_compressed(point_format_id: u8) -> bool {
    point_format_id & IS_COMPRESSED_MASK == IS_COMPRESSED_MASK
}
pub fn point_format_id_compressed_to_uncompressd(point_format_id: u8) -> u8 {
    point_format_id & 0x3f
}

fn point_format_id_uncompressed_to_compressed(point_format_id: u8) -> u8 {
    point_format_id | 0x80
}

pub trait LasPointReader {
    fn read_next_into(&mut self, buffer: &mut [u8]) -> std::io::Result<()>;
}

struct RawPointReader<R: Read> {
    src: R,
}

impl<R: Read> LasPointReader for RawPointReader<R> {
    fn read_next_into(&mut self, buffer: &mut [u8]) -> std::io::Result<()> {
        self.src.read_exact(buffer)
    }
}

impl<'a, R: Read + Seek + Send> LasPointReader for LasZipDecompressor<'a, R> {
    fn read_next_into(&mut self, buffer: &mut [u8]) -> std::io::Result<()> {
        self.decompress_one(buffer)
    }
}
