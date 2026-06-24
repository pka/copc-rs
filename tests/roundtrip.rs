#![cfg(feature = "writer")]

//! Regression test for the build against current `las`/`laz`, plus a basic
//! write -> read round-trip through copc-rs's own writer and reader.
//!
//! With copc-rs's stale `laz = "0.9"` pin, a fresh dependency resolution selects
//! `las 0.9.11` (which uses `laz 0.12`) while copc-rs pulls `laz 0.9`, so
//! `header.laz_vlr()` is a different `LazVlr` type and the crate does not compile.
//! This test therefore fails to *build* before the fix and passes after it.

use std::io::Cursor;

use copc_rs::{BoundsSelection, CopcReader, CopcWriter, LodSelection};
use las::point::Format;
use las::{Builder, Point, Transform, Vector, Vlr};

const WKT: &[u8] = b"GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563]],PRIMEM[\"Greenwich\",0],UNIT[\"degree\",0.0174532925199433]]";

fn header_with_bounds() -> las::Header {
    let mut b = Builder::from((1u8, 4u8));
    b.point_format = Format::new(6).unwrap();
    let t = Transform {
        scale: 0.01,
        offset: 0.0,
    };
    b.transforms = Vector { x: t, y: t, z: t };
    // raw round-trip only to stamp non-degenerate bounds (Builder has no bounds field)
    let mut raw = b.into_header().unwrap().into_raw().unwrap();
    raw.min_x = 0.0;
    raw.max_x = 100.0;
    raw.min_y = 0.0;
    raw.max_y = 100.0;
    raw.min_z = 0.0;
    raw.max_z = 100.0;
    // VLRs are not part of raw::Header, so add the mandatory WKT CRS VLR afterwards.
    let mut b2 = Builder::new(raw).unwrap();
    b2.vlrs.push(Vlr {
        user_id: "LASF_Projection".into(),
        record_id: 2112,
        description: String::new(),
        data: WKT.to_vec(),
    });
    b2.into_header().unwrap()
}

#[test]
fn writes_and_reads_back_all_points() {
    let pts: Vec<Point> = (0..500)
        .map(|i| {
            let f = (i % 100) as f64;
            Point {
                x: f,
                y: f,
                z: f,
                gps_time: Some(1000.0 + i as f64),
                ..Default::default()
            }
        })
        .collect();
    let n = pts.len();

    let mut buf = Cursor::new(Vec::<u8>::new());
    {
        let mut w = CopcWriter::new(&mut buf, header_with_bounds(), -1, -1).unwrap();
        w.write(pts, n as i32).unwrap();
    }

    buf.set_position(0);
    let mut r = CopcReader::new(buf).unwrap();
    let read = r
        .points(LodSelection::All, BoundsSelection::All)
        .unwrap()
        .count();
    assert_eq!(read, n, "every written point must round-trip");
}
