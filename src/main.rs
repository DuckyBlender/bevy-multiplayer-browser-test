#![allow(clippy::needless_pass_by_value)] // clippy pedantic false positive
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::prelude::*;
use bevy_ggrs::{ggrs, GGRSPlugin, GGRSSchedule, PlayerInputs, RollbackIdProvider};
use bevy_matchbox::prelude::*;

use bevy::render::camera::ScalingMode;

const ROOM_ID: &str = "duckytest";
const ROOM_SIZE: usize = 2;
const INPUT_DELAY: usize = 2; // in frames

const ROOM_IP: &str = "matchbox.ducky.pics";
// const ROOM_IP: &str = "localhost";
const WS_OR_WSS: &str = "wss";
const ROOM_PORT: u16 = 7777;

const INPUT_UP: u8 = 1 << 0;
const INPUT_DOWN: u8 = 1 << 1;
const INPUT_LEFT: u8 = 1 << 2;
const INPUT_RIGHT: u8 = 1 << 3;
const INPUT_FIRE: u8 = 1 << 4;

#[derive(Component)]
struct Player {
    handle: usize,
}

struct GgrsConfig;

impl ggrs::Config for GgrsConfig {
    // 4-directions + fire fits easily in a single byte
    type Input = u8;
    type State = u8;
    // Matchbox' WebRtcSocket addresses are called `PeerId`s
    type Address = PeerId;
}

fn main() {
    let mut app = App::new();

    GGRSPlugin::<GgrsConfig>::new()
        .with_input_system(input)
        .register_rollback_component::<Transform>() // rollback the player's position to be in sync with the game state
        .build(&mut app);

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            // fill the entire browser window
            fit_canvas_to_parent: true,
            // don't hijack keyboard shortcuts like F5, F6, F12, Ctrl+R etc.
            prevent_default_event_handling: false,
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::rgb(0.15, 0.15, 0.15)))
    .add_startup_systems((setup, spawn_players, start_matchbox_socket))
    .add_systems((move_players.in_schedule(GGRSSchedule), wait_for_players)) // NEW
    .run();
}

fn setup(mut commands: Commands) {
    let mut camera_bundle = Camera2dBundle::default();
    camera_bundle.projection.scaling_mode = ScalingMode::FixedVertical(10.);
    commands.spawn(camera_bundle);
}

fn spawn_players(mut commands: Commands, mut rollback: ResMut<RollbackIdProvider>) {
    // Player 1
    commands.spawn((
        Player { handle: 0 },
        rollback.next(), // rollback component has a unique ID
        SpriteBundle {
            transform: Transform::from_translation(Vec3::new(-2., 0., 0.)),
            sprite: Sprite {
                color: Color::rgb(0., 0.47, 1.),
                custom_size: Some(Vec2::new(1., 1.)),
                ..default()
            },
            ..default()
        },
    ));

    // Player 2
    commands.spawn((
        Player { handle: 1 },
        rollback.next(),
        SpriteBundle {
            transform: Transform::from_translation(Vec3::new(2., 0., 0.)),
            sprite: Sprite {
                color: Color::rgb(1., 0.47, 0.),
                custom_size: Some(Vec2::new(1., 1.)),
                ..default()
            },
            ..default()
        },
    ));
}

fn move_players(
    inputs: Res<PlayerInputs<GgrsConfig>>,
    mut player_query: Query<(&mut Transform, &Player)>,
) {
    for (mut transform, player) in player_query.iter_mut() {
        let (input, _) = inputs[player.handle];

        let mut direction = Vec2::ZERO;

        if input & INPUT_UP != 0 {
            direction.y += 1.;
        }
        if input & INPUT_DOWN != 0 {
            direction.y -= 1.;
        }
        if input & INPUT_RIGHT != 0 {
            direction.x += 1.;
        }
        if input & INPUT_LEFT != 0 {
            direction.x -= 1.;
        }
        if direction == Vec2::ZERO {
            continue;
        }

        let move_speed = 0.13;
        let move_delta = (direction * move_speed).extend(0.);

        transform.translation += move_delta;
    }
}

fn start_matchbox_socket(mut commands: Commands) {
    let room_url = format!("{WS_OR_WSS}://{ROOM_IP}:{ROOM_PORT}/{ROOM_ID}?next={ROOM_SIZE}");
    info!("Connecting to matchbox server: {:?}", room_url);
    commands.insert_resource(MatchboxSocket::new_ggrs(room_url));
}

fn wait_for_players(mut commands: Commands, mut socket: ResMut<MatchboxSocket<SingleChannel>>) {
    if socket.get_channel(0).is_err() {
        return; // we've already started
    }

    // Check for new connections
    socket.update_peers();
    let players = socket.players();

    if players.len() < ROOM_SIZE {
        return; // wait for more players
    }

    info!("All peers have joined, going in-game");

    // create a GGRS P2P session
    let mut session_builder = ggrs::SessionBuilder::<GgrsConfig>::new()
        .with_num_players(ROOM_SIZE)
        .with_input_delay(INPUT_DELAY);

    for (i, player) in players.into_iter().enumerate() {
        session_builder = session_builder
            .add_player(player, i)
            .expect("Failed to add player");
    }

    // move the channel out of the socket (required because GGRS takes ownership of it)
    let channel = socket.take_channel(0).unwrap();

    // start the GGRS session
    let ggrs_session = session_builder
        .start_p2p_session(channel)
        .expect("Failed to start session");

    commands.insert_resource(bevy_ggrs::Session::P2PSession(ggrs_session));
}

fn input(_: In<ggrs::PlayerHandle>, keys: Res<Input<KeyCode>>) -> u8 {
    let mut input = 0u8;

    if keys.any_pressed([KeyCode::Up, KeyCode::W]) {
        input |= INPUT_UP;
    }
    if keys.any_pressed([KeyCode::Down, KeyCode::S]) {
        input |= INPUT_DOWN;
    }
    if keys.any_pressed([KeyCode::Left, KeyCode::A]) {
        input |= INPUT_LEFT;
    }
    if keys.any_pressed([KeyCode::Right, KeyCode::D]) {
        input |= INPUT_RIGHT;
    }
    if keys.any_pressed([KeyCode::Space, KeyCode::Return]) {
        input |= INPUT_FIRE;
    }

    input
}
