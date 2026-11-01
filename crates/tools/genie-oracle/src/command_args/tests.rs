use crate::Error;
use crate::command_args::frames;
use genie_rec::de::{Chapter, Checkpoint, Op};

/// A chapter after the first (no pregame record) of the ops `ops`.
fn body(ops: &[Op<'_>]) -> Vec<u8> {
    let mut out = vec![];
    for op in ops {
        op.write(&mut out).unwrap();
    }
    out
}

fn chapter(body: &[u8]) -> Chapter<'_> {
    Chapter {
        index: 1,
        offset: 0,
        compressed_header: &[],
        body_offset: 0,
        body,
    }
}

/// A sync starts a frame and its commands follow it; a checkpoint is a frame's too; other
/// ops are skipped; the frames stop at the limit.
#[test]
fn the_frames_hold_their_dt_checkpoint_and_commands() {
    let checkpoint = Box::new(Checkpoint {
        lanes: [[0; 11]; 8],
        world_time: 1000,
    });
    let bytes = body(&[
        Op::Sync { dt: 50 },
        Op::Command {
            packet: &[3, 1, 0, 0],
            world_time: 0,
        },
        Op::Command {
            packet: &[3, 2, 0, 0],
            world_time: 0,
        },
        Op::Chat {
            channel: -1,
            text: b"hi",
        },
        Op::Checkpoint { dt: 60, checkpoint },
        Op::Sync { dt: 70 },
    ]);
    let all = frames(&chapter(&bytes), 10).unwrap();
    assert_eq!(
        all.iter().map(|f| f.dt.get()).collect::<Vec<_>>(),
        [50, 60, 70]
    );
    assert_eq!(all[0].commands, [&[3, 1, 0, 0][..], &[3, 2, 0, 0][..]]);
    assert!(all[0].checkpoint.is_none());
    assert_eq!(all[1].checkpoint.as_ref().map(|c| c.world_time), Some(1000));
    let two = frames(&chapter(&bytes), 2).unwrap();
    assert_eq!(two.len(), 2);
}

#[test]
fn a_command_before_the_first_sync_is_an_error() {
    let bytes = body(&[Op::Command {
        packet: &[3, 1],
        world_time: 0,
    }]);
    assert!(matches!(
        frames(&chapter(&bytes), 10),
        Err(Error::CommandBeforeFirstSync)
    ));
}

/// An op the body cannot read ends the frames with its error.
#[test]
fn an_unreadable_op_is_an_error() {
    let bytes = [9, 0, 0, 0];
    assert!(matches!(frames(&chapter(&bytes), 10), Err(Error::Rec(_))));
}
