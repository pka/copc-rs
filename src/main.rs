mod copc;
mod file;
mod header;

use crate::file::CopcHeaders;
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
    let laz_header = copc_headers.las_header;
    let laz_vlr = copc_headers
        .laszip_vlr
        .expect("Expected a laszip VLR for laz file");

    laz_file.seek(SeekFrom::Start(laz_header.offset_to_point_data as u64))?;

    let num_points_per_iter = 100;

    let point_size = laz_header.point_data_record_length as usize;
    let mut points = vec![0u8; point_size * num_points_per_iter];
    let mut num_points_left = laz_header.number_of_point_records as usize;

    let mut decompressor = LasZipDecompressor::new(&mut laz_file, laz_vlr.clone())?;
    while num_points_left > 0 {
        let num_points_to_read = num_points_per_iter.min(num_points_left);

        let points_batch = &mut points[..num_points_to_read * point_size];
        decompressor.decompress_many(points_batch)?;

        for i in 0..num_points_to_read - 1 {
            let point_data = &points_batch[i * point_size..];
            let raw_point =
                las::raw::Point::read_from(point_data, &las::point::Format::new(10).unwrap())
                    .unwrap();
            let point = las::point::Point::new(raw_point, &Default::default());
            // println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
        }

        num_points_left -= num_points_to_read;
    }
    let point_data = &points[0..point_size * 2];
    // dbg!(point_data);
    dbg!(laz_header, laz_vlr);
    let raw_point =
        las::raw::Point::read_from(point_data, &las::point::Format::new(10).unwrap()).unwrap();
    dbg!(&raw_point);
    let point = las::point::Point::new(raw_point, &Default::default());
    dbg!(&point);

    Ok(())
}
