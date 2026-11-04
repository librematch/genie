//! The world's effect table (`+0x128`, `TRIBE_Effects`): every effect's commands by id, the
//! `.dat`'s, the one table a technology's effect id names ([`Effects`], FD-046-D11).

use crate::technology::EffectCommand;
use crate::technology::EffectId;
use crate::technology::Party;
use serde::Deserialize;
use serde::Serialize;

/// An effect: its commands, in the `.dat`'s order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Effect {
    /// The commands, each parsed by its kind ([`EffectCommand::of_data`]).
    pub commands: Vec<EffectCommand>,
}

impl Effect {
    /// It upgrades a unit (a command of kind 3, the player's own), which keeps RESEARCH's queue
    /// to one at a time (`FUN_1411d70f0`).
    #[must_use]
    pub fn upgrades(&self) -> bool {
        self.commands.iter().any(|effect_command| {
            matches!(
                effect_command,
                EffectCommand::Upgrade {
                    party: Party::Own,
                    ..
                }
            )
        })
    }

    /// It marks a master with an upgrade when its research starts: an upgrade of any block
    /// (kinds 3, 13, 23, 33, 43; `FUN_140ec4540` from the research's start, `FUN_1413d1530`,
    /// marks the master with the unit it turns into, `+0x330`).
    #[must_use]
    pub fn marks_upgrade(&self) -> bool {
        self.commands
            .iter()
            .any(|effect_command| matches!(effect_command, EffectCommand::Upgrade { .. }))
    }
}

/// The effect table by id (the world's `+0x128`): a technology's effect (`TechDef::effect_id`),
/// a scenario's and a world effect attribute's read it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Effects(Vec<Effect>);

impl Effects {
    /// The table of `effects`, the first one's id 0.
    #[must_use]
    pub const fn new(effects: Vec<Effect>) -> Self {
        Self(effects)
    }

    /// The effect of id `id`; None for an id outside the table, a negative one too, which
    /// `TRIBE_Effects::do_effect` skips.
    #[must_use]
    pub fn get(&self, id: EffectId) -> Option<&Effect> {
        usize::try_from(id.get())
            .ok()
            .and_then(|index| self.0.get(index))
    }

    /// How many effects the table holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// The table holds no effect.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::EffectCommand;
    use crate::technology::EffectId;
    use crate::technology::effects::{Effect, Effects};

    /// A table of two effects, the second with one command.
    fn table() -> Effects {
        Effects::new(vec![
            Effect::default(),
            Effect {
                commands: vec![EffectCommand::Inert(99)],
            },
        ])
    }

    /// An id inside the table reads its effect; one past its end reads none.
    #[test]
    fn an_effect_id_outside_the_table_reads_as_none() {
        let effects = table();
        assert_eq!(
            effects
                .get(EffectId::of_data(1))
                .map(|effect| effect.commands.len()),
            Some(1)
        );
        assert_eq!(effects.get(EffectId::of_data(2)), None);
        assert_eq!((effects.len(), effects.is_empty()), (2, false));
    }

    /// A negative id (−1, a technology without an effect) reads none.
    #[test]
    fn negative_effect_ids_read_as_none() {
        assert_eq!(table().get(EffectId::of_data(-1)), None);
        assert!(Effects::default().is_empty());
    }
}
