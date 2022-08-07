use crate::copc::{CopcInfo, Page};
use crate::header::Header;
use byteorder::{LittleEndian, ReadBytesExt};
use laz::{LasZipDecompressor, LazVlr};
use std::fmt;
use std::io::{Cursor, Read, Seek};

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

pub struct CopcHeaders {
    pub las_header: Header,
    pub copc_info: CopcInfo,
    pub laszip_vlr: Option<LazVlr>,
    pub projection_vlr: Option<Vlr>,
    pub hierarchy_vlr: Option<Vlr>,
}

impl CopcHeaders {
    pub fn read_from<R: Read + Seek>(src: &mut R) -> std::io::Result<Self> {
        let las_header = Header::read_from(src).unwrap();
        let copc_vlr = Vlr::read_from(src).unwrap();
        if copc_vlr.user_id().as_str() != "copc" || copc_vlr.record_id != 1 {
            panic!("format error");
        }
        let copc_info = CopcInfo::read_from(Cursor::new(copc_vlr.data))?;
        dbg!(&copc_info);
        let mut headers = CopcHeaders {
            las_header,
            copc_info,
            laszip_vlr: None,
            projection_vlr: None,
            hierarchy_vlr: None,
        };
        for _i in 0..headers.las_header.number_of_variable_length_records - 1 {
            let vlr = Vlr::read_from(src).unwrap();
            dbg!(&vlr);
            match (vlr.user_id().as_str(), vlr.record_id) {
                ("laszip encoded", 22204) => {
                    headers.laszip_vlr = Some(LazVlr::read_from(vlr.data.as_slice()).unwrap())
                }
                ("copc", 1000) => headers.hierarchy_vlr = Some(vlr),
                ("LASF_Projection", 2112) => headers.projection_vlr = Some(vlr),
                (user_id, record_id) => {
                    eprintln!("Ignoring VLR {user_id}/{record_id}")
                }
            }
        }

        if let Some(ref hierarchy_vlr) = headers.hierarchy_vlr {
            //src.seek(SeekFrom::Start(copc_info.root_hier_offset))?;
            let _root_page =
                Page::read_from(Cursor::new(&hierarchy_vlr.data), copc_info.root_hier_size)?;
        }
        Ok(headers)
    }
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
