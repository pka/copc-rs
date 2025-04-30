use copc_rs::{BoundsSelection, CopcReader, LodSelection};
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

fn main() -> copc_rs::Result<()> {
    let lazfn = env::args().nth(1).expect("COPC file required");

    let mut copc_reader = CopcReader::from_path(&lazfn)?;

    let dest = Path::new(&lazfn).with_extension("xyz");
    println!("Writing {:?}", &dest);
    let mut file = BufWriter::new(File::create(dest)?);

    for point in copc_reader.points(LodSelection::Level(0), BoundsSelection::All)? {
        writeln!(&mut file, "{} {} {}", point.x, point.y, point.z)?;
    }

    Ok(())
}
