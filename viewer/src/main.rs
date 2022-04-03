use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;

fn main() {
    App::new()
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_startup_system(setup_scene)
        .add_system(cycle_msaa)
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    info!("Press 'm' to toggle MSAA");
    info!("Using 4x MSAA");

    // mesh
    let mesh = build_mesh();
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
    let cam = bbox.center + bbox.half_extents * Vec3::new(-3.0, 3.0, 5.0);
    commands.spawn_bundle(PerspectiveCameraBundle {
        transform: Transform::from_translation(cam).looking_at(bbox.center, Vec3::Y),
        ..Default::default()
    });
}

fn build_mesh() -> Mesh {
    let positions = vec![
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [-1.0, 1.0, -1.0],
        [1.0, 1.0, -1.0],
        [1.0, -1.0, -1.0],
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [1.0, 1.0, 1.0],
        [1.0, -1.0, 1.0],
        [-1.0, -1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, -1.0, 1.0],
        [-1.0, -1.0, 1.0],
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
    ];
    let points_length = positions.len();

    let mut mesh = Mesh::new(PrimitiveTopology::PointList);
    mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.set_attribute(Mesh::ATTRIBUTE_NORMAL, vec![0.0; points_length]);
    mesh.set_attribute(Mesh::ATTRIBUTE_UV_0, vec![0.0; points_length]);
    mesh.set_indices(Some(Indices::U32(Vec::from_iter(0..points_length as u32))));

    mesh
}

fn cycle_msaa(input: Res<Input<KeyCode>>, mut msaa: ResMut<Msaa>) {
    if input.just_pressed(KeyCode::M) {
        if msaa.samples == 4 {
            info!("Not using MSAA");
            msaa.samples = 1;
        } else {
            info!("Using 4x MSAA");
            msaa.samples = 4;
        }
    }
}
