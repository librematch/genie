use super::*;
use anyhow::Context;
use std::fs::File;

/// A group whose count claims more hotkeys than the file holds fails at the file's end, not on
/// its allocation.
#[test]
fn hotkeys_past_the_file_fail_at_its_end() {
    let bytes = u32::MAX.to_le_bytes();
    assert!(HotkeyGroup::read_from(&mut bytes.as_slice()).is_err());
}

#[test]
fn aoc1() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    // The test only checks that the file reads.
    let _ = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    Ok(())
}

#[test]
fn aoc2() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc2.hki")?;
    // The test only checks that the file reads.
    let _ = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    Ok(())
}

#[test]
fn aoc3() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc3.hki")?;
    // The test only checks that the file reads.
    let _ = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    Ok(())
}

#[test]
fn hd0() -> anyhow::Result<()> {
    let mut f = File::open("test/files/hd0.hki")?;
    // The test only checks that the file reads.
    let _ = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    Ok(())
}

#[test]
fn hd1() -> anyhow::Result<()> {
    let mut f = File::open("test/files/hd1.hki")?;
    // The test only checks that the file reads.
    let _ = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    Ok(())
}

#[test]
fn wk() -> anyhow::Result<()> {
    let mut f = File::open("test/files/wk.hki")?;
    // The test only checks that the file reads.
    let _ = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    Ok(())
}

#[test]
fn hk_group_iter() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let group = info.group(HotkeyGroupId::UnitCommands as usize).unwrap();
    let mut hotkey_iter = group.iter();
    assert_eq!(19214, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19215, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19224, hotkey_iter.next().unwrap().string_id);
    assert_eq!(-1, hotkey_iter.next().unwrap().string_id);
    assert_eq!(-1, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19216, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19225, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19241, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19242, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19221, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19222, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19012, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19000, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19002, hotkey_iter.next().unwrap().string_id);
    assert_eq!(19220, hotkey_iter.next().unwrap().string_id);
    assert_eq!(None, hotkey_iter.next());
    Ok(())
}

#[test]
fn hk_info_iter() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let mut iter = info.iter();
    assert_eq!(
        info.group(HotkeyGroupId::UnitCommands as usize),
        iter.next()
    );
    assert_eq!(
        info.group(HotkeyGroupId::GameCommands as usize),
        iter.next()
    );
    assert_eq!(info.group(HotkeyGroupId::Scroll as usize), iter.next());
    assert_eq!(info.group(HotkeyGroupId::Villager as usize), iter.next());
    assert_eq!(info.group(HotkeyGroupId::TownCenter as usize), iter.next());
    assert_eq!(info.group(HotkeyGroupId::Dock as usize), iter.next());
    assert_eq!(info.group(HotkeyGroupId::Barracks as usize), iter.next());
    assert_eq!(
        info.group(HotkeyGroupId::ArcheryRange as usize),
        iter.next()
    );
    assert_eq!(info.group(HotkeyGroupId::Stable as usize), iter.next());
    assert_eq!(
        info.group(HotkeyGroupId::SiegeWorkshop as usize),
        iter.next()
    );
    assert_eq!(info.group(HotkeyGroupId::Monastery as usize), iter.next());
    assert_eq!(info.group(HotkeyGroupId::Market as usize), iter.next());
    assert_eq!(
        info.group(HotkeyGroupId::MilitaryUnits as usize),
        iter.next()
    );
    assert_eq!(info.group(HotkeyGroupId::Castle as usize), iter.next());
    assert_eq!(None, iter.next());
    Ok(())
}

#[test]
fn hk_group_unbind() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let group0 = info.group(HotkeyGroupId::UnitCommands as usize).unwrap();
    let group1 = group0.unbind(UnitCommandHotkeys::BuildEconomic as usize)?;
    assert_eq!(66, group0.hotkey(0).unwrap().key);
    assert_eq!(0, group1.hotkey(0).unwrap().key);
    Ok(())
}

