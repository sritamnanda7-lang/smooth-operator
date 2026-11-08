// SPDX-License-Identifier: FSL-1.1-MIT
//! T17: HotkeyChoice.
use crate::so::settings::{HotkeyChoice, Platform};

#[test]
fn t17_serde_names() {
    assert_eq!(serde_json::to_string(&HotkeyChoice::Fn).unwrap(), "\"fn\"");
    assert_eq!(serde_json::to_string(&HotkeyChoice::RightCmd).unwrap(), "\"right_cmd\"");
    assert_eq!(serde_json::to_string(&HotkeyChoice::RightCtrl).unwrap(), "\"right_ctrl\"");
    assert_eq!(serde_json::from_str::<HotkeyChoice>("\"right_cmd\"").unwrap(), HotkeyChoice::RightCmd);
}

#[test]
fn t17_os_defaults() {
    assert_eq!(HotkeyChoice::default_for(Platform::Mac), HotkeyChoice::Fn);
    assert_eq!(HotkeyChoice::default_for(Platform::Windows), HotkeyChoice::RightCtrl);
}

#[test]
fn t17_options_per_os() {
    assert_eq!(HotkeyChoice::options_for(Platform::Mac), vec![HotkeyChoice::Fn, HotkeyChoice::RightCmd]);
    assert_eq!(HotkeyChoice::options_for(Platform::Windows), vec![HotkeyChoice::RightCtrl]);
}

#[test]
fn t17_windows_rejects_fn_and_right_cmd() {
    assert!(HotkeyChoice::from_command("fn", Platform::Windows).is_err());
    assert!(HotkeyChoice::from_command("right_cmd", Platform::Windows).is_err());
    assert_eq!(HotkeyChoice::from_command("right_ctrl", Platform::Windows), Ok(HotkeyChoice::RightCtrl));
}

#[test]
fn t17_mac_accepts_fn_and_right_cmd_rejects_right_ctrl() {
    assert_eq!(HotkeyChoice::from_command("fn", Platform::Mac), Ok(HotkeyChoice::Fn));
    assert_eq!(HotkeyChoice::from_command("right_cmd", Platform::Mac), Ok(HotkeyChoice::RightCmd));
    assert!(HotkeyChoice::from_command("right_ctrl", Platform::Mac).is_err());
}

#[test]
fn t17_bad_command_value_is_error() {
    for bad in ["", "Fn", "left_cmd", "cmd", "fn ", "../x", "null"] {
        assert!(HotkeyChoice::from_command(bad, Platform::Mac).is_err(), "{bad:?}");
    }
}

#[test]
fn t17_bad_stored_value_gives_os_default() {
    for bad in [None, Some(""), Some("garbage"), Some("FN"), Some("left_cmd")] {
        assert_eq!(HotkeyChoice::from_stored(bad, Platform::Mac), HotkeyChoice::Fn, "{bad:?}");
        assert_eq!(HotkeyChoice::from_stored(bad, Platform::Windows), HotkeyChoice::RightCtrl, "{bad:?}");
    }
    // A stored value that is not allowed on this OS also falls back.
    assert_eq!(HotkeyChoice::from_stored(Some("fn"), Platform::Windows), HotkeyChoice::RightCtrl);
    assert_eq!(HotkeyChoice::from_stored(Some("right_cmd"), Platform::Windows), HotkeyChoice::RightCtrl);
    // Good stored values are kept.
    assert_eq!(HotkeyChoice::from_stored(Some("right_cmd"), Platform::Mac), HotkeyChoice::RightCmd);
    assert_eq!(HotkeyChoice::from_stored(Some("right_ctrl"), Platform::Windows), HotkeyChoice::RightCtrl);
}
