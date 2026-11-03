//! The damage sprites' check (`FUN_1411859a0`), which the static part runs around the hit
//! points' change and the upgrade around the master's: the share of an object's hit points, or
//! of its storage, that picks its master's damage sprite.

use crate::GenieError;
use crate::math::conversions::cvttss2si;
use crate::math::image::upgrade::WHOLE as UPGRADE_WHOLE;
use crate::math::rng::Well512;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::layers::MotionMut;
use crate::objects::masters::DamageSpriteMode;
use crate::objects::masters::HpPercent;
use crate::objects::masters::Master;
use crate::objects::sprite::{ActiveSprite, new_sprite};
use crate::objects::unit::Unit;
use crate::technology::upgrade::NO_SPRITE;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpriteTuning;

/// The whole of a damage share (`DAT_14330ebcc`, 100).
const WHOLE: f32 = UPGRADE_WHOLE;

/// What a damage sprite apply mode does to the simulation (`FUN_1411859a0`'s switch on the
/// entry's `+0x9`).
pub(super) const SPRITE_PLACED: DamageSpriteMode = DamageSpriteMode::PLACED;
pub(super) const SPRITE_SCATTERED: DamageSpriteMode = DamageSpriteMode::SCATTERED;
pub(super) const SPRITE_SHOWN: DamageSpriteMode = DamageSpriteMode::SHOWN;

/// `FUN_141193590`: an object of `master` in `state` whose damage share is its storage's
/// (`+0x681` set, or a resource class: those of [`ClassRoles::DAMAGE_BY_STORAGE`] and
/// [`ClassRoles::DAMAGE_BY_STORAGE_DEAD`] in a state above 2).
fn storage_share(master: &Master, state: ObjectState, roles: &Roles) -> bool {
    master.auto_gather.is_on()
        || ((roles
            .of_class(master.class)
            .contains(ClassRoles::DAMAGE_BY_STORAGE)
            || roles
                .of_class(master.class)
                .contains(ClassRoles::DAMAGE_BY_STORAGE_DEAD))
            && ObjectState::Alive < state)
}

