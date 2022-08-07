mod file;

use crate::file::read_header_and_vlrs;
use laz::laszip::LasZipDecompressor;
use std::fs::File;
use std::io::BufReader;

fn main() -> laz::Result<()> {
    let mut laz_file = BufReader::new(File::open("tests/data/autzen.laz")?);
    let (laz_header, laz_vlr) = read_header_and_vlrs(&mut laz_file)?;
    let laz_vlr = laz_vlr.expect("Expected a laszip VLR for laz file");

    let num_points_per_iter = 100;

    let point_size = laz_header.point_size as usize;
    let mut points = vec![0u8; point_size * num_points_per_iter];
    let mut num_points_left = laz_header.num_points as usize;

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
            println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
        }

        num_points_left -= num_points_to_read;
    }
    let point_data = &points[0..point_size * 2];
    dbg!(point_data);
    dbg!(laz_header, laz_vlr);
    let raw_point =
        las::raw::Point::read_from(point_data, &las::point::Format::new(10).unwrap()).unwrap();
    dbg!(&raw_point);
    let point = las::point::Point::new(raw_point, &Default::default());
    dbg!(&point);

    Ok(())
}
