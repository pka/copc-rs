# copc-rs

[![crates.io version](https://img.shields.io/crates/v/copc-rs.svg)](https://crates.io/crates/copc-rs)
[![docs.rs docs](https://docs.rs/copc-rs/badge.svg)](https://docs.rs/copc-rs)


copc-rs is a library for reading Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.


## Usage example

```rust
let mut copc_reader = CopcReader::from_path("autzen-classified.copc.laz")?;
for point in copc_reader.points(LodSelection::Level(0), BoundsSelection::All)?.take(5) {
    println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
}
```

## Credits
This fork simplifies the work of Pirmin Kalberer, owner of the forked repo

This library depends heavily on the work of Thomas Montaigu (@tmontaigu) and Pete Gadomski (@gadomski), the authors of the laz and las crates.