#[test]
fn hk_group_bind() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let group0 = info.group(HotkeyGroupId::UnitCommands as usize).unwrap();
    let group1 = group0.bind(
        UnitCommandHotkeys::BuildEconomic as usize,
        65,
        false,
        false,
        false,
    )?;
    assert_eq!(66, group0.hotkey(0).unwrap().key);
    assert_eq!(65, group1.hotkey(0).unwrap().key);
    Ok(())
}

#[test]
fn hk_group_bad_index() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let group = info.group(HotkeyGroupId::UnitCommands as usize).unwrap();
    let result = group.unbind(99999);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn hk_info_unbind() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info0 = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let info1 = info0.unbind_key(0, 0)?;
    assert_eq!(
        66,
        info0
            .group(HotkeyGroupId::UnitCommands as usize)
            .unwrap()
            .hotkey(0)
            .unwrap()
            .key
    );
    assert_eq!(
        0,
        info1
            .group(HotkeyGroupId::UnitCommands as usize)
            .unwrap()
            .hotkey(0)
            .unwrap()
            .key
    );
    Ok(())
}

#[test]
fn hk_info_bind() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info0 = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let info1 = info0.bind_key(0, 0, 65, false, false, false)?;
    assert_eq!(
        66,
        info0
            .group(HotkeyGroupId::UnitCommands as usize)
            .unwrap()
            .hotkey(0)
            .unwrap()
            .key
    );
    assert_eq!(
        65,
        info1
            .group(HotkeyGroupId::UnitCommands as usize)
            .unwrap()
            .hotkey(0)
            .unwrap()
            .key
    );
    Ok(())
}

#[test]
fn hk_info_bad_index_group() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let result = info.unbind_key(999_999, 0);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn hk_info_bad_index_hk() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let result = info.unbind_key(0, 999_999);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_keycode_to_bindings_map() -> anyhow::Result<()> {
    let mut f = File::open("test/files/aoc1.hki")?;
    let info = HotkeyInfo::read_from(&mut f).context("failed to read file")?;
    let map = info.bindings_per_keycode();
    // 19270: Ctrl-65
    let h0 = Hotkey::default().string_id(19270).key(65).ctrl(true);
    // 19062: 65
    let h1 = Hotkey::default().string_id(19062).key(65);
    // 19059: 65
    let h2 = Hotkey::default().string_id(19059).key(65);
    // 19038: 65
    let h3 = Hotkey::default().string_id(19038).key(65);
    // 19285: 65
    let h4 = Hotkey::default().string_id(19285).key(65);
    // 19315: 65
    let h5 = Hotkey::default().string_id(19315).key(65);
    let hotkeys = vec![h0, h1, h2, h3, h4, h5];
    assert_eq!(Some(&hotkeys), map.get(&65));
    Ok(())
}

/// A hotkey file writes back what it read: each test file, written and read again, gives the
/// same hotkeys.
#[test]
fn hotkey_file_writes_back_what_it_read() -> anyhow::Result<()> {
    for name in ["aoc1", "aoc2", "aoc3", "hd0", "hd1", "wk"] {
        let mut f = File::open(format!("test/files/{name}.hki"))?;
        let info = HotkeyInfo::read_from(&mut f)?;
        let mut out = vec![];
        info.write_to(&mut out)?;
        let again = HotkeyInfo::read_from(&mut out.as_slice())?;
        assert_eq!(again, info, "{name}");
    }
    Ok(())
}

/// The keycodes a language file names: F1 to F9, F11 to F24 (F10 opens the menu) and the
/// three mouse wheel and button codes 253 to 255; none for any other.
#[test]
fn keycode_string_id_names_the_function_keys_and_mouse_codes() {
    let named: Vec<(i32, i32)> = (0..300)
        .filter_map(|k| keycode_id(k).map(|id| (k, id)))
        .collect();
    let mut expected: Vec<(i32, i32)> = (112..=120).map(|k| (k, k + 19433)).collect();
    expected.extend((122..=135).map(|k| (k, k + 19433)));
    expected.extend([(253, 19712), (254, 19711), (255, 19710)]);
    assert_eq!(named, expected);
}
