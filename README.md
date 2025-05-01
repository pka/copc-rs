# copc-rs

[![crates.io version](https://img.shields.io/crates/v/copc-rs.svg)](https://crates.io/crates/copc-rs)
[![docs.rs docs](https://docs.rs/copc-rs/badge.svg)](https://docs.rs/copc-rs)

copc-rs is a rust library for reading and writing Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.
It utilizes the las and laz crates heavily and tries to offer a similiar API to las.

## Usage examples

### Reader

```rust
let mut copc_reader = CopcReader::from_path("autzen-classified.copc.laz")?;
for point in copc_reader.points(LodSelection::Level(0), BoundsSelection::All)?.take(5) {
    println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
}
```

Full example with bounds selection:
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

Run an example:
```
cargo run --example copc_http
```

### Writer [[*]](#writing-is-still-a-wip)

```rust
use copc_rs::CopcWriter;
use las::Reader;

fn main() {
    let mut las_reader = Reader::from_path("./lidar.las").unwrap();

    let header = las_reader.header().clone();
    let num_points = header.number_of_points() as i32;
    let points = las_reader.points().filter_map(las::Result::ok);

    let mut copc_writer = CopcWriter::from_path("./lidar.copc.laz", header, -1, -1).unwrap();

    copc_writer.write(points, num_points).unwrap();

    println!("{:#?}", copc_writer.copc_info());
}
```

## Writing is still a WIP

Writing of the octree structure seem to work, so spatial queries in full resolution on copc-rs written files should be good.
BUT the octree levels does not yet contain a similar point distribution as the whole cloud so results from resolution queries on copc-rs written files are wrong.
This means the written files will look bad in viewers.


I will look into it when I find time, for now I only need full resolution spatial queries in my current project anyway.

-yvind

## Credits

This library depends heavily on the work of Thomas Montaigu (@tmontaigu) and Pete Gadomski (@gadomski), the authors of the laz and las crates.
