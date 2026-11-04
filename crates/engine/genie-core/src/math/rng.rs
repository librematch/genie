//! The engine's random number generators: a `WELL512a` variant in eleven streams, all reseeded at
//! the top of every tick from one frame seed.
//!
//! 158041: setSeed `0x140811ae0`, the step `0x140811070`, `rand15` (`debug_rand`) `0x1408117b0`,
//! `SeedAll` `0x140efe4f0`. The registry order is the order of the static constructors
//! (`0x142e65cf8`); every constructor (`0x140810840`) appends its generator to it.

use crate::math::image::rng::{FLOAT_MAX, FLOAT_SCALE};
use crate::trace::draw;
use std::array::from_fn;

/// One generator. Only what the simulation reads: the 16-word state and its index, the draw count
/// and the last `rand15` value (`currand`, which a save records).
// not `Copy`: a stream's draws must advance the stream, never a silent copy of it
#[allow(missing_copy_implementations)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Well512 {
    /// The 16 words of the ring, turned so that the word at the game's index (`+0x64`) comes
    /// first: the step's taps sit at fixed places, and a step turns the ring by one.
    state: [u32; 16],
    /// Draws since construction; `set_seed` leaves it alone.
    pub(crate) draws: u32,
    /// The last `rand15` value; the constructor sets it to `u32::MAX`.
    pub(crate) last: u32,
    /// Which of the eleven streams it is ([`Stream`] as its number, [`NO_STREAM`] for a
    /// generator outside [`Streams`]): the draw trace's only ([`crate::trace::DRAWS`]).
    pub(crate) stream: u8,
}

/// [`Well512::stream`] of a generator outside [`Streams`].
pub const NO_STREAM: u8 = 255;

impl Default for Well512 {
    fn default() -> Self {
        Self {
            state: [0; 16],
            draws: 0,
            last: u32::MAX,
            stream: NO_STREAM,
        }
    }
}

impl Well512 {
    /// Fill the state from a seed: a byte-swapped xorshift32 mixed with Bob Jenkins' 32-bit
    /// integer hash, chained through the seed. The index restarts at 0.
    pub fn set_seed(&mut self, seed: u32) {
        let mut shift = seed.swap_bytes() ^ 0xaac2_9377;
        let mut hash = seed;
        for word in &mut self.state {
            shift ^= shift << 17;
            shift ^= shift >> 13;
            shift ^= shift << 5;
            hash = jenkins(hash);
            *word = shift ^ hash;
        }
    }

    /// One step. The engine's variant of `WELL512a`: taps at index − 3 and index − 7, the mask
    /// `0xfed22169`, and an extra `^ z1` in the new word. Each step is traced
    /// ([`crate::trace::DRAWS`]) with the engine's line that asked for it.
    #[track_caller]
    pub fn raw(&mut self) -> u32 {
        self.draws = self.draws.wrapping_add(1);
        // the ring from the index on: index − 3 is its 14th word, index − 7 its 10th, and
        // index − 1, the next index, its last
        let state = &mut self.state;
        let v0 = state[0];
        let word13 = state[13];
        let z1 = (((v0 << 1) ^ word13) << 15) ^ word13 ^ v0;
        let word9 = state[9];
        let z2 = word9 ^ (word9 >> 11);
        let new1 = z1 ^ z2;
        state[0] = new1;
        let z0 = state[15];
        state[15] =
            z0 ^ ((((z2 << 10) ^ z1) << 16 ^ z0) << 2) ^ ((new1 & 0xfed2_2169) << 5) ^ new1 ^ z1;
        // the next index's word comes first
        state.rotate_right(1);
        draw(self.stream, self.draws, state[0]);
        state[0]
    }

    /// `rand15`: a step masked to 15 bits, kept as `last`.
    /// A 64-bit draw (`FUN_140811100`): two steps, the first one's word high and the second
    /// one's low, counted as one draw.
    #[track_caller]
    pub fn raw64(&mut self) -> u64 {
        let hi = self.raw();
        let lo = self.raw();
        self.draws = self.draws.wrapping_sub(1);
        (u64::from(hi) << 32) | u64::from(lo)
    }

    /// The next draw, 15 bits (`rand15`).
    #[track_caller]
    pub fn rand15(&mut self) -> u32 {
        self.last = self.raw() & 0x7fff;
        self.last
    }

