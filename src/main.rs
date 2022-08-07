mod copc;
mod file;
mod header;

use crate::copc::Page;
use crate::file::CopcHeaders;
use las::{Transform, Vector};
use laz::laszip::LasZipDecompressor;
use std::env;
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};

fn main() -> laz::Result<()> {
    let lazfn = env::args()
        .nth(1)
        .unwrap_or("tests/data/autzen.laz".to_string());
    let mut laz_file = BufReader::new(File::open(lazfn)?);

    let copc_headers = CopcHeaders::read_from(&mut laz_file)?;
    let las_header = copc_headers.las_header;
    let laz_vlr = copc_headers
        .laszip_vlr
        .expect("Expected a laszip VLR for laz file");
    // dbg!(&copc_headers.copc_info);
    // dbg!(&las_header);
    // dbg!(&laz_vlr);

    let point_format = las::point::Format::new(las_header.point_data_record_format).unwrap();
    let point_size = las_header.point_data_record_length as usize;
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

    // Read root hierarchy page
    laz_file.seek(SeekFrom::Start(copc_headers.copc_info.root_hier_offset))?;
    let page = Page::read_from(&mut laz_file, copc_headers.copc_info.root_hier_size)?;
    let page_entry = page.entries[0];
    dbg!(page_entry);
    // Point reading by page not supported yet. LasZipDecompressor always reads Chunk table.
    // laz_file.seek(SeekFrom::Start(page_entry.offset))?;

    laz_file.seek(SeekFrom::Start(las_header.offset_to_point_data as u64))?;

    let num_points_per_iter = 100;
    let max_points = 5;
    let mut points = vec![0u8; point_size * num_points_per_iter];
    let mut num_points_left = las_header.number_of_points().min(max_points) as usize;

    let mut decompressor = LasZipDecompressor::new(&mut laz_file, laz_vlr)?;
    while num_points_left > 0 {
        let num_points_to_read = num_points_per_iter.min(num_points_left);

        let points_batch = &mut points[..num_points_to_read * point_size];
        decompressor.decompress_many(points_batch)?;

        for i in 0..num_points_to_read {
            let point_data = &points_batch[i * point_size..];
            let raw_point = las::raw::Point::read_from(point_data, &point_format).unwrap();
            let point = las::point::Point::new(raw_point, &transforms);
            println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
        }

        num_points_left -= num_points_to_read;
    }
    // Display full data of first point
    let point_data = &points[0..point_size];
    let raw_point = las::raw::Point::read_from(point_data, &point_format).unwrap();
    dbg!(&raw_point);
    let point = las::point::Point::new(raw_point, &transforms);
    dbg!(&point);

    Ok(())
}
