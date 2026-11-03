use crate::key::key;

/// Each part of the key changes it: a file's content, the build, the layer's text and the
/// schema hash; the same parts give the same key.
#[test]
fn each_part_of_the_key_changes_it() {
    let schema = [0u8; 32];
    let dat = |bytes: &'static str| [("empires2_x2_p1.dat", bytes.as_bytes())];
    let base = key(&dat("dat"), 158_041, b"layer", &schema);
    assert_ne!(base, key(&dat("dat2"), 158_041, b"layer", &schema));
    assert_ne!(base, key(&dat("dat"), 158_042, b"layer", &schema));
    assert_ne!(base, key(&dat("dat"), 158_041, b"layer2", &schema));
    assert_ne!(base, key(&dat("dat"), 158_041, b"layer", &[1u8; 32]));
    assert_eq!(base, key(&dat("dat"), 158_041, b"layer", &schema));
}

/// A name and its content do not run together: the same bytes split another way give another
/// key, and so does another file name.
#[test]
fn names_and_contents_stay_apart() {
    let schema = [0u8; 32];
    assert_ne!(
        key(&[("ab", b"c")], 1, b"", &schema),
        key(&[("a", b"bc")], 1, b"", &schema)
    );
    assert_ne!(
        key(&[("a.dat", b"x")], 1, b"", &schema),
        key(&[("b.dat", b"x")], 1, b"", &schema)
    );
}

/// The hex form is the 64 lowercase digits of the hash.
#[test]
fn the_hex_form_is_sixty_four_lowercase_digits() {
    let k = key(&[], 0, b"", &[0; 32]).hex();
    assert_eq!(k.len(), 64);
    assert!(
        k.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
}
