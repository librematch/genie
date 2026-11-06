//! The command packets of op 1, decoded per command byte.
//!
//! The layouts are generated (`generated.rs`, from `aoe2de-exe-re/command_decoders` in the
//! workspace) from what build 158041 itself says about each packet:
//!
//! - its command registry names the byte (`TRIBE_COMMAND_BUILD` is `0x66`) and its class;
//! - the class's formatter, which prints a packet for the game's logs (`Build(num=%d, x=%.2f, …)`),
//!   names the fields it prints and gives their offsets and types;
//! - the tick oracle's trace of what the command's handler reads gives the other fields.
//!
//! Every packet starts with the command byte, the issuing comm player (`commp_id`) and a `u16` size,
//! which is the packet length minus 4 in every recorded packet. A fixed part follows, then the
//! tail: the object ids most commands carry, or raw bytes. Bytes that no code reads stay as `gap_*`
//! arrays, so a decoded packet writes back byte for byte.

mod generated;

pub use generated::*;
use genie_error::GenieResult;
use genie_support::FIELD_DOES_NOT_FIT;

/// What follows the fixed part of a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tail {
    /// Nothing: the packet is exactly the fixed part.
    None,
    /// 32-bit words.
    Words,
    /// Any bytes.
    Bytes,
}

/// Whether `packet` has the layout's shape: the fixed part, a size that matches, and a tail of its
/// kind.
fn fits(packet: &[u8], fixed: usize, tail: Tail) -> bool {
    let sized = packet.len() >= 4
        && usize::from(u16::from_le_bytes([packet[2], packet[3]])) + 4 == packet.len();
    sized
        && packet.len() >= fixed
        && match tail {
            Tail::None => packet.len() == fixed,
            Tail::Words => (packet.len() - fixed).is_multiple_of(4),
            Tail::Bytes => true,
        }
}

/// A little-endian value of a packet field.
trait Field: Copy {
    /// The value at the start of `bytes`; None for fewer bytes than its width.
    fn read(bytes: &[u8]) -> Option<Self>;
    fn write(self, out: &mut Vec<u8>);
}

macro_rules! numeric_field {
    ($($t:ty),*) => {$(
        impl Field for $t {
            fn read(bytes: &[u8]) -> Option<Self> {
                bytes
                    .get(..size_of::<$t>())?
                    .try_into()
                    .ok()
                    .map(<$t>::from_le_bytes)
            }
            fn write(self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
        }
    )*};
}
numeric_field!(u8, i8, u16, i16, u32, i32, u64, i64, f32);

impl<const N: usize> Field for [u8; N] {
    fn read(bytes: &[u8]) -> Option<Self> {
        bytes.get(..N)?.try_into().ok()
    }
    fn write(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self);
    }
}

/// The field at `at`; None for a packet too short to hold it.
fn get<T: Field>(packet: &[u8], at: usize) -> Option<T> {
    T::read(packet.get(at..)?)
}

fn put<T: Field>(out: &mut Vec<u8>, value: T) {
    value.write(out);
}

/// The words from `at` to the end.
fn words(packet: &[u8], at: usize) -> Option<Vec<i32>> {
    let tail = packet.get(at..)?;
    let (words, rest) = tail.as_chunks::<4>();
    rest.is_empty()
        .then(|| words.iter().map(|&word| i32::from_le_bytes(word)).collect())
}

/// The bytes from `at` to the end.
fn bytes(packet: &[u8], at: usize) -> Option<Vec<u8>> {
    packet.get(at..).map(<[u8]>::to_vec)
}

/// Write the size into a packet the encoder has filled.
///
/// # Errors
///
/// Invalid input for a packet whose size does not fit its `u16`, or one without
/// the size's two bytes.
fn finish(mut out: Vec<u8>) -> GenieResult<Vec<u8>> {
    let size = u16::try_from(out.len().saturating_sub(4)).map_err(|_| FIELD_DOES_NOT_FIT)?;
    out.get_mut(2..4)
        .ok_or(FIELD_DOES_NOT_FIT)?
        .copy_from_slice(&size.to_le_bytes());
    Ok(out)
}
