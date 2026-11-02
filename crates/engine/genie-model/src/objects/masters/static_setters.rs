//! The static, animated, moving and action masters' setters.

use crate::map::Distance;
use crate::map::RestrictionId;
use crate::math::fmath::at_most_or_unordered;
use crate::movement::Speed;
use crate::objects::MasterHitPoints;
use crate::objects::WorkRate;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::CarryCapacity;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::DamageGraphicEntry;
use crate::objects::masters::DamageSpriteMode;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::HpPercent;
use crate::objects::masters::InteractionMode;
use crate::objects::masters::InterfaceKind;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MinimapMode;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::Occlusion;
use crate::objects::masters::RunPattern;
use crate::objects::masters::SelectionEffect;
use crate::objects::masters::{
    AttributeWorld, MISSILE_SPEED_CAP, Master, ROUND, SPEED_CAP, add_short, add_sprite, attribute,
    insert_damage_graphic, low8, low16, master_type, multiply_short, resize_damage_graphics,
    select_entry, set_sprite,
};
use crate::objects::masters::{MasterField, MasterId};
use crate::objects::masters::{TrailDensity, TrailMode};
use crate::players::Amount;
use crate::world::Seconds;

impl Master {
    /// `FUN_141126400`, the static master's slot 0x18.
    pub(super) fn set_static(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si_64;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::HIT_POINTS => self.hit_points = MasterHitPoints::of_data(low16(v)),
            a::LINE_OF_SIGHT => self.line_of_sight = Distance::new(v),
            a::GARRISON_CAPACITY => self.garrison = GarrisonCapacity::of_data(low8(v)),
            a::SIZE_X => self.radius = self.radius.with_x(v),
            a::SIZE_Y => self.radius = self.radius.with_y(v),
            a::CARRY_CAPACITY => self.capacity = CarryCapacity::of_data(low16(v)),
            a::STORAGE_1 | a::STORAGE_2 | a::STORAGE_3 => {
                if let Some(amount) = self.storage_mut(attribute) {
                    *amount = Amount::new(f32::from(low16(v)));
                }
            }
            a::FOG_VISIBILITY => self.fog_visibility = FogVisibility::of_data(low8(v)),
            a::OCCLUSION_MODE => self.occlusion = Occlusion::of_data(low8(v)),
            a::SIZE_Z => self.height = Distance::new(v),
            a::CAN_BE_BUILT_ON | a::BLAST_DEFENSE_LEVEL => {
                self.blast_defense = BlastDefense::new(low8(v));
            }
            a::TERRAIN_RESTRICTION_ID => {
                self.terrain_restriction = RestrictionId::of_data(low16(v));
            }
            // `CVTTSS2SI RAX`, the low 32 bits
            a::UNIT_TRAIT => {
                self.object_flags = ObjectFlags::from_bits_retain(cvttss2si_64(v) as u32);
            }
            a::TRAIT_PIECE => {
                self.object_flags = ObjectFlags::from_bits_retain(
                    ((cvttss2si_64(v) as u32) << 16) | (self.object_flags.bits() & 0xffff),
                );
            }
            a::DEAD_UNIT_ID => self.death_spawns[0] = MasterId::of_data(low16(v)),
            a::BLOOD_UNIT => self.death_spawns[1] = MasterId::of_data(low16(v)),
            a::STANDING_GRAPHIC | a::STANDING_GRAPHIC_2 | a::DYING_GRAPHIC | a::UNDEAD_GRAPHIC => {
                self.set_static_sprite(attribute, v, world);
            }
            a::OBSTRUCTION_TYPE => {
                self.obstruction_type = ObstructionType::set(low8(v));
            }
            a::BLOCKAGE_CLASS => self.obstruction_class = ObstructionClass::from(low8(v)),
            a::SELECTION_EFFECT => self.selection_effect = SelectionEffect::of_data(low8(v)),
            a::DAMAGE_GRAPHIC
            | a::DAMAGE_GRAPHICS_ENTRY_MOD
            | a::DAMAGE_GRAPHICS_TOTAL_NUM
            | a::DAMAGE_GRAPHIC_PERCENT
            | a::DAMAGE_GRAPHIC_APPLY_MODE => self.set_damage_graphic(attribute, v, world),
            a::AVAILABLE_UNIT_FLAG
            | a::DISABLED_UNIT_FLAG
            | a::INTERFACE_KIND
            | a::COMBAT_LEVEL
            | a::INTERACTION_MODE
            | a::MINIMAP_MODE => self.set_static_level(attribute, v),
            // the icon (25), the name and description (50, 51) and the sounds (92 to 99) are the
            // interface's
            _ => {}
        }
    }

    /// [`Master::set_static`]'s flags and levels (`FUN_141126400`): available, disabled, the
    /// interface kind, the combat level, the interaction and minimap modes.
    pub(super) fn set_static_level(&mut self, attribute: MasterAttribute, v: f32) {
        use crate::objects::masters::attribute as a;
        match attribute {
            // the save's byte, which the slice holds as its test against 0
            a::AVAILABLE_UNIT_FLAG => self.available = low8(v) != 0,
            a::DISABLED_UNIT_FLAG => self.disabled = low8(v) != 0,
            a::INTERFACE_KIND => self.unit_level = InterfaceKind::of_data(low8(v)),
            a::COMBAT_LEVEL => self.combat_level = CombatLevel::of_data(low8(v)),
            a::INTERACTION_MODE => self.select_level = InteractionMode::of_data(low8(v)),
            a::MINIMAP_MODE => self.map_draw_level = MinimapMode::of_data(low8(v)),
            _ => {}
        }
    }

    /// [`Master::set_static`]'s sprites (`FUN_141126400`): standing, second standing, dying and
    /// undead, each a sprite the world's table holds.
    pub(super) fn set_static_sprite(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::STANDING_GRAPHIC => set_sprite(&mut self.standing_sprite, cvttss2si(v), world),
            a::STANDING_GRAPHIC_2 => set_sprite(&mut self.standing_sprite_2, cvttss2si(v), world),
            a::DYING_GRAPHIC => set_sprite(&mut self.death_sprite, cvttss2si(v), world),
            a::UNDEAD_GRAPHIC => set_sprite(&mut self.undead_sprite, cvttss2si(v), world),
            _ => {}
        }
    }

    /// [`Master::set_static`]'s damage sprites (`FUN_141126400`): the selected entry's sprite,
    /// percent and apply mode, the selection (at most the last) and the count.
    pub(super) fn set_damage_graphic(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::DAMAGE_GRAPHIC => {
                if let Some(g) = self.damage_graphic_mut() {
                    set_sprite(&mut g.sprite, cvttss2si(v), world);
                }
            }
            a::DAMAGE_GRAPHICS_ENTRY_MOD => {
                if let Some(n) = select_entry(self.damage_graphics.len(), cvttss2si(v)) {
                    self.damage_graphic_entry = DamageGraphicEntry::of_selected(n);
                }
            }
            a::DAMAGE_GRAPHICS_TOTAL_NUM => {
                resize_damage_graphics(
                    &mut self.damage_graphics,
                    &mut self.damage_graphic_entry,
                    v,
                );
            }
            a::DAMAGE_GRAPHIC_PERCENT => {
                if let Some(g) = self.damage_graphic_mut() {
                    g.percent = HpPercent::of_data(low8(v));
                }
            }
            a::DAMAGE_GRAPHIC_APPLY_MODE => {
                if let Some(g) = self.damage_graphic_mut() {
                    g.apply_mode = DamageSpriteMode::of_data(low8(v));
                }
            }
            _ => {}
        }
    }

    /// `FUN_141126af0`, the static master's slot 0x20.
    pub(super) fn add_static(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si_64;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::HIT_POINTS => {
                self.hit_points =
                    MasterHitPoints::of_data(add_short(self.hit_points.get(), v, world.capped));
            }
            a::LINE_OF_SIGHT => self.line_of_sight += Distance::new(v),
            a::GARRISON_CAPACITY => self.garrison = self.garrison.plus(low8(v)),
            a::SIZE_X => self.radius = self.radius.with_x(self.radius.x() + v),
            a::SIZE_Y => self.radius = self.radius.with_y(self.radius.y() + v),
            a::CARRY_CAPACITY => {
                self.capacity =
                    CarryCapacity::of_data(add_short(self.capacity.get(), v, world.capped));
            }
            a::STORAGE_1 | a::STORAGE_2 | a::STORAGE_3 => {
                if let Some(amount) = self.storage_mut(attribute) {
                    *amount += Amount::new(f32::from(low16(v)));
                }
            }
            a::OCCLUSION_MODE => self.occlusion = self.occlusion.plus(low8(v)),
            a::SIZE_Z => self.height += Distance::new(v),
            a::CAN_BE_BUILT_ON | a::BLAST_DEFENSE_LEVEL => {
                self.blast_defense =
                    BlastDefense::new(self.blast_defense.get().wrapping_add(low8(v)));
            }
            a::UNIT_TRAIT => {
                self.object_flags = ObjectFlags::from_bits_retain(
                    self.object_flags
                        .bits()
                        .wrapping_add(cvttss2si_64(v) as u32),
                );
            }
            a::STANDING_GRAPHIC | a::STANDING_GRAPHIC_2 | a::DYING_GRAPHIC | a::UNDEAD_GRAPHIC => {
                self.add_static_sprite(attribute, v, world);
            }
            a::DAMAGE_GRAPHIC
            | a::DAMAGE_GRAPHICS_ENTRY_MOD
            | a::DAMAGE_GRAPHICS_TOTAL_NUM
            | a::DAMAGE_GRAPHIC_PERCENT
            | a::DAMAGE_GRAPHIC_APPLY_MODE => self.add_damage_graphic(attribute, v, world),
            a::AVAILABLE_UNIT_FLAG => {
                self.available = u8::from(self.available).wrapping_add(low8(v)) != 0;
            }
            a::DISABLED_UNIT_FLAG => {
                self.disabled = u8::from(self.disabled).wrapping_add(low8(v)) != 0;
            }
            a::INTERFACE_KIND => self.unit_level = self.unit_level.plus(low8(v)),
            a::COMBAT_LEVEL => self.combat_level = self.combat_level.plus(low8(v)),
            a::INTERACTION_MODE => self.select_level = self.select_level.plus(low8(v)),
            a::MINIMAP_MODE => self.map_draw_level = self.map_draw_level.plus(low8(v)),
            // the sounds (92, 94, 96, 98, through slot 0x18) are the interface's
            _ => {}
        }
    }

    /// [`Master::add_static`]'s sprites (`FUN_141126af0`): standing, second standing, dying and
    /// undead, each its id plus the amount ([`add_sprite`]).
    pub(super) fn add_static_sprite(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::STANDING_GRAPHIC => add_sprite(&mut self.standing_sprite, v, world),
            a::STANDING_GRAPHIC_2 => add_sprite(&mut self.standing_sprite_2, v, world),
            a::DYING_GRAPHIC => add_sprite(&mut self.death_sprite, v, world),
            a::UNDEAD_GRAPHIC => add_sprite(&mut self.undead_sprite, v, world),
            _ => {}
        }
    }

    /// [`Master::add_static`]'s damage sprites (`FUN_141126af0`): the selected entry's sprite,
    /// percent and apply mode, the selection and the count, each from the selected entry.
    pub(super) fn add_damage_graphic(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::DAMAGE_GRAPHIC => {
                if let Some(g) = self.damage_graphic_mut() {
                    add_sprite(&mut g.sprite, v, world);
                }
            }
            a::DAMAGE_GRAPHICS_ENTRY_MOD => {
                let n = cvttss2si(f32::from(self.damage_graphic_entry.get()) + v);
                if let Some(n) = select_entry(self.damage_graphics.len(), n) {
                    self.damage_graphic_entry = DamageGraphicEntry::of_selected(n);
                }
            }
            // the game's: the selected entry plus the amount, not the count
            a::DAMAGE_GRAPHICS_TOTAL_NUM => {
                let count = f32::from(self.damage_graphic_entry.get()) + v;
                resize_damage_graphics(
                    &mut self.damage_graphics,
                    &mut self.damage_graphic_entry,
                    count,
                );
            }
            a::DAMAGE_GRAPHIC_PERCENT => {
                if let Some(g) = self.damage_graphic_mut() {
                    g.percent = g.percent.plus(low8(v));
                }
            }
            a::DAMAGE_GRAPHIC_APPLY_MODE => {
                if let Some(g) = self.damage_graphic_mut() {
                    g.apply_mode = g.apply_mode.plus(low8(v));
                }
            }
            _ => {}
        }
    }

    /// `FUN_1411271e0`, the static master's slot 0x28.
    pub(super) fn multiply_static(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::HIT_POINTS => {
                self.hit_points = MasterHitPoints::of_data(multiply_short(
                    self.hit_points.get(),
                    v,
                    world.capped,
                ));
            }
            a::LINE_OF_SIGHT => self.line_of_sight *= v,
            a::GARRISON_CAPACITY => {
                self.garrison =
                    GarrisonCapacity::of_data(low8(f32::from(self.garrison.get()) * v + ROUND));
            }
            a::SIZE_X => self.radius = self.radius.with_x(self.radius.x() * v),
            a::SIZE_Y => self.radius = self.radius.with_y(self.radius.y() * v),
            a::CARRY_CAPACITY => {
                self.capacity =
                    CarryCapacity::of_data(multiply_short(self.capacity.get(), v, world.capped));
            }
            a::STORAGE_1 | a::STORAGE_2 | a::STORAGE_3 => {
                if let Some(amount) = self.storage_mut(attribute) {
                    *amount = Amount::new(f32::from(low16(v * amount.get() + ROUND)));
                }
            }
            a::SIZE_Z => self.height *= v,
            // without the rounding
            a::BLAST_DEFENSE_LEVEL => {
                self.blast_defense =
                    BlastDefense::new(low8(f32::from(self.blast_defense.get()) * v));
            }
            a::AVAILABLE_UNIT_FLAG => {
                self.available = low8(f32::from(u8::from(self.available)) * v + ROUND) != 0;
            }
            a::DAMAGE_GRAPHICS_ENTRY_MOD => {
                insert_damage_graphic(&mut self.damage_graphics, &mut self.damage_graphic_entry, v);
            }
            a::DAMAGE_GRAPHICS_TOTAL_NUM => {
                let count = self.damage_graphics.len() as f32 * v;
                resize_damage_graphics(
                    &mut self.damage_graphics,
                    &mut self.damage_graphic_entry,
                    count,
                );
            }
            _ => {}
        }
    }

    /// The speed's cap (`FUN_1410a8b60`, `FUN_1410a8c00`): capped, a speed above it (`COMISS`,
    /// `JBE`: not a NaN) takes it, 20 for a missile master, else 4.5.
    pub(super) fn cap_speed(&mut self, capped: bool) {
        let cap = if self.kind == master_type::MISSILE {
            MISSILE_SPEED_CAP
        } else {
            SPEED_CAP
        };
        if capped && !at_most_or_unordered(self.max_speed.get(), cap) {
            self.max_speed = Speed::new(cap);
        }
    }

    /// `FUN_1410a8b00`, the animated master's slot 0x18.
    pub(super) fn set_animated(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        if attribute == attribute::MOVEMENT_SPEED {
            self.max_speed = Speed::new(v);
        } else {
            self.set_static(attribute, v, world);
        }
    }

    /// `FUN_1410a8b60`, the animated master's slot 0x20.
    pub(super) fn add_animated(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        if attribute == attribute::MOVEMENT_SPEED {
            self.max_speed += Speed::new(v);
            self.cap_speed(world.capped);
        } else {
            self.add_static(attribute, v, world);
        }
    }

    /// `FUN_1410a8c00`, the animated master's slot 0x28.
    pub(super) fn multiply_animated(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        if attribute == attribute::MOVEMENT_SPEED {
            self.max_speed *= v;
            self.cap_speed(world.capped);
        } else {
            self.multiply_static(attribute, v, world);
        }
    }

    /// `FUN_1410e1c00`, the moving master's slot 0x18.
    pub(super) fn set_moving(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::ROTATION_SPEED => self.turn_speed = Seconds::new(v),
            a::WALKING_GRAPHIC => set_sprite(&mut self.move_sprite, cvttss2si(v), world),
            a::RUNNING_GRAPHIC => set_sprite(&mut self.run_sprite, cvttss2si(v), world),
            a::TRAILING_UNIT => self.trail.unit = MasterField::of_data(low16(v)),
            a::TRAIL_MODE => self.trail.mode = TrailMode::of_data(low8(v)),
            a::TRAIL_DENSITY => self.trail.density = TrailDensity::new(v),
            _ => self.set_animated(attribute, v, world),
        }
    }

    /// `FUN_1410e1da0`, the moving master's slot 0x20.
    pub(super) fn add_moving(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::ROTATION_SPEED => self.turn_speed += Seconds::new(v),
            a::WALKING_GRAPHIC => add_sprite(&mut self.move_sprite, v, world),
            a::RUNNING_GRAPHIC => add_sprite(&mut self.run_sprite, v, world),
            a::TRAILING_UNIT => {
                self.trail.unit = self.trail.unit.plus(low16(v));
            }
            a::TRAIL_MODE => self.trail.mode = self.trail.mode.plus(low8(v)),
            a::TRAIL_DENSITY => self.trail.density += TrailDensity::new(v),
            _ => self.add_animated(attribute, v, world),
        }
    }

    /// `FUN_1410e1f60`, the moving master's slot 0x28.
    pub(super) fn multiply_moving(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match attribute {
            attribute::ROTATION_SPEED => self.turn_speed *= v,
            attribute::TRAIL_DENSITY => self.trail.density *= v,
            _ => self.multiply_animated(attribute, v, world),
        }
    }

    /// `FUN_14109fbb0`, the action master's slot 0x18.
    pub(super) fn set_action(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match attribute {
            attribute::WORK_RATE => self.work_rate = WorkRate::new(v),
            attribute::SEARCH_RADIUS => self.search_radius = Distance::new(v),
            attribute::RUN_PATTERN => self.run_pattern = RunPattern::of_data(low8(v)),
            // the command and move sounds (132 to 135) are the interface's
            attribute::COMMAND_SOUND
            | attribute::COMMAND_SOUND_EVENT
            | attribute::MOVE_SOUND
            | attribute::MOVE_SOUND_EVENT => {}
            _ => self.set_moving(attribute, v, world),
        }
    }

    /// `FUN_14109fdb0`, the action master's slot 0x20.
    pub(super) fn add_action(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match attribute {
            attribute::WORK_RATE => self.work_rate += WorkRate::new(v),
            attribute::SEARCH_RADIUS => self.search_radius += Distance::new(v),
            attribute::RUN_PATTERN => self.run_pattern = self.run_pattern.plus(low8(v)),
            // the command and move sounds (through slot 0x18) are the interface's
            attribute::COMMAND_SOUND | attribute::MOVE_SOUND => {}
            _ => self.add_moving(attribute, v, world),
        }
    }

    /// `FUN_14109fea0`, the action master's slot 0x28.
    pub(super) fn multiply_action(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match attribute {
            attribute::WORK_RATE => self.work_rate *= v,
            attribute::SEARCH_RADIUS => self.search_radius *= v,
            _ => self.multiply_moving(attribute, v, world),
        }
    }
}
