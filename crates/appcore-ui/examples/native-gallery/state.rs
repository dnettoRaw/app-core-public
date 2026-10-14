// =============================================================================
//        #######
//     ###       ###     F: state.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Application-owned deterministic presentation commands, also callable by a future tool adapter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Editing,
    Review,
    Applied,
}
#[derive(Clone)]
pub struct Draft {
    pub name: String,
    pub mode: usize,
    pub enabled: bool,
    pub strength: f64,
}
pub enum Command {
    Review(Draft),
    Apply,
    Edit,
    Reset,
}
pub struct State {
    pub revision: u64,
    pub phase: Phase,
    pub draft: Draft,
}
impl Default for State {
    fn default() -> Self {
        Self {
            revision: 0,
            phase: Phase::Editing,
            draft: Draft {
                name: "Local workspace".into(),
                mode: 0,
                enabled: true,
                strength: 50.0,
            },
        }
    }
}
impl State {
    pub fn execute(&mut self, expected: u64, command: Command) -> Result<(), &'static str> {
        if expected != self.revision {
            return Err("stale revision");
        }
        let next = self.revision.checked_add(1).ok_or("revision exhausted")?;
        match command {
            Command::Review(draft) if self.phase == Phase::Editing => {
                if draft.name.trim().is_empty()
                    || draft.name.len() > 80
                    || draft.name.contains('\0')
                    || draft.mode > 2
                    || !draft.strength.is_finite()
                    || !(0.0..=100.0).contains(&draft.strength)
                {
                    return Err("invalid draft");
                }
                self.draft = draft;
                self.phase = Phase::Review;
            }
            Command::Apply if self.phase == Phase::Review => self.phase = Phase::Applied,
            Command::Edit if self.phase != Phase::Editing => self.phase = Phase::Editing,
            Command::Reset => {
                let draft = Self::default().draft;
                self.draft = draft;
                self.phase = Phase::Editing;
            }
            _ => return Err("invalid transition"),
        }
        self.revision = next;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_or_forbidden_commands_do_not_apply_and_valid_flow_completes() {
        let mut state = State::default();
        assert_eq!(state.execute(0, Command::Apply), Err("invalid transition"));
        let mut invalid = state.draft.clone();
        invalid.name.clear();
        assert_eq!(
            state.execute(0, Command::Review(invalid)),
            Err("invalid draft")
        );
        assert_eq!(state.revision, 0);
        state
            .execute(0, Command::Review(state.draft.clone()))
            .unwrap();
        assert_eq!(state.execute(0, Command::Apply), Err("stale revision"));
        assert_eq!(state.phase, Phase::Review);
        state.execute(1, Command::Apply).unwrap();
        assert_eq!(state.phase, Phase::Applied);
        assert_eq!(state.execute(2, Command::Apply), Err("invalid transition"));
        state.execute(2, Command::Edit).unwrap();
        assert_eq!(state.phase, Phase::Editing);
    }
}
