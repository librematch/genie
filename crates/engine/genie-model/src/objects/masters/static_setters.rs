//! The static, animated, moving and action masters' setters.

use crate::layer::generated::MastersTuning;
use crate::map::Distance;
use crate::map::RestrictionId;
use crate::math::fmath::at_most_or_unordered;
use crate::movement::Speed;
use crate::objects::MasterHitPoints;
use crate::objects::masters::AttributeWorld;
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
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MinimapMode;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::Occlusion;
use crate::objects::masters::ROUND;
use crate::objects::masters::SelectionEffect;
use crate::objects::masters::add_short;
use crate::objects::masters::add_sprite;
use crate::objects::masters::insert_damage_graphic;
use crate::objects::masters::low8;
use crate::objects::masters::low16;
use crate::objects::masters::multiply_short;
use crate::objects::masters::resize_damage_graphics;
use crate::objects::masters::select_entry;
use crate::objects::masters::set_sprite;
use crate::objects::masters::{Availability, DisableMode};
use crate::players::Amount;

impl Master {
    /// `FUN_141126400`, the static master's slot 0x18.
    pub(super) fn set_static(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si_64;
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::HIT_POINTS => self.hit_points = MasterHitPoints::of_data(low16(v)),
            MasterAttribute::LINE_OF_SIGHT => self.line_of_sight = Distance::new(v),
            MasterAttribute::GARRISON_CAPACITY => {
                self.garrison = GarrisonCapacity::of_data(low8(v));
            }
            MasterAttribute::SIZE_X => self.radius = self.radius.with_x(v),
            MasterAttribute::SIZE_Y => self.radius = self.radius.with_y(v),
            MasterAttribute::CARRY_CAPACITY => self.capacity = CarryCapacity::of_data(low16(v)),
            MasterAttribute::STORAGE_1
            | MasterAttribute::STORAGE_2
            | MasterAttribute::STORAGE_3 => {
                if let Some(amount) = self.storage_mut(attribute) {
                    *amount = Amount::new(f32::from(low16(v)));
                }
            }
            MasterAttribute::FOG_VISIBILITY => {
                self.fog_visibility = FogVisibility::of_data(low8(v));
            }
            MasterAttribute::OCCLUSION_MODE => self.occlusion = Occlusion::of_data(low8(v)),
            MasterAttribute::SIZE_Z => self.height = Distance::new(v),
            MasterAttribute::CAN_BE_BUILT_ON | MasterAttribute::BLAST_DEFENSE_LEVEL => {
                self.blast_defense = BlastDefense::new(low8(v));
            }
            MasterAttribute::TERRAIN_RESTRICTION_ID => {
                self.terrain_restriction = RestrictionId::of_data(low16(v));
            }
            // `CVTTSS2SI RAX`, the low 32 bits
            MasterAttribute::UNIT_TRAIT => {
                self.object_flags = ObjectFlags::from_bits_retain(cvttss2si_64(v) as u32);
            }
            MasterAttribute::TRAIT_PIECE => {
                self.object_flags = ObjectFlags::from_bits_retain(
                    ((cvttss2si_64(v) as u32) << 16) | (self.object_flags.bits() & 0xffff),
                );
            }
            MasterAttribute::DEAD_UNIT_ID => self.death_spawns[0] = MasterId::of_data(low16(v)),
            MasterAttribute::BLOOD_UNIT => self.death_spawns[1] = MasterId::of_data(low16(v)),
            MasterAttribute::STANDING_GRAPHIC
            | MasterAttribute::STANDING_GRAPHIC_2
            | MasterAttribute::DYING_GRAPHIC
            | MasterAttribute::UNDEAD_GRAPHIC => {
                self.set_static_sprite(attribute, v, world);
            }
            MasterAttribute::OBSTRUCTION_KIND => {
                self.obstruction_kind = ObstructionKind::set(low8(v));
            }
            MasterAttribute::BLOCKAGE_CLASS => {
                self.obstruction_class = ObstructionClass::from(low8(v));
            }
            MasterAttribute::SELECTION_EFFECT => {
                self.selection_effect = SelectionEffect::of_data(low8(v));
            }
            MasterAttribute::DAMAGE_GRAPHIC
            | MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD
            | MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM
            | MasterAttribute::DAMAGE_GRAPHIC_PERCENT
            | MasterAttribute::DAMAGE_GRAPHIC_APPLY_MODE => {
                self.set_damage_graphic(attribute, v, world);
            }
            MasterAttribute::AVAILABLE_UNIT_FLAG
            | MasterAttribute::DISABLED_UNIT_FLAG
            | MasterAttribute::INTERFACE_KIND
            | MasterAttribute::COMBAT_LEVEL
            | MasterAttribute::INTERACTION_MODE
            | MasterAttribute::MINIMAP_MODE => self.set_static_level(attribute, v),
            // the icon (25), the name and description (50, 51) and the sounds (92 to 99) are the
            // interface's
            _ => {}
        }
    }

    /// [`Master::set_static`]'s flags and levels (`FUN_141126400`): available, disabled, the
    /// interface kind, the combat level, the interaction and minimap modes.
    pub(super) fn set_static_level(&mut self, attribute: MasterAttribute, v: f32) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            // the save's byte, which the slice holds as its test against 0
            MasterAttribute::AVAILABLE_UNIT_FLAG => self.available = Availability::of_data(low8(v)),
            MasterAttribute::DISABLED_UNIT_FLAG => self.disabled = DisableMode::of_data(low8(v)),
            MasterAttribute::INTERFACE_KIND => self.unit_level = InterfaceKind::of_data(low8(v)),
            MasterAttribute::COMBAT_LEVEL => self.combat_level = CombatLevel::of_data(low8(v)),
            MasterAttribute::INTERACTION_MODE => {
                self.select_level = InteractionMode::of_data(low8(v));
            }
            MasterAttribute::MINIMAP_MODE => self.map_draw_level = MinimapMode::of_data(low8(v)),
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::STANDING_GRAPHIC => {
                set_sprite(&mut self.standing_sprite, cvttss2si(v), world);
            }
            MasterAttribute::STANDING_GRAPHIC_2 => {
                set_sprite(&mut self.standing_sprite_2, cvttss2si(v), world);
            }
            MasterAttribute::DYING_GRAPHIC => {
                set_sprite(&mut self.death_sprite, cvttss2si(v), world);
            }
            MasterAttribute::UNDEAD_GRAPHIC => {
                set_sprite(&mut self.undead_sprite, cvttss2si(v), world);
            }
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::DAMAGE_GRAPHIC => {
                if let Some(g) = self.damage_graphic_mut() {
                    set_sprite(&mut g.sprite, cvttss2si(v), world);
                }
            }
            MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD => {
                if let Some(n) = select_entry(self.damage_graphics.len(), cvttss2si(v)) {
                    self.damage_graphic_entry = DamageGraphicEntry::of_selected(n);
                }
            }
            MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM => {
                resize_damage_graphics(
                    &mut self.damage_graphics,
                    &mut self.damage_graphic_entry,
                    v,
                );
            }
            MasterAttribute::DAMAGE_GRAPHIC_PERCENT => {
                if let Some(g) = self.damage_graphic_mut() {
                    g.percent = HpPercent::of_data(low8(v));
                }
            }
            MasterAttribute::DAMAGE_GRAPHIC_APPLY_MODE => {
                if let Some(damage_graphic) = self.damage_graphic_mut() {
                    damage_graphic.apply_mode = DamageSpriteMode::of_data(low8(v));
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::HIT_POINTS => {
                self.hit_points =
                    MasterHitPoints::of_data(add_short(self.hit_points.get(), v, world.capped));
            }
            MasterAttribute::LINE_OF_SIGHT => self.line_of_sight += Distance::new(v),
            MasterAttribute::GARRISON_CAPACITY => self.garrison = self.garrison.plus(low8(v)),
            MasterAttribute::SIZE_X => self.radius = self.radius.with_x(self.radius.x() + v),
            MasterAttribute::SIZE_Y => self.radius = self.radius.with_y(self.radius.y() + v),
            MasterAttribute::CARRY_CAPACITY => {
                self.capacity =
                    CarryCapacity::of_data(add_short(self.capacity.get(), v, world.capped));
            }
            MasterAttribute::STORAGE_1
            | MasterAttribute::STORAGE_2
            | MasterAttribute::STORAGE_3 => {
                if let Some(amount) = self.storage_mut(attribute) {
                    *amount += Amount::new(f32::from(low16(v)));
                }
            }
            MasterAttribute::OCCLUSION_MODE => self.occlusion = self.occlusion.plus(low8(v)),
            MasterAttribute::SIZE_Z => self.height += Distance::new(v),
            MasterAttribute::CAN_BE_BUILT_ON | MasterAttribute::BLAST_DEFENSE_LEVEL => {
                self.blast_defense =
                    BlastDefense::new(self.blast_defense.get().wrapping_add(low8(v)));
            }
            MasterAttribute::UNIT_TRAIT => {
                self.object_flags = ObjectFlags::from_bits_retain(
                    self.object_flags
                        .bits()
                        .wrapping_add(cvttss2si_64(v) as u32),
                );
            }
            MasterAttribute::STANDING_GRAPHIC
            | MasterAttribute::STANDING_GRAPHIC_2
            | MasterAttribute::DYING_GRAPHIC
            | MasterAttribute::UNDEAD_GRAPHIC => {
                self.add_static_sprite(attribute, v, world);
            }
            MasterAttribute::DAMAGE_GRAPHIC
            | MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD
            | MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM
            | MasterAttribute::DAMAGE_GRAPHIC_PERCENT
            | MasterAttribute::DAMAGE_GRAPHIC_APPLY_MODE => {
                self.add_damage_graphic(attribute, v, world);
            }
            MasterAttribute::AVAILABLE_UNIT_FLAG => {
                self.available = self.available.plus(low8(v));
            }
            MasterAttribute::DISABLED_UNIT_FLAG => {
                self.disabled = self.disabled.plus(low8(v));
            }
            MasterAttribute::INTERFACE_KIND => self.unit_level = self.unit_level.plus(low8(v)),
            MasterAttribute::COMBAT_LEVEL => self.combat_level = self.combat_level.plus(low8(v)),
            MasterAttribute::INTERACTION_MODE => {
                self.select_level = self.select_level.plus(low8(v));
            }
            MasterAttribute::MINIMAP_MODE => {
                self.map_draw_level = self.map_draw_level.plus(low8(v));
            }
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::STANDING_GRAPHIC => add_sprite(&mut self.standing_sprite, v, world),
            MasterAttribute::STANDING_GRAPHIC_2 => {
                add_sprite(&mut self.standing_sprite_2, v, world);
            }
            MasterAttribute::DYING_GRAPHIC => add_sprite(&mut self.death_sprite, v, world),
            MasterAttribute::UNDEAD_GRAPHIC => add_sprite(&mut self.undead_sprite, v, world),
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::DAMAGE_GRAPHIC => {
                if let Some(g) = self.damage_graphic_mut() {
                    add_sprite(&mut g.sprite, v, world);
                }
            }
            MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD => {
                let entry = cvttss2si(f32::from(self.damage_graphic_entry.get()) + v);
                if let Some(n) = select_entry(self.damage_graphics.len(), entry) {
                    self.damage_graphic_entry = DamageGraphicEntry::of_selected(n);
                }
            }
            // the game's: the selected entry plus the amount, not the count
            MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM => {
                let count = f32::from(self.damage_graphic_entry.get()) + v;
                resize_damage_graphics(
                    &mut self.damage_graphics,
                    &mut self.damage_graphic_entry,
                    count,
                );
            }
            MasterAttribute::DAMAGE_GRAPHIC_PERCENT => {
                if let Some(g) = self.damage_graphic_mut() {
                    g.percent = g.percent.plus(low8(v));
                }
            }
            MasterAttribute::DAMAGE_GRAPHIC_APPLY_MODE => {
                if let Some(damage_graphic) = self.damage_graphic_mut() {
                    damage_graphic.apply_mode = damage_graphic.apply_mode.plus(low8(v));
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::HIT_POINTS => {
                self.hit_points = MasterHitPoints::of_data(multiply_short(
                    self.hit_points.get(),
                    v,
                    world.capped,
                ));
            }
            MasterAttribute::LINE_OF_SIGHT => self.line_of_sight *= v,
            MasterAttribute::GARRISON_CAPACITY => {
                self.garrison =
                    GarrisonCapacity::of_data(low8(f32::from(self.garrison.get()) * v + ROUND));
            }
            MasterAttribute::SIZE_X => self.radius = self.radius.with_x(self.radius.x() * v),
            MasterAttribute::SIZE_Y => self.radius = self.radius.with_y(self.radius.y() * v),
            MasterAttribute::CARRY_CAPACITY => {
                self.capacity =
                    CarryCapacity::of_data(multiply_short(self.capacity.get(), v, world.capped));
            }
            MasterAttribute::STORAGE_1
            | MasterAttribute::STORAGE_2
            | MasterAttribute::STORAGE_3 => {
                if let Some(amount) = self.storage_mut(attribute) {
                    *amount = Amount::new(f32::from(low16(v * amount.get() + ROUND)));
                }
            }
            MasterAttribute::SIZE_Z => self.height *= v,
            // without the rounding
            MasterAttribute::BLAST_DEFENSE_LEVEL => {
                self.blast_defense =
                    BlastDefense::new(low8(f32::from(self.blast_defense.get()) * v));
            }
            MasterAttribute::AVAILABLE_UNIT_FLAG => {
                self.available =
                    Availability::of_data(low8(f32::from(self.available.get()) * v + ROUND));
            }
            MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD => {
                insert_damage_graphic(&mut self.damage_graphics, &mut self.damage_graphic_entry, v);
            }
            MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM => {
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
    /// `JBE`: not a NaN) takes it, `MastersTuning::missile_speed_cap` for a missile master, else
    /// `MastersTuning::speed_cap`.
    pub(super) fn cap_speed(&mut self, capped: bool, masters_tuning: &MastersTuning) {
        let cap = if self.kind == MasterKind::MISSILE {
            masters_tuning.missile_speed_cap
        } else {
            masters_tuning.speed_cap
        };
        if let Some(animated_part) = self.animated.as_mut()
            && capped
            && !at_most_or_unordered(animated_part.max_speed.get(), cap)
        {
            animated_part.max_speed = Speed::new(cap);
        }
    }

    /// `FUN_1410a8b00`, the animated master's slot 0x18.
    pub(super) fn set_animated(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        if attribute == MasterAttribute::MOVEMENT_SPEED {
            if let Some(animated_part) = self.animated.as_mut() {
                animated_part.max_speed = Speed::new(v);
            }
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
        if attribute == MasterAttribute::MOVEMENT_SPEED {
            if let Some(animated_part) = self.animated.as_mut() {
                animated_part.max_speed += Speed::new(v);
            }
            self.cap_speed(world.capped, world.masters_tuning);
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
        if attribute == MasterAttribute::MOVEMENT_SPEED {
            if let Some(animated_part) = self.animated.as_mut() {
                animated_part.max_speed *= v;
            }
            self.cap_speed(world.capped, world.masters_tuning);
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ROTATION_SPEED
            | MasterAttribute::WALKING_GRAPHIC
            | MasterAttribute::RUNNING_GRAPHIC
            | MasterAttribute::TRAILING_UNIT
            | MasterAttribute::TRAIL_MODE
            | MasterAttribute::TRAIL_DENSITY => {
                if let Some(moving_part) = self.moving.as_mut() {
                    moving_part.set_value(attribute, v, world);
                }
            }
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ROTATION_SPEED
            | MasterAttribute::WALKING_GRAPHIC
            | MasterAttribute::RUNNING_GRAPHIC
            | MasterAttribute::TRAILING_UNIT
            | MasterAttribute::TRAIL_MODE
            | MasterAttribute::TRAIL_DENSITY => {
                if let Some(moving_part) = self.moving.as_mut() {
                    moving_part.add_value(attribute, v, world);
                }
            }
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
            MasterAttribute::ROTATION_SPEED | MasterAttribute::TRAIL_DENSITY => {
                if let Some(moving_part) = self.moving.as_mut() {
                    moving_part.multiply_value(attribute, v);
                }
            }
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
            MasterAttribute::WORK_RATE
            | MasterAttribute::SEARCH_RADIUS
            | MasterAttribute::RUN_PATTERN => {
                if let Some(action_part) = self.action.as_mut() {
                    action_part.set_value(attribute, v);
                }
            }
            // the command and move sounds (132 to 135) are the interface's
            MasterAttribute::COMMAND_SOUND
            | MasterAttribute::COMMAND_SOUND_EVENT
            | MasterAttribute::MOVE_SOUND
            | MasterAttribute::MOVE_SOUND_EVENT => {}
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
            MasterAttribute::WORK_RATE
            | MasterAttribute::SEARCH_RADIUS
            | MasterAttribute::RUN_PATTERN => {
                if let Some(action_part) = self.action.as_mut() {
                    action_part.add_value(attribute, v);
                }
            }
            // the command and move sounds (through slot 0x18) are the interface's
            MasterAttribute::COMMAND_SOUND | MasterAttribute::MOVE_SOUND => {}
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
            MasterAttribute::WORK_RATE | MasterAttribute::SEARCH_RADIUS => {
                if let Some(action_part) = self.action.as_mut() {
                    action_part.multiply_value(attribute, v);
                }
            }
            _ => self.multiply_moving(attribute, v, world),
        }
    }
}