/// The damage sprites' check (`FUN_1411859a0`) of object `u` reading `master`, whose damage share
/// (`+0xd`) is `before`: a master without damage sprites leaves it. Else the share is that of its
/// storage left for a resource object ([`storage_share`]: the first storage entry of the kind it
/// holds, `100 - amount * 100 / storage`, CVTTSS2SI; 0 for none), or of its hit points, which
/// first stay within 0 and the master's (COMISS: a NaN stays; a master below 1 hit point leaves
/// the share). A changed share below 100 that moves the object to another damage sprite (the
/// last entry whose percent lies below the share, the old one's below the old share) removes the
/// object's attachments and shows the new entry by its apply mode: mode 0 places it on the
/// object (`FUN_14118a9d0`, else a decal), mode 1 at a random spot of its footprint (two draws of
/// the Object stream, `CheckDamageSprites`, then a decal), mode 2 as the object's sprite (slot
/// 0x68); then the old entry goes: a decal of mode 0 or 1, or for mode 2 the master's standing
/// sprite (slots 0x798 and 0x68) unless the new entry also shows one. The attachments and decals
/// are drawn, not simulated. Returns the share (`+0xd`), which the slice does not hold.
///
/// # Errors
///
/// A sprite the data file does not have.
pub(crate) fn damage_sprites(
    (u, mut motion): (&mut Unit, MotionMut<'_>),
    master: &Master,
    before: HpPercent,
    (graphics, object, sprite): (&Graphics, &mut Well512, &mut Well512),
    roles: &Roles,
    sprite_tuning: SpriteTuning,
) -> Result<HpPercent, GenieError> {
    let entries = &master.damage_graphics;
    let Some((share, hp)) = share_of(u, master, roles) else {
        return Ok(before);
    };
    u.life.hp = HitPoints::new(hp);
    if share == before || HpPercent::WHOLE <= share {
        return Ok(share);
    }
    let (mut now, mut was) = (None, None);
    for (i, e) in entries.iter().enumerate() {
        if e.percent < share {
            now = Some(i);
        }
        if e.percent < before {
            was = Some(i);
        }
    }
    if now == was {
        return Ok(share);
    }
    let shown = |i: Option<usize>| {
        i.and_then(|i| entries.get(i).copied())
            .filter(|e| e.sprite.is_some())
    };
    let graphic =
        |id: Option<SpriteId>| id.and_then(|id| graphics.get(id)).copied().ok_or(NO_SPRITE);
    if let Some(e) = shown(now) {
        match e.apply_mode {
            SPRITE_SCATTERED => {
                let _ = (object.rand15(), object.rand15());
            }
            SPRITE_SHOWN if !graphics.is_empty() => {
                if u.sprite.is_some() {
                    new_sprite(
                        &mut u.sprite,
                        motion.move_stats_mut(),
                        graphic(e.sprite)?,
                        sprite,
                        sprite_tuning,
                    );
                } else {
                    let _ = ActiveSprite::start(graphic(e.sprite)?, sprite, sprite_tuning);
                }
            }
            _ => {}
        }
    }
    if let Some(e) = shown(was)
        && e.apply_mode == SPRITE_SHOWN
        && shown(now).is_none_or(|n| n.apply_mode != SPRITE_SHOWN)
        && !graphics.is_empty()
    {
        let standing = graphic(master.standing_sprite)?;
        if u.sprite.is_some() {
            new_sprite(
                &mut u.sprite,
                motion.move_stats_mut(),
                standing,
                sprite,
                sprite_tuning,
            );
        } else {
            let _ = ActiveSprite::start(standing, sprite, sprite_tuning);
        }
    }
    let _ = SPRITE_PLACED;
    Ok(share)
}

/// The damage share [`damage_sprites`] finds for object `u` under `master`, with its hit points
/// after it (within 0 and the master's for a share of hit points); None where it leaves the
/// share (no damage sprites, or a master below 1 hit point).
fn share_of(u: &Unit, master: &Master, roles: &Roles) -> Option<(HpPercent, f32)> {
    if master.damage_graphics.is_empty() {
        return None;
    }
    let mut hp = u.life.hp.get();
    if storage_share(master, u.life.state, roles) {
        let held = master
            .storage
            .iter()
            .take(3)
            .find(|&&(kind, _, _)| kind == u.life.attribute_kind);
        let share = held.map_or(HpPercent::of_data(0), |&(_, amount, _)| {
            HpPercent::of_data(cvttss2si(WHOLE - u.life.amount.get() * WHOLE / amount.get()) as u8)
        });
        return Some((share, hp));
    }
    if master.hit_points.get() < 1 {
        return None;
    }
    let most = HitPoints::of_master(master.hit_points).get();
    // COMISS hp, most; JBE: a NaN keeps; then COMISS 0, hp; JBE
    if most < hp {
        hp = most;
    } else if hp < 0.0 {
        hp = 0.0;
    }
    Some((
        HpPercent::of_data(cvttss2si(WHOLE - hp * WHOLE / most) as u8),
        hp,
    ))
}

/// The damage share (`+0xd`) object `u` holds under `master`, which every change of its hit
/// points or master set through [`damage_sprites`] (the slice does not hold it); 0 where that
/// check would leave it.
pub(super) fn damage_share_of(u: &Unit, master: &Master, roles: &Roles) -> HpPercent {
    share_of(u, master, roles).map_or(HpPercent::of_data(0), |(s, _)| s)
}

#[cfg(test)]
mod tests {
    use crate::objects::HitPoints;
    use crate::objects::MasterHitPoints;
    use crate::objects::ObjectState;
    use crate::objects::lifecycle::Lifecycle;
    use crate::objects::masters::{DamageGraphic, DamageSpriteMode, HpPercent, Master};
    use crate::objects::unit::Unit;
    use crate::players::Amount;
    use crate::test_ids::{master_id, object_id};
    use crate::testing::attribute;
    use crate::testing::class;
    use crate::testing::roles;

    /// `FUN_141193590`: a dead tree's share is that of its storage left (the entry of the kind
    /// it holds; none, 0), a living one's that of its hit points.
    #[test]
    fn resource_object_damage_share_is_its_storage_s() {
        let tree = Master {
            class: class::TREE,
            hit_points: MasterHitPoints::of_data(20),
            storage: vec![(Some(attribute::WOOD), Amount::new(100.0), 0)],
            damage_graphics: vec![DamageGraphic {
                sprite: None,
                percent: HpPercent::of_data(50),
                apply_mode: DamageSpriteMode::of_data(0),
            }],
            ..Master::default()
        };
        let mut u = Unit {
            id: object_id(1),
            life: Lifecycle {
                master_id: master_id(1),
                state: ObjectState::Dying,
                hp: HitPoints::new(20.0),
                ..Lifecycle::default()
            },
            ..Unit::default()
        };
        (u.life.attribute_kind, u.life.amount) = (Some(attribute::WOOD), Amount::new(25.0));
        assert_eq!(
            super::share_of(&u, &tree, &roles()),
            Some((HpPercent::of_data(75), 20.0))
        );
        u.life.attribute_kind = Some(attribute::STONE);
        assert_eq!(
            super::share_of(&u, &tree, &roles()),
            Some((HpPercent::of_data(0), 20.0))
        );
        u.life.state = ObjectState::Alive;
        u.life.hp = HitPoints::new(5.0);
        assert_eq!(
            super::share_of(&u, &tree, &roles()),
            Some((HpPercent::of_data(75), 5.0))
        );
    }
}
