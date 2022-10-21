# copc-rs

[![crates.io version](https://img.shields.io/crates/v/copc-rs.svg)](https://crates.io/crates/copc-rs)
[![docs.rs docs](https://docs.rs/copc-rs/badge.svg)](https://docs.rs/copc-rs)


copc-rs is a library for reading Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.


## Usage example

```rust
let laz_file = BufReader::new(File::open("autzen-classified.copc.laz")?);
let mut copc_reader = CopcReader::open(laz_file)?;
for point in copc_reader.points(LodSelection::Level(0), BoundsSelection::All)?.take(5) {
    println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
}
```
