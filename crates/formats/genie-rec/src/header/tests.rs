use crate::header::{
    Particle, ParticleSystem, skip_game_mode_and_campaign, skip_lobby_players,
    skip_sent_commanded_units, skip_world_clock,
};
use crate::test_support::{Bytes, read_all, skipped_length};

/// A particle: its start, facet and update, the sprite, the location and the flags.
fn particle() -> Bytes {
    Bytes::default()
        .words(&[10, 2, 30])
        .u16(400)
        .floats(&[1.0, 2.0, 0.0])
        .u8(1)
}

#[test]
fn particle_reads_its_sprite_and_location() {
    let particle = read_all::<Particle>(&particle().0, 11.97);
    assert_eq!(
        (particle.start, particle.facet, particle.update),
        (10, 2, 30)
    );
    assert_eq!(u16::from(particle.sprite_id), 400);
    assert_eq!(particle.location, (1.0, 2.0, 0.0));
    assert_eq!(particle.flags, 1);
}

#[test]
fn particle_system_reads_its_particles_behind_their_count() {
    let bytes = Bytes::default()
        .words(&[5000, 2])
        .raw(&particle().0)
        .raw(&particle().0)
        .0;
    let system = read_all::<ParticleSystem>(&bytes, 11.97);
    assert_eq!(system.world_time, 5000);
    assert_eq!(system.particles.len(), 2);
}

#[test]
fn the_world_clock_takes_47_bytes() {
    assert_eq!(skipped_length(skip_world_clock, 11.97), 47);
}

#[test]
fn the_game_mode_and_campaign_come_with_their_versions() {
    assert_eq!(skipped_length(skip_game_mode_and_campaign, 10.12), 53);
    assert_eq!(skipped_length(skip_game_mode_and_campaign, 10.13), 59);
    assert_eq!(skipped_length(skip_game_mode_and_campaign, 11.76), 61);
}

/// Nine players of an empty name each from 11.32.
#[test]
fn the_lobby_players_come_with_their_versions() {
    assert_eq!(skipped_length(skip_lobby_players, 7.15), 0);
    assert_eq!(skipped_length(skip_lobby_players, 7.16), 4);
    assert_eq!(skipped_length(skip_lobby_players, 10.23), 8);
    assert_eq!(skipped_length(skip_lobby_players, 11.32), 116);
    assert_eq!(skipped_length(skip_lobby_players, 11.35), 152);
    assert_eq!(skipped_length(skip_lobby_players, 11.36), 156);
}

/// A name's bytes follow its length.
#[test]
fn lobby_player_name_follows_its_length() {
    let mut bytes = Bytes::default().words(&[0, 0, 1, 2, 3]).raw(b"Ann");
    for _ in 0..8 {
        bytes = bytes.words(&[0, 0, 0]);
    }
    let mut reader = crate::test_support::header_reader(&bytes.0, 11.32);
    skip_lobby_players(&mut reader).unwrap();
    assert!(crate::test_support::consumed(&mut reader));
}

#[test]
fn the_sent_commanded_units_come_from_11_38() {
    assert_eq!(skipped_length(skip_sent_commanded_units, 11.37), 0);
    assert_eq!(skipped_length(skip_sent_commanded_units, 11.38), 1613);
    assert_eq!(skipped_length(skip_sent_commanded_units, 11.39), 1617);
}
