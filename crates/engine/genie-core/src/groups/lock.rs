//! A group's formation type (FD-036): `CalcFormationType` (`FUN_14100b940`), the members' vote
//! after a change of members; `LockFormation` (`FUN_141024e70`), the lock to a type; and the
//! members' stances as the group's state (`FUN_14100d140`).

use crate::GenieResult;
use crate::groups::FormationKind;
use crate::groups::GroupId;
use crate::groups::create::FormationUnit;
use crate::map::Distance;
use crate::objects::ObjectId;
use crate::unit_ai::UnitStance;
use crate::world::World;
use crate::world::slots::CheckedIndexExt;

impl World {
    /// `CalcFormationType` (`FUN_14100b940`): the members' unit AIs' formation types (`+0x1d4`,
    /// those below 9) counted, the most common the type (the first of equals); one below 3 unlocks
    /// the group (`mFormationLockedType` 0) and makes its formation anew
    /// ([`World::update_desired_positions`] with -1), the members' stances its state and their
    /// formation types 0 ([`World::stances_state`]); a higher one locks the group to it
    /// ([`World::lock_formation`]).
    ///
    /// # Errors
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn calc_formation_kind(&mut self, group: GroupId) -> GenieResult<()> {
        let ids: Vec<ObjectId> = self
            .groups
            .held(group)?
            .members
            .iter()
            .map(|member| member.id)
            .collect();
        let mut counts = [0i32; 9];
        for &id in &ids {
            if let Some(vote) = self
                .parts
                .unit_ai
                .get(id)
                .and_then(|ai| ai.formation_kind.vote())
            {
                *counts.checked_mut(vote)? += 1;
            }
        }
        let mut best = 0;
        for index in 1..9 {
            if *counts.checked(best)? < *counts.checked(index)? {
                best = index;
            }
        }
        if 3 <= best {
            return self.lock_formation(group, FormationKind::of_vote(best));
        }
        if let Some(group) = self.groups.get_mut(group) {
            group.formation_locked_kind = Some(FormationKind::Horde);
        }
        self.update_desired_positions(group, None)?;
        self.stances_state(group, FormationKind::Horde)?;
        // FUN_1415417a0 finds sleeping members too
        for &id in &ids {
            if let Some(ai) = self.parts.unit_ai.get_mut(id) {
                ai.formation_kind = FormationKind::Horde;
            }
        }
        Ok(())
    }

    /// `LockFormation` (`FUN_141024e70`) of group `group` to type `kind`: a group without a
    /// spacing yet (`mDesiredSpacing` 0) takes its held members' widest radius twice
    /// ([`FormationUnit`]); a column or a line (1, 2) unlocks it (`mFormationLockedType` 0) to
    /// its own choice ([`World::update_desired_positions`] with -1), any other type locks it to
    /// that type and lays it; then the members' stances give its state and their formation
    /// types are the lock's ([`World::stances_state`]). The analytics event it sends (each
    /// member's master counted, `FUN_140986b60`) changes nothing in the world.
    ///
    /// # Errors
    ///
    /// What [`World::update_desired_positions`] does not
    /// model.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn lock_formation(&mut self, group: GroupId, kind: FormationKind) -> GenieResult<()> {
        let held = self.groups.held(group)?;
        // the spacing is read again for every member held (0x14102500a): the first whose
        // radius gives one above it sets it, and every later member finds it set
        let mut spacing = held.desired_spacing;
        for member in &held.members {
            if spacing != Distance::ZERO {
                break;
            }
            if let Some((player_id, held)) = self.object(member.id) {
                let unit = FormationUnit::of(
                    held.master(self.master_tables(), player_id)?,
                    &self.model.tuning.group,
                );
                let distance = unit.radius * self.model.tuning.group.spacing_factor;
                if spacing <= distance && distance != spacing {
                    spacing = distance;
                }
            }
        }
        self.groups.held_mut(group)?.desired_spacing = spacing;
        let own_choice = matches!(kind, FormationKind::Column | FormationKind::Line);
        let locked = if own_choice {
            FormationKind::Horde
        } else {
            kind
        };
        self.groups.held_mut(group)?.formation_locked_kind = Some(locked);
        self.update_desired_positions(group, (!own_choice).then_some(kind))?;
        self.stances_state(group, locked)?;
        Ok(())
    }

    /// `FUN_14100d140` with 1: the group's state (`mState`) is the members' most common stance
    /// (`+0x1a9`: 0 aggressive, 1 defensive, 2 stand ground, 3 no attack; on equal counts the
    /// lower), 0 without members whose unit AI the world holds; each such member's formation
    /// type (`+0x1d4`) becomes `formation_type`.
    pub fn stances_state(
        &mut self,
        group: GroupId,
        formation_kind: FormationKind,
    ) -> GenieResult<()> {
        let Some(unit_group) = self.groups.get(group) else {
            return Ok(());
        };
        let ids: Vec<ObjectId> = unit_group.members.iter().map(|member| member.id).collect();
        let mut stances = [0i32; 4];
        for &id in &ids {
            if let Some(at) = self
                .parts
                .unit_ai
                .get(id)
                .and_then(|ai| ai.stance.counted())
            {
                *stances.checked_mut(at)? += 1;
            }
        }
        let stance = if ids.is_empty()
            || (stances[1] <= stances[0] && stances[2] <= stances[0] && stances[3] <= stances[0])
        {
            UnitStance::Aggressive
        } else if stances[2] <= stances[1] && stances[3] <= stances[1] {
            UnitStance::Defensive
        } else if stances[3] <= stances[2] {
            UnitStance::StandGround
        } else {
            UnitStance::NoAttack
        };
        if let Some(group) = self.groups.get_mut(group) {
            group.stance = Some(stance);
        }
        // FUN_1415417a0 finds sleeping members too
        for &id in &ids {
            if let Some(ai) = self.parts.unit_ai.get_mut(id) {
                ai.formation_kind = formation_kind.unit_byte();
            }
        }
        Ok(())
    }
}
