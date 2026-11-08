// SPDX-License-Identifier: FSL-1.1-MIT
//! macOS FnEnv. Read-only: it reads two named preference keys with the CoreFoundation preferences
//! read call and lists running apps' bundle IDs. It never writes a setting, never starts a process
//! and never dumps a whole preferences domain.
//! (`CFPreferencesCopyAppValue(key, domain)` returns one value of one domain, like
//! `defaults read <domain> <key>`.)

use super::FnEnv;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSDictionary, NSNumber, NSString};

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    /// Returns a retained (+1) value, or null when the key is not set. `NSString` is toll-free
    /// bridged to `CFString`, so the pointers are interchangeable.
    fn CFPreferencesCopyAppValue(key: &NSString, application_id: &NSString) -> *mut AnyObject;
}

/// The two domains we may read, and the one key we read from each.
const HITOOLBOX: &str = "com.apple.HIToolbox";
const FN_USAGE_KEY: &str = "AppleFnUsageType";
const SYMBOLIC: &str = "com.apple.symbolichotkeys";
const SYMBOLIC_KEY: &str = "AppleSymbolicHotKeys";
/// "Press Globe twice to start Dictation" (found on macOS 26.5.1, step 1 sub-step 0).
const DICTATION_ID: &str = "164";

fn read(domain: &str, key: &str) -> Option<Retained<AnyObject>> {
    let (domain, key) = (NSString::from_str(domain), NSString::from_str(key));
    // SAFETY: both arguments are valid NSStrings; the call only reads. A non-null result is owned
    // by us (the "Copy" rule), so `from_raw` takes over that one reference.
    unsafe { Retained::from_raw(CFPreferencesCopyAppValue(&key, &domain)) }
}

pub struct MacFnEnv;

impl FnEnv for MacFnEnv {
    fn globe_usage(&self) -> Option<i64> {
        let value = read(HITOOLBOX, FN_USAGE_KEY)?;
        value.downcast_ref::<NSNumber>().map(|n| n.as_i64())
    }

    fn dictation_shortcut_on(&self) -> Option<bool> {
        let all = read(SYMBOLIC, SYMBOLIC_KEY)?;
        let all = all.downcast_ref::<NSDictionary>()?;
        let entry = all.objectForKey(&*NSString::from_str(DICTATION_ID))?;
        let entry = entry.downcast_ref::<NSDictionary>()?;
        let enabled = entry.objectForKey(&*NSString::from_str("enabled"))?;
        enabled.downcast_ref::<NSNumber>().map(|n| n.as_bool())
    }

    fn running_bundle_ids(&self) -> Vec<String> {
        NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter_map(|app| app.bundleIdentifier())
            .map(|id| id.to_string())
            .collect()
    }
}
