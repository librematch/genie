//! The swing's state (7), slot 9 (`RGE_Action_Attack::work`) and slot 0x68
//! (`set_target_obj`).

use crate::combat::AttackTargetId;
use crate::combat::attack::AttackState;
use crate::combat::attack::{
    Attack, AttackReads, Fight, Ground, NO_OBJECT_STREAM, reset_sprite, set_walker_anim,
};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::movement::MoveAlgorithm;
use crate::movement::Speed;
use crate::movement::footprint_gap;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::{GenieError, combat::damage};

/// An object as the Attack's slot 9 ([`Attack::work`]) and slot 0x68 ([`Attack::set_target`])
/// read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    /// The object.
    pub id: ObjectId,
    /// Its master's type (80 a building).
    pub kind: MasterKind,
    /// Its master's class.
    pub class: ClassId,
    /// Its state (`+0x640`).
    pub state: ObjectState,
    /// Its place (decoded `x`, `y`, `z`).
    pub position: Position,
    /// Its master's radii on the ground, which the footprint gap (`FUN_14118f680`) reads.
    pub radius: Footprint,
}

/// What the Attack's slot 9 ([`Attack::work`]) reads of its unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wielder {
    /// Its master's `area_effect_range` (`+0x82c`).
    pub area_effect: Distance,
    /// Its top speed (slot 0x208).
    pub speed: Speed,
    /// Its place.
    pub position: Position,
    /// Its master's radii on the ground.
    pub radius: Footprint,
}

/// What the Attack's slot 9 or slot 0x68 leaves to its caller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Worked {
    /// Slot 9 took the target (its answer, 1).
    pub kept: bool,
    /// Slot 0x68 took another target: the unit's attack count (`+0x900`) clears.
    pub clears_count: bool,
    /// The old target, whose slot 0x180 lets the unit go when the world still holds it.
    pub lets_go: Option<ObjectId>,
    /// The new target, whose slot 0x178 takes the unit with the action's type (9).
    pub takes: Option<ObjectId>,
}

impl Attack {
    /// `set_state(7)` (case 7): the swing. Its sprite: the terrain look's attack sprite while
    /// the unit shows its look ([`SwingSprites::look`]); else the master's special graphic on a
    /// charge that hits the target (slot 0x2f0, [`crate::combat::damage::charge_hit`]; slot 0x2f8's
    /// charge range is a charge of types 6 and 7, which [`engagement`] does not take); else the
    /// attack sprite (`+0x70`) or, with a second one (`+0x78`), a draw of the Object stream
    /// (`FUN_140810fc0` with 2, [`crate::math::rng::Well512::below_checked`]: 0 the first) for a master
    /// whose move algorithm lacks [`crate::movement::MoveAlgorithm::ALTERNATES_SPRITES`] (slot
    /// 0x5d8); one with it takes the second after a stop
    /// (`+0x958`, not held). The unit takes it (without one slot 0x68 shows its master's standing
    /// sprite) from its first frame (`FUN_1413b9f00`); a hit is pending; a reload shorter than
    /// the sprite speeds the animation up to fit it.
    ///
    /// [`SwingSprites::look`]: crate::combat::attack::sprites::SwingSprites::look
    /// [`engagement`]: crate::combat::attack::constructor::engagement
    pub(super) fn set_swing(
        &mut self,
        unit: &mut Unit,
        g: &mut Ground<'_>,
        f: &Fight,
        r: &AttackReads,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        self.enter(AttackState::Swing, g.motion.reborrow())?;
        let alternates = g.motion.mover().is_some_and(|m| {
            m.stats
                .move_algorithm
                .contains(MoveAlgorithm::ALTERNATES_SPRITES)
        });
        let special = r.swing_sprites.special.filter(|_| {
            MasterKind::COMBAT <= f.own.kind && damage::charge_hit(&f.own.source, &f.victim, roles)
        });
        let chosen = if let Some(look) = r.swing_sprites.look {
            Some(look)
        } else if let Some(special) = special {
            Some(special)
        } else if let Some(second) = self.arms.second_sprite {
            if alternates {
                return Err(GenieError::unsupported(
                    "an attacker with a second attack sprite (its draw)",
                ));
            }
            let object = g.object_rng.as_deref_mut().ok_or(NO_OBJECT_STREAM)?;
            if object.below_checked(2) == 0 {
                self.arms.sprite
            } else {
                Some(second)
            }
        } else {
            self.arms.sprite
        };
        let shown = chosen.or(r.standing).ok_or(GenieError::unsupported(
            "an attacker without an attack or standing sprite",
        ))?;
        new_sprite(
            &mut unit.sprite,
            g.motion.move_stats_mut(),
            shown,
            g.sprite_rng,
        );
        // FUN_1413b9f00 on the unit's sprite list with its sprite (`+0x140`)
        if let Some(current) = unit.sprite.map(|sp| sp.graphic) {
            let _ = reset_sprite(&mut unit.sprite, current);
        }
        self.pending = true;
        let length = Seconds::new(shown.frames.as_f32() * shown.frame_duration.get());
        self.rest = r.reload - length;
        set_walker_anim(
            g.motion.reborrow(),
            if self.rest < Seconds::ZERO && Seconds::ZERO < r.reload {
                Speed::new(length / r.reload)
            } else {
                Speed::new(1.0)
            },
        );
        Ok(())
    }

