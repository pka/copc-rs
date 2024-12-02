# copc-rs

[![crates.io version](https://img.shields.io/crates/v/copc-rs.svg)](https://crates.io/crates/copc-rs)
[![docs.rs docs](https://docs.rs/copc-rs/badge.svg)](https://docs.rs/copc-rs)


copc-rs is a library for reading Cloud Optimized Point Cloud ([COPC](https://copc.io/)) data.


## Usage example

```rust
use copc_rs::{Bounds, BoundsSelection, CopcReader, LodSelection, Vector};

fn main() {
    let path = "../aydatlidar/1.copc.laz";
    let mut las_reader = CopcReader::from_path(path).unwrap();

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

    for point in las_reader
        .points(LodSelection::All, BoundsSelection::Within(bounds))
        .unwrap()
    {
        // do something with the points
    }
}
```

## Credits
This fork simplifies the work of Pirmin Kalberer, owner of the forked repo

This library depends heavily on the work of Thomas Montaigu (@tmontaigu) and Pete Gadomski (@gadomski), the authors of the laz and las crates.
