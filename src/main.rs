use las::{Read, Reader};

fn main() {
    let mut reader = Reader::from_path("tests/data/autzen.laz").unwrap();
    for wrapped_point in reader.points() {
        let point = wrapped_point.unwrap();
        println!("Point coordinates: ({}, {}, {})", point.x, point.y, point.z);
        if let Some(color) = point.color {
            println!(
                "Point color: red={}, green={}, blue={}",
                color.red, color.green, color.blue,
            );
        }
    }
}
