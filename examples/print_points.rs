use copc_rs::{BoundsSelection, CopcReader, LodSelection};
use std::env;

fn main() -> copc_rs::Result<()> {
    let lazfn = env::args().nth(1).expect("COPC file required");

    let mut copc_reader = CopcReader::from_path(&lazfn)?;
    for (i, point) in copc_reader
        .points(LodSelection::Level(0), BoundsSelection::All)?
        .enumerate()
        .take(5)
    {
        if i == 0 {
            dbg!(&point);
        }
        println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
    }

    Ok(())
}
