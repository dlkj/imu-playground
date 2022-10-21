#![warn(clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc, clippy::needless_pass_by_value)]

use std::f32::consts::PI;

use bevy::{input::mouse::MouseMotion, prelude::*};

#[derive(Component)]
struct Rotator;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_startup_system(startup)
        .add_system(rotator_system)
        .add_system(camera_controller)
        .run();
}

fn startup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Plane { size: 5.0 })),
        material: materials.add(Color::rgb(0.3, 0.5, 0.3).into()),
        ..default()
    });

    let cube_handle = meshes.add(Mesh::from(shape::Cube { size: 1.0 }));
    let cube_material_handle = materials.add(Color::rgb(0.8, 0.7, 0.6).into());

    commands
        .spawn_bundle(PbrBundle {
            mesh: cube_handle.clone(),
            material: cube_material_handle.clone(),
            transform: Transform::from_xyz(0.0, 0.5, 0.0),
            ..default()
        })
        .insert(Rotator)
        .with_children(|parent| {
            parent.spawn_bundle(PbrBundle {
                mesh: cube_handle,
                material: cube_material_handle,
                transform: Transform::from_xyz(1.0, 1.0, 1.0),
                ..default()
            });
        });

    let arrow_material_handle = materials.add(Color::rgb(0.8, 0.3, 0.3).into());

    commands
        .spawn_bundle(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Capsule {
                radius: 0.05,
                ..default()
            })),
            material: arrow_material_handle.clone(),
            transform: Transform::from_xyz(-2.0, 0.2, 0.0).with_rotation(Quat::from_euler(
                EulerRot::ZYX,
                0.0,
                0.0,
                PI / 2.0,
            )),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn_bundle(PbrBundle {
                mesh: meshes.add(Mesh::from(shape::UVSphere {
                    radius: 0.1,
                    ..default()
                })),
                material: arrow_material_handle,
                transform: Transform::from_xyz(0.0, 0.5, 0.0),
                ..default()
            });
        });

    // light
    commands.spawn_bundle(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });

    // camera
    commands
        .spawn_bundle(Camera3dBundle {
            transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        })
        .insert(CameraController::default());
}

fn rotator_system(time: Res<Time>, mut query: Query<&mut Transform, With<Rotator>>) {
    for mut transform in &mut query {
        transform.rotate_y(1.0 * time.delta_seconds());
    }
}

#[derive(Component)]
pub struct CameraController {
    pub initialized: bool,
    pub sensitivity: f32,
    pub mouse_key_enable_mouse: MouseButton,
    pub pitch: f32,
    pub yaw: f32,
    pub distance: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            initialized: false,
            sensitivity: 0.5,
            mouse_key_enable_mouse: MouseButton::Left,
            pitch: 0.0,
            yaw: 0.0,
            distance: 0.0,
        }
    }
}

pub fn camera_controller(
    time: Res<Time>,
    mut mouse_events: EventReader<MouseMotion>,
    mouse_button_input: Res<Input<MouseButton>>,
    mut query: Query<(&mut Transform, &mut CameraController), With<Camera>>,
) {
    let dt = time.delta_seconds();

    if let Ok((mut transform, mut options)) = query.get_single_mut() {
        if !options.initialized {
            let (yaw, pitch, _roll) = transform.rotation.to_euler(EulerRot::YXZ);
            options.yaw = yaw;
            options.pitch = pitch;
            options.distance = transform.translation.length();
            options.initialized = true;
        }

        // Handle mouse input
        let mut mouse_delta = Vec2::ZERO;
        if mouse_button_input.pressed(options.mouse_key_enable_mouse) {
            for mouse_event in mouse_events.iter() {
                mouse_delta += mouse_event.delta;
            }
        }

        if mouse_delta != Vec2::ZERO {
            // Apply look update
            let (pitch, yaw) = (
                (options.pitch - mouse_delta.y * 0.5 * options.sensitivity * dt).clamp(
                    -0.99 * std::f32::consts::FRAC_PI_2,
                    0.99 * std::f32::consts::FRAC_PI_2,
                ),
                options.yaw - mouse_delta.x * options.sensitivity * dt,
            );

            transform.translation = Vec3::ZERO;
            transform.rotation = Quat::from_euler(EulerRot::ZYX, 0.0, yaw, pitch);
            transform.translation = transform.back() * options.distance;

            options.pitch = pitch;
            options.yaw = yaw;
        }
    }
}
