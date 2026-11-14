//! Generic characters: anything that moves, acts and takes damage like the player —
//! future enemies included. Nothing in here knows about the keyboard or the player.
//!
//! **Interface:** a character is an entity with
//! - [`MoveIntent`]: where it wants to go (written by player input or, later, AI),
//! - [`ActionIntent`]: which action it wants to do next (input buffer),
//! - [`controller::CharacterController`], [`action::CharacterAction`],
//!   [`stats::Health`], [`stats::Stamina`], a `Collider` and `RigidBody::Kinematic`.
//!
//! [`character_bundle`] bundles all of that. Pair it with a humanoid model
//! (`crate::humanoid`) to get the shared animations.

pub mod action;
pub mod controller;
pub mod stats;

use avian3d::prelude::*;
use bevy::prelude::*;

use action::{ActionRequest, CharacterAction};
use controller::CharacterController;
use stats::{DamageMessage, DamageTaken, Health, Stamina};

use crate::tuning::*;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageMessage>()
            .add_message::<DamageTaken>()
            .configure_sets(
                FixedUpdate,
                (
                    CharacterSystems::Damage,
                    CharacterSystems::Actions,
                    CharacterSystems::Combat,
                    CharacterSystems::Stats,
                    CharacterSystems::Movement,
                )
                    .chain(),
            )
            .add_systems(
                FixedUpdate,
                (
                    stats::apply_damage.in_set(CharacterSystems::Damage),
                    (action::start_buffered_action, action::release_heavy_charge, action::tick_action)
                        .chain()
                        .in_set(CharacterSystems::Actions),
                    stats::update_stamina.in_set(CharacterSystems::Stats),
                    controller::move_character.in_set(CharacterSystems::Movement),
                ),
            );
    }
}

/// Fixed-timestep order: damage → actions → combat (hit detection) → stats → movement.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CharacterSystems {
    Damage,
    Actions,
    Combat,
    Stats,
    Movement,
}

/// What the character wants to do with its legs this tick.
#[derive(Component, Default, Debug, Clone)]
pub struct MoveIntent {
    /// World-space horizontal direction, length 0..=1.
    pub dir: Vec3,
    pub sprint: bool,
    /// Seconds left in which a jump request is still honoured.
    pub jump_buffer: f32,
}

/// Which action the character wants next (the input buffer) and whether the attack is held.
#[derive(Component, Default, Debug, Clone)]
pub struct ActionIntent {
    pub buffered: Option<ActionRequest>,
    /// Seconds before `buffered` expires.
    pub buffer_left: f32,
    /// Attack still held: keeps a heavy attack charging.
    pub attack_held: bool,
}

impl ActionIntent {
    /// Queue an action request; it stays valid for `INPUT_BUFFER` seconds.
    pub fn buffer(&mut self, request: ActionRequest) {
        self.buffered = Some(request);
        self.buffer_left = INPUT_BUFFER;
    }

    /// Ages the buffer by `dt`, dropping the request when it expires.
    pub fn tick(&mut self, dt: f32) {
        self.buffer_left = (self.buffer_left - dt).max(0.0);
        if self.buffer_left == 0.0 {
            self.buffered = None;
        }
    }
}

/// Everything a character needs to move, act and take damage. `transform` is the capsule center.
pub fn character_bundle(transform: Transform, max_hp: f32, max_stamina: f32) -> impl Bundle {
    (
        MoveIntent::default(),
        ActionIntent::default(),
        CharacterController::default(),
        CharacterAction::default(),
        Health::new(max_hp),
        Stamina::new(max_stamina),
        transform,
        Visibility::default(),
        RigidBody::Kinematic,
        Collider::capsule(PLAYER_RADIUS, PLAYER_CAPSULE_LENGTH),
        TransformInterpolation,
    )
}
