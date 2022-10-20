use copc_rs::{CopcReader, LodSelection};
use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

fn main() -> laz::Result<()> {
    let lazfn = env::args().nth(1).expect("COPC file required");

    let laz_file = BufReader::new(File::open(&lazfn)?);
    let mut copc_reader = CopcReader::open(laz_file)?;

    let dest = Path::new(&lazfn).with_extension("xyz");
    println!("Writing {:?}", &dest);
    let mut file = BufWriter::new(File::create(dest)?);

    for point in copc_reader.points(LodSelection::Level(0), None)? {
        writeln!(&mut file, "{} {} {}", point.x, point.y, point.z)?;
    }

    Ok(())
}
