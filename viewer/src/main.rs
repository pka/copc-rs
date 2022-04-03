use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;

use las::{Read, Reader};

fn main() {
    App::new()
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_startup_system(setup_scene)
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // mesh
    let mesh = build_mesh("../tests/data/autzen.laz");
    let bbox = mesh.compute_aabb().unwrap();
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(mesh),
        material: materials.add(Color::rgb(0.8, 0.7, 0.6).into()),
        ..Default::default()
    });

    // light
    let light = bbox.center + bbox.half_extents * Vec3::new(4.0, 8.0, 4.0);
    commands.spawn_bundle(PointLightBundle {
        transform: Transform::from_translation(light),
        ..Default::default()
    });
    // camera
    // let cam = bbox.center + bbox.half_extents * Vec3::new(-3.0, 3.0, 5.0);
    let cam = bbox.center + bbox.half_extents * 1.1;
    dbg!(&bbox, &light, &cam);
    commands.spawn_bundle(PerspectiveCameraBundle {
        transform: Transform::from_translation(cam).looking_at(bbox.center, Vec3::Y),
        ..Default::default()
    });
}

fn build_mesh(laz_file: &str) -> Mesh {
    let mut reader = Reader::from_path(laz_file).unwrap();
    let positions: Vec<[f32; 3]> = reader
        .points()
        .map(|wrapped_point| {
            let point = wrapped_point.unwrap();
            if let Some(color) = point.color {
                println!(
                    "Point color: red={}, green={}, blue={}",
                    color.red, color.green, color.blue,
                );
            }
            [-point.x as f32, point.z as f32, point.y as f32]
        })
        .collect();
    let points_length = positions.len();

    let mut mesh = Mesh::new(PrimitiveTopology::PointList);
    mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.set_attribute(Mesh::ATTRIBUTE_NORMAL, vec![0.0; points_length]);
    mesh.set_attribute(Mesh::ATTRIBUTE_UV_0, vec![0.0; points_length]);
    mesh.set_indices(Some(Indices::U32(Vec::from_iter(0..points_length as u32))));

    /*
    Advanced techniques:
    - https://twitter.com/m_schuetz/status/1509963316143267844
    - https://discord.com/channels/691052431525675048/960107893158473758/960218799771095110
    */

    mesh
}
