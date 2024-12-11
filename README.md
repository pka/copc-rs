# copc-rs

[![crates.io version](https://img.shields.io/crates/v/copc-rs.svg)](https://crates.io/crates/copc-rs)
[![docs.rs docs](https://docs.rs/copc-rs/badge.svg)](https://docs.rs/copc-rs)


copc-rs is a library for reading Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.


## Usage examples

### reader
```rust
use copc_rs::{Bounds, BoundsSelection, CopcReader, LodSelection, Vector};

fn main() {
    let path = "./lidar.copc.laz";
    let mut copc_reader = CopcReader::from_path(path).unwrap();

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
    let las_reader = Reader::from_path("./lidar.las");

    let header = las_reader.header().clone();

    let mut copc_writer = CopcWriter::from_path("./lidar.copc.laz", header, -1);

    copc_writer.write(las_reader.points().filter_map(las::Result::ok)).unwrap();

    // This is not necessary as it is done automatically
    // when the writer is dropped, but functions such as
    // copc_writer.copc_info() and copc_writer.hierarchy_entries()
    // only make sense after closing the writer
    copc_writer.close();

    println!("{:#?}", copc_writer.copc_info());
}
```

## Credits
This library depends heavily on the work of Thomas Montaigu (@tmontaigu) and Pete Gadomski (@gadomski), the authors of the laz and las crates.
