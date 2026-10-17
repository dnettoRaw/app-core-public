// =============================================================================
//        #######
//     ###       ###     F: scene_commands.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:59:22 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::{scene_storage::reserved, EntityId, RenderError, SceneWorld, Velocity};
/// Deferred structural operations, applied only when mutable world ownership is available.
#[derive(Debug, Clone, Copy)]
pub enum SceneCommand {
    /// Migrate to/from the velocity archetype without changing identity.
    SetVelocity(EntityId, Option<Velocity>),
    /// Remove one live entity; stale/foreign identities return a controlled error.
    Despawn(EntityId),
}
/// Independent bounded structural queue. Application owns ordering and frame boundaries.
pub struct SceneCommands {
    commands: Vec<SceneCommand>,
    capacity: usize,
}
impl SceneCommands {
    /// Reserve 1..256 operations; no workers, timers or automatic application.
    pub fn new(capacity: usize) -> Result<Self, RenderError> {
        if !(1..=256).contains(&capacity) {
            return Err(RenderError::InvalidLimits);
        }
        Ok(Self {
            commands: reserved(capacity)?,
            capacity,
        })
    }
    /// Queue before world mutation; overflow leaves existing queued operations intact.
    pub fn push(&mut self, command: SceneCommand) -> Result<(), RenderError> {
        if self.commands.len() == self.capacity {
            return Err(RenderError::BudgetExceeded);
        }
        self.commands.push(command);
        Ok(())
    }
    /// Apply in order, returning one outcome per command. This batch is explicitly nontransactional:
    /// an invalid command does not undo earlier effects or discard subsequent commands.
    /// Outcome allocation is admitted before any mutation; no user callbacks run inside the batch.
    pub fn apply(
        self,
        world: &mut SceneWorld,
    ) -> Result<Vec<Result<(), RenderError>>, RenderError> {
        let mut outcomes = reserved(self.commands.len())?;
        for command in self.commands {
            outcomes.push(match command {
                SceneCommand::SetVelocity(entity, value) => world.set_velocity(entity, value),
                SceneCommand::Despawn(entity) => world.despawn(entity),
            });
        }
        Ok(outcomes)
    }
}
