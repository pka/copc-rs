//! Regression test: dropping a `CopcWriter` must never panic.
//!
//! Before the fix, `Drop` called `self.close().expect(...)`, so a writer that was
//! constructed but never written (or whose write errored before close) panicked on
//! drop -- and a panic in Drop during unwinding aborts the process.

use std::io::Cursor;

use copc_rs::CopcWriter;
use las::point::Format;
use las::{Builder, Transform, Vector, Vlr};

const WKT: &[u8] = b"GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563]],PRIMEM[\"Greenwich\",0],UNIT[\"degree\",0.0174532925199433]]";

fn header() -> las::Header {
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
    b2.into_header().unwrap()
}

#[test]
fn dropping_an_unwritten_writer_does_not_panic() {
    let w = CopcWriter::new(Cursor::new(Vec::<u8>::new()), header(), -1, -1).unwrap();
    assert!(!w.is_closed());
    drop(w); // pre-fix: panics with EmptyCopcFile; post-fix: clean.
}
