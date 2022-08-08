use crate::copc::{CopcInfo, Page};
use crate::decompressor::LasZipDecompressor;
use crate::header::Header;
use crate::vlr::Vlr;
use las::{Transform, Vector};
use laz::LazVlr;
use std::io::{Cursor, Read, Seek, SeekFrom};

pub struct CopcReader {
    las_header: Header,
    copc_info: CopcInfo,
    laszip_vlr: Option<LazVlr>,
    projection_vlr: Option<Vlr>,
    hierarchy_vlr: Option<Vlr>,
    transforms: Vector<Transform>,
    pages: Vec<Page>,
    num_points_per_iter: usize,
}

impl CopcReader {
    pub fn create<R: Read + Seek>(src: &mut R) -> std::io::Result<Self> {
        let las_header = Header::read_from(src).unwrap();
        let copc_vlr = Vlr::read_from(src).unwrap();
        if copc_vlr.user_id().as_str() != "copc" || copc_vlr.record_id != 1 {
            panic!("format error");
        }
        let copc_info = CopcInfo::read_from(Cursor::new(copc_vlr.data))?;
        // dbg!(&copc_info);
        // dbg!(&las_header);

        let transforms = Vector {
            x: Transform {
                scale: las_header.x_scale_factor,
                offset: las_header.x_offset,
            },
            y: Transform {
                scale: las_header.y_scale_factor,
                offset: las_header.y_offset,
            },
            z: Transform {
                scale: las_header.z_scale_factor,
                offset: las_header.z_offset,
            },
        };

        let mut reader = CopcReader {
            las_header,
            copc_info,
            laszip_vlr: None,
            projection_vlr: None,
            hierarchy_vlr: None,
            transforms,
            pages: Vec::new(),
            num_points_per_iter: 100,
        };

        for _i in 0..reader.las_header.number_of_variable_length_records - 1 {
            let vlr = Vlr::read_from(src).unwrap();
            dbg!(&vlr);
            match (vlr.user_id().as_str(), vlr.record_id) {
                ("laszip encoded", 22204) => {
                    reader.laszip_vlr = Some(LazVlr::read_from(vlr.data.as_slice()).unwrap())
                }
                ("copc", 1000) => reader.hierarchy_vlr = Some(vlr),
                ("LASF_Projection", 2112) => reader.projection_vlr = Some(vlr),
                (user_id, record_id) => {
                    eprintln!("Ignoring VLR {user_id}/{record_id}")
                }
            }
        }

        if let Some(ref hierarchy_vlr) = reader.hierarchy_vlr {
            //src.seek(SeekFrom::Start(copc_info.root_hier_offset))?;
            let _root_page =
                Page::read_from(Cursor::new(&hierarchy_vlr.data), copc_info.root_hier_size)?;
        }

        // Read root hierarchy page
        src.seek(SeekFrom::Start(reader.copc_info.root_hier_offset))?;
        let page = Page::read_from(src, reader.copc_info.root_hier_size)?;
        reader.pages.push(page);

        Ok(reader)
    }

    pub fn read_points<R: Read + Seek + Send>(&self, src: &mut R) -> laz::Result<()> {
        let laz_vlr = self
            .laszip_vlr
            .as_ref()
            .expect("Expected a laszip VLR for laz file");
        // dbg!(&laz_vlr);

        let point_format =
            las::point::Format::new(self.las_header.point_data_record_format).unwrap();
        let point_size = self.las_header.point_data_record_length as usize;

        // src.seek(SeekFrom::Start(las_header.offset_to_point_data as u64))?;
        // let mut decompressor = LasZipDecompressor::new(&mut src, None, laz_vlr)?;
        let page_entry = self.pages[0].entries[0];
        dbg!(page_entry);
        src.seek(SeekFrom::Start(page_entry.offset))?;
        let mut decompressor = LasZipDecompressor::new(src, Some(page_entry.offset), laz_vlr)?;

        let max_points = 5;
        let mut points = vec![0u8; point_size * self.num_points_per_iter];
        let mut num_points_left = self.las_header.number_of_points().min(max_points) as usize;
        while num_points_left > 0 {
            let num_points_to_read = self.num_points_per_iter.min(num_points_left);

            let points_batch = &mut points[..num_points_to_read * point_size];
            decompressor.decompress_many(points_batch)?;

            for i in 0..num_points_to_read {
                let point_data = &points_batch[i * point_size..];
                let raw_point = las::raw::Point::read_from(point_data, &point_format).unwrap();
                let point = las::point::Point::new(raw_point, &self.transforms);
                println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
            }

            num_points_left -= num_points_to_read;
        }
        // Display full data of first point
        let point_data = &points[0..point_size];
        let raw_point = las::raw::Point::read_from(point_data, &point_format).unwrap();
        dbg!(&raw_point);
        let point = las::point::Point::new(raw_point, &self.transforms);
        dbg!(&point);
        Ok(())
    }
}
