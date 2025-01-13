# copc-rs

[![crates.io version](https://img.shields.io/crates/v/copc-rs.svg)](https://crates.io/crates/copc-rs)
[![docs.rs docs](https://docs.rs/copc-rs/badge.svg)](https://docs.rs/copc-rs)

copc-rs is a rust library for reading and writing Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.

## Usage examples

### reader
```rust
use copc_rs::{Bounds, BoundsSelection, CopcReader, LodSelection, Vector};

fn main() {
    let mut copc_reader = CopcReader::from_path("./lidar.copc.laz").unwrap();

    let bounds = Bounds {
        min: Vector {
            x: 698_100.,
            y: 6_508_100.,
            z: 0.,
        },
        max: Vector {
            x: 698_230.,
            y: 6_508_189.,
            z: 2_000.,
        },
    };

    for point in copc_reader
        .points(LodSelection::Resolution(1.), BoundsSelection::Within(bounds))
        .unwrap()
    {
        // do something with the points
    }
}
```
### writer
```rust
use copc_rs::CopcWriter;
use las::Reader;

fn main() {
    let mut las_reader = Reader::from_path("./lidar.las").unwrap();

    let header = las_reader.header().clone();
    let num_points = header.number_of_points();

    let points = las_reader.points().filter_map(las::Result::ok);

    let mut copc_writer = CopcWriter::from_path("./lidar.copc.laz", header, -1, -1).unwrap();

    copc_writer.write(points, num_points).unwrap();

    println!("{:#?}", copc_writer.copc_info());
}
```

## Credits
This library depends heavily on the work of Thomas Montaigu (@tmontaigu) and Pete Gadomski (@gadomski), the authors of the laz and las crates.
