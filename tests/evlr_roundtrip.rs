//! Regression test: an EVLR already present in the input header must not corrupt
//! the written COPC's EVLR offset/count, which the reader uses to locate the EPT
//! hierarchy. Before the fix, the header's `start_of_first_evlr`/`number_of_evlrs`
//! were updated on a discarded local copy (the input's stale values were written),
//! so the reader could not find the COPC hierarchy EVLR.

use std::io::Cursor;

use copc_rs::{BoundsSelection, CopcReader, CopcWriter, LodSelection};
use las::point::Format;
use las::{Builder, Point, Transform, Vector, Vlr};

const WKT: &[u8] = b"GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563]],PRIMEM[\"Greenwich\",0],UNIT[\"degree\",0.0174532925199433]]";

fn header_with_extra_evlr() -> las::Header {
    let mut b = Builder::from((1u8, 4u8));
    b.point_format = Format::new(6).unwrap();
    let t = Transform { scale: 0.01, offset: 0.0 };
    b.transforms = Vector { x: t, y: t, z: t };
    let mut raw = b.into_header().unwrap().into_raw().unwrap();
    raw.min_x = 0.0;
    raw.max_x = 100.0;
    raw.min_y = 0.0;
    raw.max_y = 100.0;
    raw.min_z = 0.0;
    raw.max_z = 100.0;
    let mut b2 = Builder::new(raw).unwrap();
    b2.vlrs.push(Vlr {
        user_id: "LASF_Projection".into(),
        record_id: 2112,
        description: String::new(),
        data: WKT.to_vec(),
    });
    // A pre-existing EVLR in the input header -- the path that triggered the bug.
    b2.evlrs.push(Vlr {
        user_id: "TESTVEND".into(),
        record_id: 4242,
        description: String::new(),
        data: vec![1, 2, 3, 4, 5, 6, 7, 8],
    });
    b2.into_header().unwrap()
}

#[test]
fn pre_existing_input_evlr_keeps_hierarchy_readable() {
    let pts: Vec<Point> = (0..2000)
        .map(|i| {
            let f = (i % 100) as f64;
            Point { x: f, y: f, z: f, gps_time: Some(1000.0 + i as f64), ..Default::default() }
        })
        .collect();
    let n = pts.len();

    let mut buf = Cursor::new(Vec::<u8>::new());
    {
        let mut w = CopcWriter::new(&mut buf, header_with_extra_evlr(), -1, -1).unwrap();
        w.write(pts, n as i32).unwrap();
    }

    buf.set_position(0);
    let mut r = CopcReader::new(buf).expect("reader must open a COPC written from an EVLR-bearing header");
    assert!(r.num_entries() > 0, "COPC hierarchy must be present/readable");
    let read = r
        .points(LodSelection::All, BoundsSelection::All)
        .unwrap()
        .count();
    assert_eq!(read, n, "all points must round-trip");
}
