//! The effects' `do_effect` (`FUN_141462440`, then the base `FUN_140ec2ac0`): each command by
//! its kind for every party ([`Party`]).

use crate::GenieResult;
use crate::objects::ObjectId;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::players::attribute::AttributeId;
use crate::technology::DisabledTech;
use crate::technology::EffectId;
use crate::technology::{EffectCommand, Party};
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::slots::PlayerSlots;
use std::sync::Arc;

impl World {
    /// The players party `party` reaches from player `p`: for a relation, each player past Gaia
    /// (`FUN_14152fdd0`, from 1, the player itself when it counts itself so) whose and to whom
    /// the relations (`+0x260`, `FUN_141667f50`) are the party's, in order.
    pub(super) fn party(&self, p: PlayerId, party: Party) -> Vec<PlayerId> {
        let relation =
            |a: PlayerId, b: PlayerId| self.players.player(a).ok()?.relations.get(b).copied();
        let mutual = |q: PlayerId, wanted: &[Relation]| {
            wanted
                .iter()
                .any(|&r| relation(p, q) == Some(r) && relation(q, p) == Some(r))
        };
        let wanted: &[Relation] = match party {
            Party::Own => return vec![p],
            Party::Gaia => return vec![PlayerId::GAIA],
            Party::Allies => &[Relation::Ally],
            Party::Enemies => &[Relation::Enemy, Relation::Relation4],
            Party::Neutrals => &[Relation::Neutral],
        };
        PlayerId::each(self.players.len())
            .skip(1)
            .filter(|&q| mutual(q, wanted))
            .collect()
    }

    /// `TRIBE_Effects::do_effect` (`FUN_141462440`) of effect `id` of the world's effect table
    /// (`+0x128`, [`crate::technology::Effects::get`], `FUN_140ec4510`): its commands
    /// ([`World::do_effect`]) for player `p` with the effect's object `object`; an id outside
    /// the table does nothing.
    pub(crate) fn do_table_effect(
        &mut self,
        p: PlayerId,
        id: EffectId,
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        let effects = Arc::clone(&self.model.effects);
        effects
            .get(id)
            .map_or(Ok(()), |e| self.do_effect(p, &e.commands, object))
    }

    /// `TRIBE_Effects::do_effect` (`FUN_141462440`) of an effect's commands `effect` (those of an
    /// effect of the table, [`World::do_table_effect`]; its 0x318 makes the spawns count in the
    /// local player's statistics, which are not simulation) for player `p` with the effect's object
    /// `object` (none for a research's): first its technology commands in order, the cost modifier
    /// (`'e'`, [`World::tech_cost`]), the disable command (`'f'`, an id other than −1,
    /// [`World::disable_tech`]) and the research time modifier (`'g'`, [`World::research_time`]);
    /// then the base's ([`World::base_effect`]).
    pub(crate) fn do_effect(
        &mut self,
        p: PlayerId,
        effect: &[EffectCommand],
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        for c in effect {
            match *c {
                EffectCommand::TechCost {
                    tech: Some(tech),
                    resource,
                    amount,
                    mode,
                } => self.tech_cost(p, tech, resource, amount, mode)?,
                EffectCommand::TechDisable { tech } => {
                    if tech != DisabledTech::NONE {
                        self.disable_tech(p, tech, object)?;
                    }
                }
                EffectCommand::TechTime {
                    tech: Some(tech),
                    amount,
                    mode,
                } => self.research_time(p, tech, amount, mode, None)?,
                _ => {}
            }
        }
        self.base_effect(p, effect, object)
    }

    /// `RGE_Effects::do_effect` (`FUN_140ec2ac0`): each command by its kind, for the players of
    /// its block ([`Party`]); a kind it does not know (the technology commands among them) does
    /// nothing. After the commands, the player's change of the farms' food (resource 0x24) grows
    /// the food its farms hold ([`World::farm_food`]), by the last such command's value before
    /// and amount.
    pub(super) fn base_effect(
        &mut self,
        p: PlayerId,
        effect: &[EffectCommand],
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        let mut farms: Option<(f32, f32)> = None;
        for c in effect {
            match *c {
                EffectCommand::Objects(kind) => Self::object_effect(kind, object)?,
                EffectCommand::Attribute { .. } | EffectCommand::Costs { .. } => {
                    self.attribute_command(p, c)?;
                }
                EffectCommand::Resource { .. } | EffectCommand::ResourceMultiply { .. } => {
                    self.resource_command(p, c, &mut farms)?;
                }
                EffectCommand::Enable { .. }
                | EffectCommand::Upgrade { .. }
                | EffectCommand::Spawn { .. }
                | EffectCommand::Tech { .. } => self.master_command(p, c, object)?,
                // the technology commands, and the kinds no switch knows
                EffectCommand::TechCost { .. }
                | EffectCommand::TechDisable { .. }
                | EffectCommand::TechTime { .. }
                | EffectCommand::Inert(_) => {}
            }
        }
        if let Some((before, amount)) = farms {
            self.farm_food(p, before, amount)?;
        }
        Ok(())
    }