    /// `GetRandFloat0to1` (`FUN_1408115a0`): a step times `2^-32` (`DAT_142ea7c48`, in f64), as
    /// f32, held to 0..1; `last` stays.
    #[track_caller]
    pub fn float01(&mut self) -> f32 {
        /// The step's scale (`DAT_142ea7c48`).
        const SCALE: f64 = FLOAT_SCALE;
        /// The top (`DAT_14330eb8c`).
        const MAX: f32 = FLOAT_MAX;
        let value = (f64::from(self.raw()) * SCALE) as f32;
        if (0.0..=MAX).contains(&value) {
            value
        } else if 0.0 <= value {
            MAX
        } else {
            0.0
        }
    }

    /// A step scaled to `0..bound` (`FUN_140810cb0`): `step × bound >> 32`; `last` stays.
    #[track_caller]
    pub fn below(&mut self, bound: u32) -> u32 {
        ((u64::from(self.raw()) * u64::from(bound)) >> 32) as u32
    }

    /// A step scaled to `0..bound` that takes no step for `bound` 0 (`FUN_140810fc0`): the draw
    /// counts either way, then 0 for `bound` 0, else [`Well512::below`]'s `step × bound >> 32`;
    /// `last` stays.
    #[track_caller]
    pub fn below_checked(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            self.draws = self.draws.wrapping_add(1);
            return 0;
        }
        self.below(bound)
    }

    /// An index in `0..bound` as the C++ library's `std::shuffle` draws it with the generator as
    /// its engine (MSVC `_Rng_from_urng`, 32 bits a step): a step, again while it falls in the last
    /// incomplete span of `bound` (`step / bound` is not below `u32::MAX / n`, unless `u32::MAX %
    /// n` is `bound − 1`), then `step % bound`.
    #[track_caller]
    pub fn index(&mut self, bound: u32) -> u32 {
        loop {
            let raw = self.raw();
            if raw / bound < u32::MAX / bound || u32::MAX % bound == bound - 1 {
                return raw % bound;
            }
        }
    }

    /// `std::shuffle` with the generator as its engine: each place from the second on swaps
    /// with an [`Well512::index`] up to it.
    #[track_caller]
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for slot in 1..items.len() {
            let index = self.index(slot as u32 + 1) as usize;
            if index != slot {
                items.swap(slot, index);
            }
        }
    }
}

/// Bob Jenkins' 32-bit integer hash, as the compiler folded it in setSeed.
const fn jenkins(key: u32) -> u32 {
    let hash = key.wrapping_mul(0x1001).wrapping_add(0x7ed5_5d16);
    let hash = ((hash >> 19) ^ hash ^ 0xc761_c23c).wrapping_mul(0x21);
    let hash = (hash.wrapping_add(0xe9f8_cc1d)
        ^ hash.wrapping_add(0x1656_67b1).wrapping_mul(0x200))
    .wrapping_mul(9)
    .wrapping_add(0xfd70_46c5);
    (hash >> 16) ^ hash ^ 0xb55a_4f09
}

/// The streams in registry order. The names come from the constructors' strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stream {
    /// The Action stream.
    Action,
    /// The random map's stream.
    RandomMap,
    /// The sprites' stream.
    Sprite,
    /// The world's stream.
    World,
    /// The AI stream.
    Ai,
    /// The objects' stream.
    Object,
    /// The players' stream.
    Player,
    /// The editor's stream.
    Editor,
    /// The stream outside the sync.
    Unsynced,
    /// The communication's stream.
    Comm,
    /// The interface's stream.
    Ui,
}

/// All eleven generators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Streams {
    rngs: [Well512; 11],
}

impl Default for Streams {
    fn default() -> Self {
        Self {
            rngs: from_fn(|stream| Well512 {
                stream: stream as u8,
                ..Well512::default()
            }),
        }
    }
}

impl Streams {
    /// `SeedAll`: seed each generator in registry order, then take its first raw draw as the
    /// next generator's seed.
    pub fn seed_all(&mut self, seed: u32) {
        let mut seed = seed;
        for rng in &mut self.rngs {
            rng.set_seed(seed);
            seed = rng.raw();
        }
    }

