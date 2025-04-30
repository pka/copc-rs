use copc_rs::{BoundsSelection, CopcReader, LodSelection};
use http_range_client::UreqHttpReader as HttpReader;

fn main() -> copc_rs::Result<()> {
    env_logger::init();
    let mut http_reader =
        HttpReader::new("https://s3.amazonaws.com/hobu-lidar/autzen-classified.copc.laz");
    // http_reader.set_min_req_size(1_048_576); // 1MB - 3 requests, 3'145'728 B
    // http_reader.set_min_req_size(524288); // 512KB - 4 requests, 2'097'152 B
    http_reader.set_min_req_size(262144); // 256KB - 5 requests, 1'310'720 B

    let mut copc_reader = CopcReader::new(http_reader)?;

    let mut max_z: f64 = 0.0;
    for point in copc_reader.points(LodSelection::Level(0), BoundsSelection::All)? {
        max_z = max_z.max(point.z);
    }
    println!("Max Z Level 0: {max_z}");

    Ok(())
}
