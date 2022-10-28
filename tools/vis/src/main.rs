#![warn(clippy::pedantic, clippy::nursery)]
#![allow(clippy::missing_errors_doc, clippy::needless_pass_by_value)]

use std::{
    f32::consts::PI,
    io::{BufRead, BufReader},
    thread,
    time::Duration,
};

use bevy::{input::mouse::MouseMotion, prelude::*};
use crossbeam_channel::{bounded, Receiver, Sender};
use csv::StringRecord;
use serde::Deserialize;
use serialport::{ClearBuffer, SerialPortInfo, SerialPortType};

fn main() {
    App::new()
        .add_event::<ImuDataEvent>()
        .add_plugins(DefaultPlugins)
        .add_startup_system(startup)
        .add_system(read_stream)
        .add_system(orientation_system)
        .add_system(acceleration_system)
        .add_system(camera_controller)
        .add_system(hud_system)
        .run();
}

#[derive(Deref)]
struct StreamReceiver(Receiver<ImuData>);
struct ImuDataEvent(ImuData);

#[derive(Debug, Deserialize)]
struct ImuData {
    acc_x: f32,
    acc_y: f32,
    acc_z: f32,
    _mag_x: f32,
    _mag_y: f32,
    _mag_z: f32,
    roll: f32,
    pitch: f32,
    yaw: f32,
}

fn startup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let (tx, rx) = bounded::<ImuData>(10);

    thread::spawn(|| serial_read_loop(tx));

    commands.insert_resource(StreamReceiver(rx));

    commands.spawn_bundle(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Plane { size: 5.0 })),
        material: materials.add(Color::rgb(0.3, 0.5, 0.3).into()),
        ..default()
    });

    commands
        .spawn_bundle(PbrBundle {
            transform: Transform::from_xyz(0.0, 1.0, 0.0),
            ..default()
        })
        .insert(Orientation)
        .with_children(|parent| {
            // circit board
            parent.spawn_bundle(PbrBundle {
                mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
                material: materials.add(Color::rgb(0.3, 0.3, 0.8).into()),
                transform: Transform::from_scale(Vec3::from((1.0, 0.1, 0.5))),
                ..default()
            });

            // connector
            parent.spawn_bundle(PbrBundle {
                mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
                material: materials.add(Color::rgb(0.8, 0.3, 0.3).into()),
                transform: Transform::from_xyz(0.5, 0.0, 0.0)
                    .with_scale(Vec3::from((0.1, 0.2, 0.6))),
                ..default()
            });

            // chips
            parent.spawn_bundle(PbrBundle {
                mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
                material: materials.add(Color::rgb(0.8, 0.3, 0.3).into()),
                transform: Transform::from_xyz(0.0, 0.1, 0.0)
                    .with_scale(Vec3::from((0.5, 0.1, 0.3))),
                ..default()
            });

            //acceleration marker
            parent
                .spawn_bundle(PbrBundle {
                    mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
                    material: materials.add(Color::rgb(0.8, 0.8, 0.3).into()),
                    transform: Transform::from_xyz(0.0, 1.0, 0.0)
                        .with_scale(Vec3::from((0.1, 0.1, 0.1))),
                    ..default()
                })
                .insert(Acceleration);
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
            transform: Transform::from_xyz(2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        })
        .insert(CameraController::default());

    // scoreboard
    commands.spawn_bundle(
        TextBundle::from_section(
            "Score:",
            TextStyle {
                font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                font_size: 16.0,
                color: Color::rgb(0.9, 0.9, 0.9),
            },
        )
        .with_style(Style {
            position_type: PositionType::Absolute,
            position: UiRect {
                top: Val::Px(5.0),
                left: Val::Px(5.0),
                ..default()
            },
            ..default()
        }),
    );
}

fn serial_read_loop(tx: Sender<ImuData>) -> ! {
    let port_info = find_usb_serial_port(0x04b9, 0x0010).expect("Failed to find port");

    let port = serialport::new(&port_info.port_name, 115_200)
        .timeout(Duration::from_millis(100))
        .open();

    match port {
        Ok(port) => {
            println!("Receiving data from {}", &port_info.port_name);

            //clear any data in the buffers
            port.clear(ClearBuffer::All)
                .expect("Failed to clear port buffers");

            //read and discard the first new line of data - could be incomplete
            let mut discard = String::new();
            let mut serial_reader = BufReader::new(port);
            serial_reader
                .read_line(&mut discard)
                .expect("Failed to read first line of serial data");

            let mut csv_reader = csv::Reader::from_reader(serial_reader);

            let mut r = StringRecord::new();

            loop {
                if csv_reader
                    .read_record(&mut r)
                    .expect("Failed to read CSV record")
                {
                    let rec: ImuData = r.deserialize(None).expect("Failed to deserialise record");
                    // println!("{:?}", &rec);
                    tx.send(rec).expect("Failed to send data to channel");
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open \"{}\". Error: {}", &port_info.port_name, e);
            ::std::process::exit(1);
        }
    }
}

#[allow(clippy::similar_names)]
fn find_usb_serial_port(vid: u16, pid: u16) -> Option<SerialPortInfo> {
    serialport::available_ports()
        .map(|pv| {
            pv.into_iter().find(|p| match &p.port_type {
                SerialPortType::UsbPort(u) => u.vid == vid && u.pid == pid,
                SerialPortType::PciPort
                | SerialPortType::BluetoothPort
                | SerialPortType::Unknown => false,
            })
        })
        .expect("Failed to list available serial ports")
}

fn read_stream(receiver: ResMut<StreamReceiver>, mut events: EventWriter<ImuDataEvent>) {
    for event in receiver.try_iter() {
        events.send(ImuDataEvent(event));
    }
}

#[derive(Component)]
struct Orientation;

fn orientation_system(
    mut reader: EventReader<ImuDataEvent>,
    mut query: Query<&mut Transform, With<Orientation>>,
) {
    if let Some(ImuDataEvent(e)) = reader.iter().last() {
        for mut transform in &mut query {
            transform.rotation = Quat::from_euler(EulerRot::YXZ, e.yaw, e.pitch, e.roll);
        }
    }
}

#[derive(Component)]
struct Acceleration;

fn acceleration_system(
    mut reader: EventReader<ImuDataEvent>,
    mut query: Query<&mut Transform, With<Acceleration>>,
) {
    if let Some(ImuDataEvent(e)) = reader.iter().last() {
        for mut transform in &mut query {
            transform.translation = Vec3::from((e.acc_y, e.acc_z, e.acc_x));
        }
    }
}

fn hud_system(mut reader: EventReader<ImuDataEvent>, mut query: Query<&mut Text>) {
    if let Some(ImuDataEvent(e)) = reader.iter().last() {
        for mut text in &mut query {
            if let Some(t) = text.sections.first_mut() {
                t.value = format!(
                    "yaw:{:.02} pitch:{:.02} roll:{:.02}\nAcc:{:.02}, {:.02}, {:.02}",
                    e.yaw / PI * 180.0,
                    e.pitch / PI * 180.0,
                    e.roll / PI * 180.0,
                    e.acc_x,
                    e.acc_y,
                    e.acc_z
                );
            }
        }
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