    /// `CommHandler::setLastWorldRandom` (`0x140d3ce20`, "setLastRandoms"), the end of every
    /// world update after the next frame seed: a `rand15` of the AI, Action, Object and Player
    /// streams in that order, which the multiplayer sync sends with the frame seed. The next
    /// update's `SeedAll` reseeds them; their `last` values and draw counts stay.
    #[track_caller]
    pub fn set_last_randoms(&mut self) -> [u32; 4] {
        // no closure: the draw trace names the caller's line
        let ai = self.get_mut(Stream::Ai).rand15();
        let action = self.get_mut(Stream::Action).rand15();
        let object = self.get_mut(Stream::Object).rand15();
        let player = self.get_mut(Stream::Player).rand15();
        [ai, action, object, player]
    }

    /// The stream `stream`.
    pub const fn get_mut(&mut self, stream: Stream) -> &mut Well512 {
        let [
            action,
            random_map,
            sprite,
            world,
            ai,
            object,
            player,
            editor,
            unsynced,
            comm,
            ui,
        ] = &mut self.rngs;
        match stream {
            Stream::Action => action,
            Stream::RandomMap => random_map,
            Stream::Sprite => sprite,
            Stream::World => world,
            Stream::Ai => ai,
            Stream::Object => object,
            Stream::Player => player,
            Stream::Editor => editor,
            Stream::Unsynced => unsynced,
            Stream::Comm => comm,
            Stream::Ui => ui,
        }
    }

