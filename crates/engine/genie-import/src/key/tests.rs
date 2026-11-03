use crate::key::key;

/// Each part of the key changes it: a file's content and the layout hash; the same parts give
/// the same key.
#[test]
fn each_part_of_the_key_changes_it() {
    let layout = [0u8; 32];
    let dat = |bytes: &'static str| [("empires2_x2_p1.dat", bytes.as_bytes())];
    let base = key(&dat("dat"), &layout);
    assert_ne!(base, key(&dat("dat2"), &layout));
    assert_ne!(base, key(&dat("dat"), &[1u8; 32]));
    assert_eq!(base, key(&dat("dat"), &layout));
}

/// A name and its content do not run together: the same bytes split another way give another
/// key, and so does another file name or a file more.
#[test]
fn names_and_contents_stay_apart() {
    let layout = [0u8; 32];
    assert_ne!(key(&[("ab", b"c")], &layout), key(&[("a", b"bc")], &layout));
    assert_ne!(
        key(&[("a.dat", b"x")], &layout),
        key(&[("b.dat", b"x")], &layout)
    );
    assert_ne!(
        key(&[("a.dat", b"x")], &layout),
        key(&[("a.dat", b"x"), ("b.json", b"")], &layout)
    );
}

/// The hex form is the 64 lowercase digits of the hash.
#[test]
fn the_hex_form_is_sixty_four_lowercase_digits() {
    let k = key(&[], &[0; 32]).hex();
    assert_eq!(k.len(), 64);
    assert!(
        k.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
}
