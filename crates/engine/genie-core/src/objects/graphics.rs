//! The world's sprite table as one type: each sprite's graphic by its id, a dense column.

use crate::objects::SpriteId;
use crate::objects::sprite::Graphic;

/// What the sprite clock reads of each graphic (the `.dat`'s), by sprite id: a dense column
/// by the id (the `.dat`'s ids are 0 and up), walked in the ids' order. Two tables are equal
/// when they hold the same sprites and graphics, as two maps are.
#[derive(Clone, Debug, Default)]
pub struct Graphics {
    /// The graphics by sprite id.
    by_id: Vec<Option<Graphic>>,
    /// How many it holds.
    len: usize,
}

/// The column's index of `sprite`: its id's 16 bits read unsigned, so that every id has one
/// (no sprite's id is below 0, [`SpriteId::named`]).
fn index(sprite: SpriteId) -> usize {
    usize::from(sprite.get().cast_unsigned())
}

impl Graphics {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            by_id: Vec::new(),
            len: 0,
        }
    }

    /// How many graphics it holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether it holds none (a world without the `.dat`'s sprites).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The graphic of `sprite`.
    #[must_use]
    pub fn get(&self, sprite: SpriteId) -> Option<&Graphic> {
        self.by_id.get(index(sprite))?.as_ref()
    }

    /// Whether it holds a graphic of `sprite`.
    #[must_use]
    pub fn contains_key(&self, sprite: SpriteId) -> bool {
        self.get(sprite).is_some()
    }

    /// `graphic` for `sprite`; the graphic it held before.
    pub fn insert(&mut self, sprite: SpriteId, graphic: Graphic) -> Option<Graphic> {
        let i = index(sprite);
        if self.by_id.len() <= i {
            self.by_id.resize(i + 1, None);
        }
        let old = self.by_id.get_mut(i).and_then(|g| g.replace(graphic));
        if old.is_none() {
            self.len += 1;
        }
        old
    }

    /// Every graphic taken out.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Every sprite and its graphic, the lowest id first.
    #[must_use]
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            inner: self.by_id.iter().enumerate(),
        }
    }
}

/// [`Graphics::iter`]'s walk.
#[derive(Clone, Debug)]
pub struct Iter<'a> {
    /// The column with its indices.
    inner: std::iter::Enumerate<std::slice::Iter<'a, Option<Graphic>>>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (SpriteId, &'a Graphic);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.find_map(|(i, g)| {
            let id = u16::try_from(i).ok()?.cast_signed();
            Some((SpriteId::named(id), g.as_ref()?))
        })
    }
}

impl<'a> IntoIterator for &'a Graphics {
    type Item = (SpriteId, &'a Graphic);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl PartialEq for Graphics {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

impl<const N: usize> From<[(SpriteId, Graphic); N]> for Graphics {
    fn from(graphics: [(SpriteId, Graphic); N]) -> Self {
        graphics.into_iter().collect()
    }
}

impl FromIterator<(SpriteId, Graphic)> for Graphics {
    fn from_iter<I: IntoIterator<Item = (SpriteId, Graphic)>>(iter: I) -> Self {
        let mut table = Self::new();
        for (sprite, graphic) in iter {
            let _ = table.insert(sprite, graphic);
        }
        table
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::graphics::Graphics;
    use crate::objects::sprite::Graphic;
    use crate::objects::{FacetCount, Frame, SpeedMultiplier, SpriteFlags, SpriteId};
    use crate::world::Seconds;

    /// The graphic of sprite `id` with `frames` frames.
    const fn graphic(id: i16, frames: i16) -> Graphic {
        Graphic {
            id: SpriteId::named(id),
            frames: Frame::of_data(frames),
            facets: FacetCount::of_data(16),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        }
    }

    /// The table finds each sprite's graphic, replaces one in place and walks the sprites in
    /// their ids' order; two tables with the same graphics are equal.
    #[test]
    fn the_table_finds_and_walks_the_sprites_by_id() {
        let mut g: Graphics = [
            (SpriteId::named(9), graphic(9, 1)),
            (SpriteId::named(2), graphic(2, 2)),
        ]
        .into_iter()
        .collect();
        assert_eq!(g.len(), 2);
        assert_eq!(g.get(SpriteId::named(9)), Some(&graphic(9, 1)));
        assert_eq!(g.get(SpriteId::named(3)), None);
        assert!(!g.contains_key(SpriteId::named(4000)));
        assert_eq!(
            g.insert(SpriteId::named(9), graphic(9, 5)),
            Some(graphic(9, 1))
        );
        let walked: Vec<i16> = g.iter().map(|(s, _)| s.get()).collect();
        assert_eq!(walked, [2, 9]);
        let same: Graphics = [
            (SpriteId::named(2), graphic(2, 2)),
            (SpriteId::named(9), graphic(9, 5)),
        ]
        .into_iter()
        .collect();
        assert_eq!(g, same);
        assert!(Graphics::new().is_empty());
        let mut from = Graphics::from([(SpriteId::named(2), graphic(2, 2))]);
        assert_eq!(from.len(), 1);
        from.clear();
        assert!(from.is_empty());
        assert_eq!((&g).into_iter().count(), 2);
    }
}
