use copc_rs::reader::CopcReader;
use std::env;
use std::fs::File;
use std::io::BufReader;

fn main() -> laz::Result<()> {
    let lazfn = env::args()
        .nth(1)
        .unwrap_or("tests/data/autzen.laz".to_string());
    let mut laz_file = BufReader::new(File::open(lazfn)?);

    let copc_reader = CopcReader::create(&mut laz_file)?;
    copc_reader.read_points(&mut laz_file)?;

    Ok(())
}