    /// [`World::base_effect`]'s attribute changes (kinds 0, 4 and 5 of a block): set and add by
    /// slot 0x148 ([`World::modify_attribute`]) for each player of the block; multiply too, but
    /// a cost attribute by a negative amount changes the cost entries instead, except in Gaia's
    /// block ([`EffectCommand::Costs`], `FUN_141666ee0`).
    pub(super) fn attribute_command(&mut self, p: PlayerId, c: &EffectCommand) -> GenieResult<()> {
        match *c {
            EffectCommand::Attribute {
                party,
                change,
                master,
                class,
                attribute,
                amount,
            } => {
                for q in self.party(p, party) {
                    self.modify_attribute(q, change, master, class, amount, attribute)?;
                }
            }
            EffectCommand::Costs {
                party,
                units,
                attribute,
                edit,
            } => {
                for q in self.party(p, party) {
                    self.cost_entries(q, units, attribute, edit);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// [`World::base_effect`]'s resource changes: kind 1 by the amount the effect's player's
    /// attribute `scale` scales ([`World::scaled_amount`]; the own block's farm food is noted in
    /// `farms`, its value before and the amount, else a resource below 0 is skipped), kind 6 a
    /// multiply of a resource 0 or above; each for the block's players.
    pub(super) fn resource_command(
        &mut self,
        p: PlayerId,
        c: &EffectCommand,
        farms: &mut Option<(f32, f32)>,
    ) -> GenieResult<()> {
        match *c {
            EffectCommand::ResourceMultiply {
                party,
                attribute: Some(attribute),
                amount,
            } => {
                for q in self.party(p, party) {
                    self.multiply_resource(q, attribute, amount)?;
                }
            }
            EffectCommand::Resource {
                party,
                attribute,
                change,
                scale,
                amount,
            } => {
                // the effect's player's attribute `scale` scales the amount
                let amount = Amount::new(self.scaled_amount(p, scale, amount));
                // a negative attribute is none: nothing changes
                let Some(attribute) = attribute else {
                    return Ok(());
                };
                if party == Party::Own
                    && self
                        .model
                        .roles
                        .of_attribute(attribute)
                        .contains(AttributeRoles::FARM_FOOD_AMOUNT)
                {
                    *farms = Some((
                        self.attribute_of(p, self.model.slots.attribute.farm_food_amount)
                            .get(),
                        amount.get(),
                    ));
                }
                for q in self.party(p, party) {
                    self.modify_resource(q, attribute, amount, change)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// [`World::base_effect`]'s master commands for the block's players: the enable (a master 0
    /// or above; the own block's with its one-object flag and the effect's object), the upgrade
    /// (the own block's with the effect's object), the spawn and the technology change.
    pub(super) fn master_command(
        &mut self,
        p: PlayerId,
        c: &EffectCommand,
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        match *c {
            EffectCommand::Enable {
                party,
                master: Some(unit),
                enable,
                one,
            } => {
                let own = party == Party::Own;
                for q in self.party(p, party) {
                    let (one, object) = if own { (one, object) } else { (false, None) };
                    self.enable_master(q, unit, enable, one, object)?;
                }
            }
            EffectCommand::Upgrade {
                party,
                from,
                to,
                mode,
            } => {
                for q in self.party(p, party) {
                    let object = object.filter(|_| party == Party::Own);
                    self.upgrade_master(q, (from, to), mode, object)?;
                }
            }
            EffectCommand::Spawn {
                party,
                master,
                at,
                count,
            } => {
                for q in self.party(p, party) {
                    self.spawn_at(q, (master, at), count)?;
                }
            }
            EffectCommand::Tech {
                party,
                tech,
                mode,
                amount,
                location,
            } => {
                let Some(tech) = tech else {
                    return Ok(());
                };
                for q in self.party(p, party) {
                    self.modify_tech(q, tech, mode, amount, location)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A resource command's amount (`FUN_140ec2ac0`'s kinds 1, 11, ...): its value `amount`
    /// times the effect's player's attribute `scale`, when the player has it.
    pub(super) fn scaled_amount(
        &self,
        p: PlayerId,
        scale: Option<AttributeId>,
        amount: f32,
    ) -> f32 {
        scale
            .and_then(|a| self.players.get(p.index())?.attribute(a))
            .map_or(amount, |scale| (amount * scale).get())
    }

    /// Player `p`'s attribute `attribute`, 0 past its count (`+0x90`, the saved attributes').
    pub(super) fn attribute_of(&self, p: PlayerId, attribute: AttributeId) -> Amount {
        self.players
            .get(p.index())
            .and_then(|player| player.attribute(attribute))
            .unwrap_or(Amount::ZERO)
    }
}
