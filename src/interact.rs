//! The E key's shared prompt. Each frame the prompt is cleared, then interactables claim it in
//! [`InteractSystems`] order (pickups, then doors, then the Hearth); the first one in reach wins and
//! consumes `PlayerInput::interact`.

use bevy::prelude::*;

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InteractPrompt>()
            .configure_sets(Update, (InteractSystems::Clear, InteractSystems::Pickups, InteractSystems::Doors, InteractSystems::Hearth).chain())
            .add_systems(Update, clear_prompt.in_set(InteractSystems::Clear));
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum InteractSystems {
    Clear,
    Pickups,
    Doors,
    Hearth,
}

/// Interaction hint for the HUD, `None` when nothing is in reach.
#[derive(Resource, Default, Debug)]
pub struct InteractPrompt(pub Option<&'static str>);

fn clear_prompt(mut prompt: ResMut<InteractPrompt>) {
    prompt.0 = None;
}
