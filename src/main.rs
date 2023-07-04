#![allow(clippy::needless_pass_by_value)] // clippy pedantic false positive
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::{
    core::{Pod, Zeroable},
    prelude::*,
};
use bevy_ggrs::{ggrs, GGRSPlugin, GGRSSchedule, PlayerInputs, RollbackIdProvider};
use bevy_matchbox::prelude::*;

use bevy::render::camera::ScalingMode;

const ROOM_ID: &str = "duckytest";
const ROOM_SIZE: usize = 2;
const INPUT_DELAY: usize = 2; // in frames

const LOCAL: bool = false;

const ROOM_IP: &str = if LOCAL {
    "localhost"
} else {
    "matchbox.ducky.pics"
};

// const WS_OR_WSS: &str = if LOCAL { "ws" } else { "wss" };
const WS_OR_WSS: &str = "ws";

const ROOM_PORT: u16 = 80;

#[derive(Component)]
struct Player {
    handle: usize,
}

#[derive(Copy, Clone, Pod, Zeroable, PartialEq)]
#[repr(C)]
struct MouseChanges {
    x: f32,
    y: f32,
}

struct GgrsConfig;

impl ggrs::Config for GgrsConfig {
    // x and y offsets (first 32 bits are x, last 32 bits are y)
    type Input = MouseChanges;
    type State = i64;
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
    camera_bundle.projection.scaling_mode = ScalingMode::FixedVertical(10.); // one unit is 10 pixels
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
                custom_size: Some(Vec2::new(0.5, 0.5)),
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
                custom_size: Some(Vec2::new(0.5, 0.5)),
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

        // unpack the input data
        let (x, y) = (input.x, input.y);
        // Check if mouse is moving
        if x == 0. && y == 0. {
            continue;
        }

        // info!("Moving player {} to {:?}", player.handle, (x, y));
        transform.translation = Vec3::new(x, y, 0.);
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

fn input(
    _: In<ggrs::PlayerHandle>,
    mut mouse_movement: EventReader<CursorMoved>,
    window: Query<&Window>,
) -> MouseChanges {
    let mut input = MouseChanges { x: 0., y: 0. };

    for event in mouse_movement.iter() {
        input.x = event.position.x;
        input.y = event.position.y;
    }

    if input.x == 0. && input.y == 0. {
        // Return early if the mouse hasn't moved
        return input;
    }

    // We need to convert the screen coordinates to world coordinates
    let window = window.single();
    let (width, height, aspect_ratio) = (
        window.width(),
        window.height(),
        window.width() / window.height(),
    );
    // The center is (0, 0), so we need to offset the mouse position
    input.x -= width / 2.;
    input.y -= height / 2.;

    // The camera is 10 units away from the origin, so we need to scale the mouse position
    input.x *= aspect_ratio * 10. / width;
    input.y *= 10. / height;

    info!("mouse: {}, {}", input.x, input.y);

    input
}