    /// Slot 9 (`FUN_140a841d0`) on `target`, the Attack's own target, for a caller that holds no
    /// more of the unit and the target than the id (the work command's `keep_attack`): one in
    /// states 4 to 7 or 11 keeps it as it is. The rest of the slot (the main building of an area
    /// effect's building target, the range test, the new target) is [`Attack::work`]'s, which
    /// needs the unit's and the target's values.
    pub fn keeps(&self, target: Option<ObjectId>) -> Result<bool, GenieError> {
        if target == self.target.map(AttackTargetId::id) && self.state.keeps_target() {
            return Ok(true);
        }
        Err(GenieError::unsupported(
            "an Attack's slot 9 on a target out of its states 4 to 7 and 11",
        ))
    }

    /// Slot 9 (`FUN_140a841d0`, `RGE_Action_Attack::work`) on `target`: none answers false. A
    /// unit whose master has an area effect (`+0x82c` above 0: `COMISS`/`JBE`) takes a building's
    /// (type 80) main building (`+0xbb8`, `main`) for it. The Attack's own target in states 4 to 7
    /// or 11 is kept as it is (true). Else a unit without top speed (slot 0x208 equal to 0:
    /// `UCOMISS`/`JZ`, so a NaN speed asks on) or whose unit AI's canMoveToAttack (slot 0x1d0,
    /// `FUN_140c48420`, on the target's id: `can_move`) is false keeps it only within its reach:
    /// the footprint gap (`FUN_14118f680`) at most the range (`+0x90`), half a tile more for a
    /// range up to 0.1 (`COMISS`/`JA`: a NaN gap is within). Kept, the indirect fire flag
    /// (`+0xa0`, which genie's Attacks never set) clears, the target becomes the Attack's (slot
    /// 0x68, [`Attack::set_target`]) and where it was seen its place (`+0xa4`..`+0xac`); true.
    pub fn work(
        &mut self,
        target: Option<Mark>,
        main: Option<Mark>,
        unit: &Wielder,
        can_move: impl FnOnce(ObjectId) -> bool,
        roles: &Roles,
    ) -> Worked {
        let Some(mut target) = target else {
            return Worked::default();
        };
        if Distance::ZERO < unit.area_effect
            && target.kind == MasterKind::BUILDING
            && let Some(m) = main
        {
            target = m;
        }
        if self.target.map(AttackTargetId::id) == Some(target.id) && self.state.keeps_target() {
            return Worked {
                kept: true,
                ..Worked::default()
            };
        }
        if unit.speed == Speed::ZERO || !can_move(target.id) {
            let gap = footprint_gap(unit.position, unit.radius, target.position, target.radius);
            if self.reach() < gap {
                return Worked::default();
            }
        }
        let mut worked = self.set_target(Some(target), roles);
        self.seen = target.position;
        worked.kept = true;
        worked
    }

    /// Slot 0x68 (`FUN_140a82a10`, `set_target_obj`): a target of class 60 (`-0xe70` raw) changes
    /// nothing; another target than the Attack's clears the unit's attack count (`+0x900`); then
    /// the action's own (`FUN_140a7a230`): a new target lets the old one go (its slot 0x180, when
    /// the world still holds it: the caller's) unless the Attack is a sub-action (`+0x60`), and
    /// becomes the Attack's (`+0x20`, its id `+0x30`) below state 7, taken (its slot 0x178 with
    /// the action's type 9, the caller's) unless a sub-action; one in state 7 or more leaves the
    /// Attack without a target (−1), as does none.
    pub fn set_target(&mut self, target: Option<Mark>, roles: &Roles) -> Worked {
        let mut worked = Worked::default();
        if target.is_some_and(|t| {
            roles
                .of_class(t.class)
                .contains(ClassRoles::NOT_ATTACK_TARGET)
        }) {
            return worked;
        }
        // slot 0x68 gives the Attack its target
        let new = target.map(|t| AttackTargetId::new(t.id));
        if new != self.target {
            worked.clears_count = true;
        }
        if self.target.is_some() && self.target != new && self.voice.is_loud() {
            worked.lets_go = self.target.map(AttackTargetId::id);
        }
        if self.target != new {
            self.target = match target {
                Some(t) if t.state < ObjectState::Removed => {
                    if self.voice.is_loud() {
                        worked.takes = Some(t.id);
                    }
                    new
                }
                _ => None,
            };
        }
        worked
    }
}
