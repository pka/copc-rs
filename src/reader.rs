use crate::copc::{CopcInfo, Page};
use crate::decompressor::LasZipDecompressor;
use crate::header::Header;
use crate::vlr::Vlr;
use las::{Transform, Vector};
use laz::LazVlr;
use std::io::{Cursor, Read, Seek, SeekFrom};

/// COPC file reader
pub struct CopcReader<R> {
    src: R,
    las_header: Header,
    copc_info: CopcInfo,
    laszip_vlr: Option<LazVlr>,
    projection_vlr: Option<Vlr>,
    pages: Vec<Page>,
}

impl<R: Read + Seek + Send> CopcReader<R> {
    /// Setup by reading LAS header and LasZip VRLs
    pub fn create(mut src: R) -> std::io::Result<Self> {
        let las_header = Header::read_from(&mut src).unwrap();
        let copc_vlr = Vlr::read_from(&mut src).unwrap();
        if copc_vlr.user_id().as_str() != "copc" || copc_vlr.record_id != 1 {
            panic!("format error");
        }
        let copc_info = CopcInfo::read_from(Cursor::new(copc_vlr.data))?;
        // dbg!(&copc_info);
        // dbg!(&las_header);

        let mut reader = CopcReader {
            src,
            las_header,
            copc_info,
            laszip_vlr: None,
            projection_vlr: None,
            pages: Vec::new(),
        };

        for _i in 0..reader.las_header.number_of_variable_length_records - 1 {
            let vlr = Vlr::read_from(&mut reader.src).unwrap();
            // dbg!(&vlr);
            match (vlr.user_id().as_str(), vlr.record_id) {
                ("laszip encoded", 22204) => {
                    reader.laszip_vlr = Some(LazVlr::read_from(vlr.data.as_slice()).unwrap())
                }
                // ("copc", 1000) => reader.hierarchy_vlr = Some(vlr),
                ("LASF_Projection", 2112) => reader.projection_vlr = Some(vlr),
                (user_id, record_id) => {
                    eprintln!("Ignoring VLR {user_id}/{record_id}")
                }
            }
        }

        Ok(reader)
    }

    /// Point iterator for selected level
    pub fn points(&mut self, level: i32) -> laz::Result<PointIter<R>> {
        // Read root hierarchy page
        self.src
            .seek(SeekFrom::Start(self.copc_info.root_hier_offset))?;
        let page = Page::read_from(&mut self.src, self.copc_info.root_hier_size)?;
        self.pages.push(page);

        let page_entry = self.pages[level as usize].entries[0]; // FIXME
        self.src.seek(SeekFrom::Start(page_entry.offset))?;

        let transforms = Vector {
            x: Transform {
                scale: self.las_header.x_scale_factor,
                offset: self.las_header.x_offset,
            },
            y: Transform {
                scale: self.las_header.y_scale_factor,
                offset: self.las_header.y_offset,
            },
            z: Transform {
                scale: self.las_header.z_scale_factor,
                offset: self.las_header.z_offset,
            },
        };

        let laz_vlr = self
            .laszip_vlr
            .as_ref()
            .expect("Expected a laszip VLR for laz file");
        // dbg!(&laz_vlr);
        let decompressor =
            LasZipDecompressor::new(&mut self.src, Some(page_entry.offset), laz_vlr)?;

        let point_format =
            las::point::Format::new(self.las_header.point_data_record_format).unwrap();
        let point_size = self.las_header.point_data_record_length as usize;
        let point = vec![0u8; point_size];
        let num_points_left = page_entry.point_count as usize;

        Ok(PointIter {
            point_format,
            transforms,
            decompressor,
            point,
            num_points_left,
        })
    }
}

/// LasZip point iterator
pub struct PointIter<'a, R: Read + Seek + Send> {
    point_format: las::point::Format,
    transforms: Vector<Transform>,
    decompressor: LasZipDecompressor<'a, &'a mut R>,
    point: Vec<u8>,
    num_points_left: usize,
}

impl<'a, R: Read + Seek + Send> Iterator for PointIter<'a, R> {
    type Item = las::point::Point;

    fn next(&mut self) -> Option<Self::Item> {
        if self.num_points_left == 0 {
            return None;
        }
        self.decompressor
            .decompress_one(self.point.as_mut_slice())
            .unwrap();
        let raw_point =
            las::raw::Point::read_from(self.point.as_slice(), &self.point_format).unwrap();
        let point = las::point::Point::new(raw_point, &self.transforms);

        self.num_points_left -= 1;
        Some(point)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.num_points_left, Some(self.num_points_left))
    }
}
