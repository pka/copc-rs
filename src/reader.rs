use crate::copc::{CopcInfo, Page};
use crate::header::Header;
use crate::vlr::Vlr;
use laz::LazVlr;
use std::io::{Cursor, Read, Seek};

pub struct CopcReader {
    pub las_header: Header,
    pub copc_info: CopcInfo,
    pub laszip_vlr: Option<LazVlr>,
    pub projection_vlr: Option<Vlr>,
    pub hierarchy_vlr: Option<Vlr>,
}

impl CopcReader {
    pub fn create<R: Read + Seek>(src: &mut R) -> std::io::Result<Self> {
        let las_header = Header::read_from(src).unwrap();
        let copc_vlr = Vlr::read_from(src).unwrap();
        if copc_vlr.user_id().as_str() != "copc" || copc_vlr.record_id != 1 {
            panic!("format error");
        }
        let copc_info = CopcInfo::read_from(Cursor::new(copc_vlr.data))?;
        let mut headers = CopcReader {
            las_header,
            copc_info,
            laszip_vlr: None,
            projection_vlr: None,
            hierarchy_vlr: None,
        };
        dbg!(headers.las_header.number_of_variable_length_records);
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