    /// The Object and the Sprite streams at once (an attacker's update draws from both).
    pub const fn object_and_sprite(&mut self) -> (&mut Well512, &mut Well512) {
        let [_, _, sprite, _, _, object, _, _, _, _, _] = &mut self.rngs;
        (object, sprite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float01_is_the_game_get_rand_float_0_to_1() {
        // FUN_1408115a0 run in unicorn on this state (words at +0x24, index at +0x64 = 5)
        let mut state: [u32; 16] = from_fn(|index| 0x9e37_79b9_u32.wrapping_mul(index as u32 + 1));
        state.rotate_left(5);
        let mut rng = Well512 {
            state,
            ..Well512::default()
        };
        let game = [
            0x3e2f_e1b9,
            0x3e66_58f1,
            0x3eea_6794,
            0x3ea8_b3e0,
            0x3f3d_c906,
            0x3eaf_bdab,
            0x3f68_7e1e,
            0x3f3b_f800,
        ];
        let ours: Vec<u32> = (0..8).map(|_| rng.float01().to_bits()).collect();
        assert_eq!(ours, game);
        assert_eq!((rng.draws, rng.last), (8, u32::MAX));
    }

    // Expected values from an independent Python transcription of the same decompiled functions.

    /// setLastRandoms draws once from the AI, Action, Object and Player streams, in that order,
    /// and leaves the others alone.
    #[test]
    fn set_last_randoms_draws_four_streams_once() {
        let mut streams = Streams::default();
        streams.seed_all(1062);
        let mut copy = streams.clone();
        let values = streams.set_last_randoms();
        let expected = [Stream::Ai, Stream::Action, Stream::Object, Stream::Player]
            .map(|draw| copy.get_mut(draw).rand15());
        assert_eq!(values, expected);
        assert_eq!(streams, copy);
        assert_eq!(streams.get_mut(Stream::Ai).last, values[0]);
    }

    #[test]
    fn set_seed_fills_the_state_and_restarts_the_index() {
        let mut rng = Well512::default();
        rng.set_seed(29910);
        assert_eq!(
            &rng.state[..4],
            &[0xd80f_fb63, 0x00fd_e7a2, 0x5fd2_5128, 0x2699_7df5]
        );
    }

    #[test]
    fn raw_draws_follow_the_reference() {
        let mut rng = Well512::default();
        rng.set_seed(29910);
        let got: Vec<u32> = (0..5).map(|_| rng.raw()).collect();
        assert_eq!(
            got,
            [
                3_025_362_954,
                1_149_836_362,
                891_056_692,
                54_472_551,
                1_361_767_319
            ]
        );
        assert_eq!(rng.draws, 5);
    }

    #[test]
    fn rand15_masks_to_15_bits_and_keeps_the_last_value() {
        let mut rng = Well512::default();
        rng.set_seed(0);
        let got: Vec<u32> = (0..5).map(|_| rng.rand15()).collect();
        assert_eq!(got, [7241, 25269, 9993, 9310, 16801]);
        assert_eq!(rng.last, 16801);
    }

    /// The ring turned to its index draws as the game's 16 words with their index (`+0x64`,
    /// the step as the game runs it), over two and a half turns.
    #[test]
    fn the_turned_ring_draws_as_the_game_indexed_ring() {
        let words: [u32; 16] = from_fn(|index| 0x9e37_79b9_u32.wrapping_mul(index as u32 + 1));
        let (mut state, mut index) = (words, 5usize);
        let game: Vec<u32> = (0..40)
            .map(|_| {
                let v0 = state[index];
                let word13 = state[index.wrapping_sub(3) & 15];
                let z1 = (((v0 << 1) ^ word13) << 15) ^ word13 ^ v0;
                let word9 = state[index.wrapping_sub(7) & 15];
                let z2 = word9 ^ (word9 >> 11);
                let new1 = z1 ^ z2;
                state[index] = new1;
                let prior = index.wrapping_sub(1) & 15;
                let z0 = state[prior];
                state[prior] = z0
                    ^ ((((z2 << 10) ^ z1) << 16 ^ z0) << 2)
                    ^ ((new1 & 0xfed2_2169) << 5)
                    ^ new1
                    ^ z1;
                index = prior;
                state[prior]
            })
            .collect();
        let mut state = words;
        state.rotate_left(5);
        let mut rng = Well512 {
            state,
            ..Well512::default()
        };
        let ours: Vec<u32> = (0..40).map(|_| rng.raw()).collect();
        assert_eq!(ours, game);
    }

    #[test]
    fn twenty_draws_wrap_the_index_twice_over() {
        let mut rng = Well512::default();
        rng.set_seed(0xdead_be3f);
        let got: Vec<u32> = (0..20).map(|_| rng.raw()).collect();
        assert_eq!(&got[17..], &[3_247_376_368, 3_960_506_532, 2_739_179_492]);
    }

    #[test]
    fn seed_all_chains_the_seeds_in_registry_order() {
        let mut streams = Streams::default();
        streams.seed_all(29910);
        let world: Vec<u32> = (0..3)
            .map(|_| streams.get_mut(Stream::World).rand15())
            .collect();
        let ai: Vec<u32> = (0..3)
            .map(|_| streams.get_mut(Stream::Ai).rand15())
            .collect();
        assert_eq!(world, [19550, 2220, 13198]);
        assert_eq!(ai, [16591, 22366, 24306]);
    }

    #[test]
    fn draw_below_bound_scales_the_step_by_the_bound() {
        // FUN_140810cb0: step × n >> 32
        let (mut first, mut second) = (Well512::default(), Well512::default());
        first.set_seed(0);
        second.set_seed(0);
        let step = second.raw();
        assert_eq!(first.below(360), ((u64::from(step) * 360) >> 32) as u32);
        assert_eq!(first.below(0), 0);
        assert_eq!(first.draws, 2);
    }

    #[test]
    fn checked_draw_below_zero_counts_the_draw_but_takes_no_step() {
        // FUN_140810fc0: the draw count +1, then 0 for n 0 with the state left alone; any other
        // n is FUN_140810cb0's draw
        let (mut first, mut second) = (Well512::default(), Well512::default());
        first.set_seed(7);
        second.set_seed(7);
        assert_eq!(first.below_checked(0), 0);
        assert_eq!(first.draws, 1);
        assert_eq!(first.state, second.state);
        assert_eq!(first.below_checked(360), second.below(360));
        assert_eq!((first.draws, second.draws), (2, 1));
        assert_eq!(first.state, second.state);
    }

    #[test]
    fn draw_of_64_bits_is_two_steps_counted_once() {
        // FUN_140811100: the first step's word high, the second's low, the draw count +1
        let (mut first, mut second) = (Well512::default(), Well512::default());
        first.set_seed(7);
        second.set_seed(7);
        let wide = first.raw64();
        let (hi, lo) = (second.raw(), second.raw());
        assert_eq!(wide, (u64::from(hi) << 32) | u64::from(lo));
        assert_eq!((first.draws, second.draws), (1, 2));
    }
}
