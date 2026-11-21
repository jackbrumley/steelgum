//! A small game using Steelgum's scene format and coordinates.

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, io};

use serde::Serialize;
use steelgum::{Scene, SceneError, SceneObject};
use thiserror::Error;

pub const STEP: Duration = Duration::from_millis(20);
pub const DOOR_ID: &str = "door";
pub const PLAYER_ID: &str = "player";

const PLATE_ID: &str = "plate";
const GOAL_ID: &str = "goal";
const DOOR_HOLD: Duration = Duration::from_secs(3);
const WALK_SPEED: f32 = 2.8;
const DEMO_STEPS: usize = 200;

#[derive(Debug, Error)]
pub enum GameError {
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error("could not read scene {path}: {source}")]
    ReadScene { path: PathBuf, source: io::Error },
    #[error("invalid scene {path}: {source}")]
    InvalidScene { path: PathBuf, source: SceneError },
    #[error("the door game scene is missing object {0:?}")]
    MissingObject(&'static str),
    #[error("the player must have a square footprint")]
    InvalidPlayer,
    #[error("the scripted playthrough did not reach the goal")]
    GoalNotReached,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub player: [f32; 3],
    pub door_open: bool,
    pub plate_pressed: bool,
    pub door_hold_ms: u32,
    pub won: bool,
}

pub struct DoorGame {
    scene: Scene,
    start: [f32; 3],
    player_half_width: f32,
    door_index: usize,
    plate_index: usize,
    goal_index: usize,
    status: Status,
    door_hold: Duration,
}

impl DoorGame {
    pub fn new(scene: Scene) -> Result<Self, GameError> {
        let find = |id| {
            scene
                .objects()
                .iter()
                .position(|object| object.id == id)
                .ok_or(GameError::MissingObject(id))
        };
        let door_index = find(DOOR_ID)?;
        let plate_index = find(PLATE_ID)?;
        let goal_index = find(GOAL_ID)?;
        let player = &scene.objects()[find(PLAYER_ID)?];
        if player.size[0] != player.size[2] {
            return Err(GameError::InvalidPlayer);
        }
        let start = player.center;
        let player_half_width = player.size[0] / 2.0;
        let mut game = Self {
            scene,
            start,
            player_half_width,
            door_index,
            plate_index,
            goal_index,
            status: Status::default(),
            door_hold: Duration::ZERO,
        };
        game.restart();
        Ok(game)
    }

    pub fn demo_scene() -> Result<Self, GameError> {
        Self::new(Scene::from_json(include_str!("../scenes/room.json"))?)
    }

    pub fn load_scene(path: impl AsRef<Path>) -> Result<Self, GameError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| GameError::ReadScene {
            path: path.to_owned(),
            source,
        })?;
        let scene = Scene::from_json(&text).map_err(|source| GameError::InvalidScene {
            path: path.to_owned(),
            source,
        })?;
        Self::new(scene)
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn restart(&mut self) {
        self.status = Status {
            player: self.start,
            ..Status::default()
        };
        self.door_hold = Duration::ZERO;
        self.update_triggers();
    }

    /// Advances one simulation step, with input in world X and Z directions.
    pub fn step(&mut self, input: [f32; 2]) {
        let [x, z] = input.map(|axis| {
            if axis.is_finite() {
                axis.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        });
        let scale = WALK_SPEED * STEP.as_secs_f32() / x.hypot(z).max(1.0);
        let [old_x, y, old_z] = self.status.player;
        let next_x = old_x + x * scale;
        let next_z = old_z + z * scale;
        let x = if self.blocked([next_x, old_z]) {
            old_x
        } else {
            next_x
        };
        let z = if self.blocked([x, next_z]) {
            old_z
        } else {
            next_z
        };
        self.status.player = [x, y, z];
        self.door_hold = self.door_hold.saturating_sub(STEP);
        self.update_triggers();
    }

    pub fn run_demo(&mut self) -> Result<(), GameError> {
        self.restart();
        for _ in 0..DEMO_STEPS {
            self.step([0.0, -1.0]);
            if self.status.won {
                return Ok(());
            }
        }
        Err(GameError::GoalNotReached)
    }

    fn update_triggers(&mut self) {
        let position = [self.status.player[0], self.status.player[2]];
        let objects = self.scene.objects();
        let plate_pressed = objects[self.plate_index].overlaps_footprint(position, 0.0);
        if plate_pressed || self.status.plate_pressed {
            self.door_hold = DOOR_HOLD;
        }
        self.status.plate_pressed = plate_pressed;
        // Keep the door clear while a player is physically crossing its threshold.
        let in_open_door = self.status.door_open
            && objects[self.door_index].overlaps_footprint(position, self.player_half_width);
        self.status.door_open = !self.door_hold.is_zero() || in_open_door;
        self.status.door_hold_ms = self.door_hold.as_millis() as u32;
        self.status.won |= objects[self.goal_index].overlaps_footprint(position, 0.0);
    }

    fn blocked(&self, position: [f32; 2]) -> bool {
        self.scene
            .objects()
            .iter()
            .any(|object| self.blocks(object, position))
    }

    fn blocks(&self, object: &SceneObject, position: [f32; 2]) -> bool {
        if object.id == PLAYER_ID {
            return false;
        }
        let solid = if object.id == DOOR_ID {
            !self.status.door_open
        } else {
            object.solid
        };
        solid && object.overlaps_footprint(position, self.player_half_width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_off_plate_starts_a_three_second_hold() {
        let mut game = DoorGame::demo_scene().expect("valid demo scene");
        assert!(!game.status().door_open);
        for _ in 0..45 {
            game.step([0.0, -1.0]);
        }
        assert!(game.status().plate_pressed);
        assert!(game.status().door_open);

        let left_plate = (0..25).any(|_| {
            game.step([1.0, 0.0]);
            !game.status().plate_pressed
        });
        assert!(left_plate);
        assert_eq!(game.status().door_hold_ms, 3_000);
        for _ in 1..150 {
            game.step([0.0, 0.0]);
        }
        assert!(game.status().door_open);
        game.step([0.0, 0.0]);
        assert!(!game.status().door_open);

        game.step([0.0, 1.0]);
        game.restart();
        assert!(!game.status().door_open);
        assert!(!game.status().won);
    }

    #[test]
    fn a_scripted_player_can_reach_the_goal() {
        let mut game = DoorGame::demo_scene().expect("valid demo scene");
        game.run_demo().expect("player can reach the goal");
        assert!(game.status().won);
        assert!(game.status().door_open);
        for _ in 0..25 {
            game.step([1.0, 0.0]);
        }
        assert!(game.status().won);
        game.restart();
        assert!(!game.status().won);
    }
}
