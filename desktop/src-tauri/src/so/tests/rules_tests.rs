// SPDX-License-Identifier: FSL-1.1-MIT
//! Step 3 tests: phrases P1-P5, app matching A1-A9, router F1-F8. Pure functions, temp folders and fakes
//! that record every call; no real keys, apps or microphone. Numbers follow draft/step-3-plan.md.
use super::rules_support::*;
use crate::so::panel::{Kind, Line};
use crate::so::rules::apps::{resolve, shown_name, AppCatalog, AppEntry, FsAppCatalog, Resolve, ALIASES, VENDORS};
use crate::so::rules::phrase::{match_command, normalise, FastCommand, MediaKey, MAX_APP_CHARS, MAX_APP_WORDS};
use crate::so::rules::router::FastRouter;
use crate::so::rules::{Route, TranscriptRouter};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================================
// Phrases (pure)
// =============================================================================================
use FastCommand::{Media, Open, SwitchTo};

#[test]
fn p1_every_table_row_matches_including_punctuation_case_spacing_and_please() {
    let rows: [(MediaKey, &[&str]); 3] = [
        (MediaKey::PlayPause, &["play", "resume", "play music", "resume music", "pause", "pause music", "pause the music"]),
        (MediaKey::Next, &["next", "next track", "next song", "skip", "skip song", "skip track", "skip this song"]),
        (MediaKey::Previous, &["previous", "previous track", "previous song", "last track", "last song"]),
    ];
    for (key, phrases) in rows {
        for p in phrases {
            assert_eq!(match_command(p), Some(Media(key)), "row {p:?}");
        }
    }
    for (said, key) in [
        ("Pause.", MediaKey::PlayPause),
        ("NEXT!", MediaKey::Next),
        ("  next   track ", MediaKey::Next),
        ("Please pause", MediaKey::PlayPause),
        ("skip this song please", MediaKey::Next),
        ("Please, play music, please!", MediaKey::PlayPause),
        ("Previous Song.", MediaKey::Previous),
        ("pause\tthe\nmusic", MediaKey::PlayPause),
    ] {
        assert_eq!(match_command(said), Some(Media(key)), "said {said:?}");
    }
}

#[test]
fn p2_everything_else_passes_through() {
    for t in [
        "play blinding lights",
        "pause in five minutes",
        "stop",
        "next tab",
        "skip ads",
        "",
        "   ",
        "pause pause",
        "playlist",
        "please please pause", // only one leading "please" is dropped
        "play the music",
        "volume up",
        "mute",
        "hello world",
    ] {
        assert_eq!(match_command(t), None, "must pass through: {t:?}");
    }
    // Positive control: the table itself is live.
    assert_eq!(match_command("pause"), Some(Media(MediaKey::PlayPause)));
}

#[test]
fn p3_open_and_switch_to_carry_the_normalised_app_name() {
    assert_eq!(match_command("Open Slack."), Some(Open("slack".into())));
    assert_eq!(match_command("Switch to Google Chrome"), Some(SwitchTo("google chrome".into())));
    assert_eq!(match_command("please open Slack"), Some(Open("slack".into())));
    assert_eq!(match_command("open Slack please"), Some(Open("slack".into())));
    assert_eq!(match_command("Open   Visual-Studio Code!"), Some(Open("visual studio code".into())));
    assert_eq!(match_command("open"), None, "no app name");
    assert_eq!(match_command("switch to"), None, "no app name");
    assert_eq!(match_command("open please"), None);
    assert_eq!(match_command("switch"), None);
    assert_eq!(match_command("opening slack"), None, "the verb must match as a whole word");
}

#[test]
fn p4_app_names_of_six_words_or_over_forty_characters_pass_through() {
    assert_eq!(MAX_APP_WORDS, 5);
    assert_eq!(MAX_APP_CHARS, 40);
    assert_eq!(match_command("open one two three four five"), Some(Open("one two three four five".into())));
    assert_eq!(match_command("open one two three four five six"), None);
    assert_eq!(match_command("switch to one two three four five six"), None);
    let forty = "a".repeat(40);
    let forty_one = "a".repeat(41);
    assert_eq!(match_command(&format!("open {forty}")), Some(Open(forty.clone())));
    assert_eq!(match_command(&format!("open {forty_one}")), None);
    assert_eq!(match_command(&format!("switch to {forty_one}")), None);
}

#[test]
fn p5_normalise_lowercases_maps_symbols_and_whitespace_to_single_spaces() {
    assert_eq!(normalise("Hello World"), "hello world");
    assert_eq!(normalise("Don\u{2019}t Stop"), "don t stop", "curly apostrophe");
    assert_eq!(normalise("\u{201C}Open\u{201D} Slack"), "open slack", "curly double quotes");
    assert_eq!(normalise("Hello-World"), "hello world", "hyphen");
    assert_eq!(normalise("Tom & Jerry"), "tom and jerry", "& becomes and");
    assert_eq!(normalise("pause\tmusic"), "pause music", "tab");
    assert_eq!(normalise("pause\nmusic"), "pause music", "newline");
    assert_eq!(normalise("pause\u{2028}music"), "pause music", "U+2028");
    assert_eq!(normalise("  a   b  "), "a b");
    assert_eq!(normalise("Office 365"), "office 365", "digits stay");
    assert_eq!(normalise("Caf\u{E9}"), "caf\u{E9}", "accented letters stay");
    assert_eq!(normalise("Please, pause, please!"), "pause", "one leading and one trailing please");
    assert_eq!(normalise("please please pause"), "please pause", "only one leading please");
    assert_eq!(normalise("pause please please"), "pause please", "only one trailing please");
    assert_eq!(normalise("!!!"), "");
    assert_eq!(normalise(""), "");
    let once = normalise("Open  Visual-Studio  Code, Please!");
    assert_eq!(normalise(&once), once, "idempotent");
}

// =============================================================================================
// Apps (pure + temp folders)
// =============================================================================================
fn found(r: &Resolve) -> Option<&str> {
    match r {
        Resolve::Found(a) => Some(a.name.as_str()),
        _ => None,
    }
}

#[test]
fn a1_exact_tier_is_case_and_space_insensitive() {
    let apps = vec![app("FaceTime", false), app("Visual Studio Code", false), app("Safari", true)];
    assert_eq!(resolve("facetime", &apps), Resolve::Found(app("FaceTime", false)));
    assert_eq!(resolve("face time", &apps), Resolve::Found(app("FaceTime", false)));
    assert_eq!(resolve("visual studio code", &apps), Resolve::Found(app("Visual Studio Code", false)));
    assert_eq!(resolve("visualstudiocode", &apps), Resolve::Found(app("Visual Studio Code", false)));
    assert_eq!(resolve("safari", &apps), Resolve::Found(app("Safari", true)), "the entry comes back as listed (running flag kept)");
}

#[test]
fn a2_vendor_trimmed_tier_drops_one_leading_vendor_and_one_trailing_year() {
    let apps = vec![
        app("Google Chrome", false),
        app("Microsoft Word", false),
        app("Adobe Photoshop 2025", false),
        app("Apple Configurator", false),
    ];
    assert_eq!(found(&resolve("chrome", &apps)), Some("Google Chrome"));
    assert_eq!(found(&resolve("word", &apps)), Some("Microsoft Word"));
    assert_eq!(found(&resolve("photoshop", &apps)), Some("Adobe Photoshop 2025"));
    assert_eq!(found(&resolve("configurator", &apps)), Some("Apple Configurator"));
    assert_eq!(found(&resolve("adobe photo", &apps)), None, "tier 3 drops a vendor, it is not a prefix match");
    for v in ["google", "microsoft", "adobe", "apple"] {
        assert!(VENDORS.contains(&v), "VENDORS must list {v}");
    }
}

#[test]
fn a3_alias_hits_only_when_the_target_app_is_installed() {
    let code = vec![app("Visual Studio Code", false), app("Safari", false)];
    for alias in ["vs code", "vscode", "code"] {
        assert_eq!(found(&resolve(alias, &code)), Some("Visual Studio Code"), "alias {alias:?}");
    }
    let no_code = vec![app("Safari", false)];
    for alias in ["vs code", "vscode", "code"] {
        assert_eq!(resolve(alias, &no_code), Resolve::NotFound, "alias {alias:?} with the target missing");
    }
    let sys = vec![app("System Settings", false), app("QuickTime Player", false)];
    assert_eq!(found(&resolve("settings", &sys)), Some("System Settings"));
    assert_eq!(found(&resolve("system preferences", &sys)), Some("System Settings"));
    assert_eq!(found(&resolve("quicktime", &sys)), Some("QuickTime Player"));
    assert_eq!(resolve("system preferences", &[app("Safari", false)]), Resolve::NotFound);
    assert!(!ALIASES.is_empty(), "the alias table is filled");
}

#[test]
fn a4_ambiguous_lists_the_names_sorted_and_at_most_three() {
    let apps = vec![app("Google Drive", false), app("Adobe Drive", false), app("Safari", false)];
    assert_eq!(resolve("drive", &apps), Resolve::Ambiguous(vec!["Adobe Drive".into(), "Google Drive".into()]));
    let four = vec![app("Google Drive", false), app("Adobe Drive", false), app("Apple Drive", false), app("Microsoft Drive", false)];
    match resolve("drive", &four) {
        Resolve::Ambiguous(names) => {
            assert_eq!(names.len(), 3, "at most 3 names: {names:?}");
            let mut sorted = names.clone();
            sorted.sort();
            assert_eq!(names, sorted, "sorted");
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
    // The same app listed twice is one name, not an ambiguity.
    let twice = vec![app("Slack", false), app("Slack", true)];
    assert!(matches!(resolve("slack", &twice), Resolve::Found(_)), "{:?}", resolve("slack", &twice));
}

#[test]
fn a5_there_is_no_fuzzy_prefix_or_substring_matching() {
    let apps = vec![app("Spotify", false), app("Google Chrome", false), app("Slack", false)];
    assert_eq!(resolve("spotfy", &apps), Resolve::NotFound);
    assert_eq!(resolve("chrom", &apps), Resolve::NotFound);
    assert_eq!(resolve("slack h", &apps), Resolve::NotFound);
    assert_eq!(resolve("spot", &apps), Resolve::NotFound);
    assert_eq!(resolve("banana", &apps), Resolve::NotFound);
    assert_eq!(resolve("", &apps), Resolve::NotFound);
    // Positive controls: the right words do match in the same list.
    assert_eq!(found(&resolve("spotify", &apps)), Some("Spotify"));
    assert_eq!(found(&resolve("slack", &apps)), Some("Slack"));
}

#[test]
fn a6_tier_order_and_candidate_order() {
    // An app really named "Code" beats the alias to Visual Studio Code (tier 1 before tier 2).
    let apps = vec![app("Code", false), app("Visual Studio Code", false)];
    assert_eq!(found(&resolve("code", &apps)), Some("Code"));
    // Candidates: X, then without a leading "the", then without a trailing "app".
    let slack = vec![app("Slack", false)];
    assert_eq!(found(&resolve("the slack", &slack)), Some("Slack"));
    assert_eq!(found(&resolve("slack app", &slack)), Some("Slack"));
    assert_eq!(found(&resolve("the slack app", &slack)), Some("Slack"), "plan A6: both words dropped");
    // The first candidate that hits wins: an app literally called "The App" is found as typed.
    let odd = vec![app("The App", false), app("App", false)];
    assert_eq!(found(&resolve("the app", &odd)), Some("The App"));
}

fn mkdir(p: &std::path::Path) {
    fs::create_dir_all(p).unwrap();
}

#[test]
fn a7_fs_catalog_finds_apps_at_depth_1_and_2_and_nothing_else() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    mkdir(&a.join("Foo.app/Contents/Inner.app")); // inside a bundle: never listed
    mkdir(&a.join("Vendor/Bar.app")); // depth 2
    mkdir(&a.join("Vendor/Deep/Baz.app")); // depth 3: ignored
    mkdir(&a.join("Plain")); // a folder that is not an app
    fs::write(a.join("notes.txt"), "x").unwrap();
    fs::write(a.join("Vendor/readme.txt"), "x").unwrap();
    mkdir(&b.join("Foo.app")); // same name again: the first folder wins
    mkdir(&b.join("Qux.app"));
    let missing = tmp.path().join("does-not-exist");
    let cat = FsAppCatalog::new(vec![a.clone(), missing, b.clone()]);
    let mut list = cat.list();
    list.sort_by(|x, y| x.name.cmp(&y.name));
    let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Bar", "Foo", "Qux"], "got {list:?}");
    let by = |n: &str| list.iter().find(|e| e.name == n).unwrap();
    assert_eq!(by("Foo").path, a.join("Foo.app"), "first folder in the order wins");
    assert_eq!(by("Bar").path, a.join("Vendor/Bar.app"));
    assert_eq!(by("Qux").path, b.join("Qux.app"));
    assert!(list.iter().all(|e| !e.running), "a folder scan marks nothing running");
}

#[test]
fn a8_shown_name_drops_control_characters_and_cuts_at_40_characters() {
    assert_eq!(shown_name("slack"), "slack");
    assert_eq!(shown_name("sl\u{7}ack\n\t\u{1b}"), "slack", "control characters removed");
    let forty = "a".repeat(40);
    assert_eq!(shown_name(&forty), forty, "exactly 40 is kept whole");
    let cut = shown_name(&"b".repeat(41));
    assert_eq!(cut, format!("{}\u{2026}", "b".repeat(40)));
    assert_eq!(cut.chars().count(), 41);
    let long = shown_name(&format!("{}\u{7}{}", "c".repeat(30), "d".repeat(30)));
    assert_eq!(long.chars().count(), 41, "{long:?}");
    assert!(long.ends_with('\u{2026}') && !long.contains('\u{7}'));
}

/// Real Mac catalog: Safari, Calculator and System Settings listed, in at most 50 ms.
/// `#[ignore]`: needs a real Mac. COMPILE-ONLY for the tester; Claude runs it.
#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn a9_system_catalog_lists_safari_calculator_and_system_settings_within_50_ms() {
    use crate::so::rules::real::SystemCatalog;
    let _ = SystemCatalog.list(); // warm the file cache
    let t = Instant::now();
    let list = SystemCatalog.list();
    let took = t.elapsed();
    for n in ["Safari", "Calculator", "System Settings"] {
        assert!(list.iter().any(|e| e.name == n), "{n} not listed");
    }
    assert!(took <= Duration::from_millis(50), "scan took {took:?}");
}

// =============================================================================================
// Router (fakes record every call)
// =============================================================================================
struct R {
    router: FastRouter,
    media: Arc<FakeMedia>,
    launcher: Arc<FakeLauncher>,
    catalog: Arc<FakeCatalog>,
    clock: Arc<FakeClock>,
}
impl R {
    fn new(apps: Vec<AppEntry>) -> Self {
        let media = Arc::new(FakeMedia::default());
        let launcher = Arc::new(FakeLauncher::default());
        let catalog = FakeCatalog::new(apps);
        let clock = FakeClock::new();
        let router = FastRouter::new(media.clone(), catalog.clone(), launcher.clone(), clock.clone());
        R { router, media, launcher, catalog, clock }
    }
    fn route(&self, text: &str) -> Route {
        self.router.route(text, self.clock.at(0))
    }
    fn no_calls(&self) -> bool {
        self.media.presses().is_empty() && self.launcher.opens().is_empty() && self.catalog.lists() == 0
    }
}
fn lines(r: Route) -> Vec<Line> {
    match r {
        Route::Handled(l) => l,
        Route::PassThrough => panic!("expected Handled, got PassThrough"),
    }
}

#[test]
fn f1_media_commands_press_exactly_one_key_and_show_the_fixed_line() {
    for (said, key, line) in [
        ("pause", MediaKey::PlayPause, "Play/pause sent"),
        ("Play.", MediaKey::PlayPause, "Play/pause sent"),
        ("next track", MediaKey::Next, "Next track"),
        ("skip", MediaKey::Next, "Next track"),
        ("previous song", MediaKey::Previous, "Previous track"),
    ] {
        let r = R::new(vec![]);
        let got = lines(r.route(said));
        assert_eq!(r.media.presses(), vec![key], "{said:?}");
        assert_eq!(got.len(), 1, "{said:?}: {got:?}");
        assert_eq!(got[0].text, line, "{said:?}");
        assert_ne!(got[0].kind, Kind::Warn, "{said:?}");
        assert!(r.launcher.opens().is_empty(), "a media command opens no app");
    }
}

#[test]
fn f2_open_a_found_app_opens_its_path_once() {
    let r = R::new(vec![app("Slack", false), app("Google Chrome", true)]);
    let got = lines(r.route("Open Slack."));
    assert_eq!(r.launcher.opens(), vec![PathBuf::from("/Applications/Slack.app")]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].text, "Opening slack", "the shown X is the normalised text");
    assert!(r.media.presses().is_empty());
    let r2 = R::new(vec![app("Google Chrome", true)]);
    let got2 = lines(r2.route("open chrome"));
    assert_eq!(r2.launcher.opens(), vec![PathBuf::from("/Applications/Google Chrome.app")]);
    assert_eq!(got2[0].text, "Opening chrome");
}

#[test]
fn f3_switch_to_a_running_app_opens_its_path_once() {
    let r = R::new(vec![app("Google Chrome", true), app("TextEdit", true)]);
    let got = lines(r.route("switch to chrome"));
    assert_eq!(r.launcher.opens(), vec![PathBuf::from("/Applications/Google Chrome.app")]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].text, "Switching to chrome");
}

#[test]
fn f3_switch_to_an_app_that_is_not_running_follows_q2() {
    // Q2 (recommended option A; the plan does not record the founder's answer): launch it, same as open.
    // If the founder chose B this test changes with the plan, not silently.
    let r = R::new(vec![app("Calculator", false)]);
    let got = lines(r.route("switch to calculator"));
    assert_eq!(r.launcher.opens(), vec![PathBuf::from("/Applications/Calculator.app")]);
    assert_eq!(got.len(), 1);
    assert!(!got[0].text.is_empty());
}

#[test]
fn f4_an_unknown_app_does_nothing_and_says_so() {
    for said in ["open banana", "switch to spotfy"] {
        let r = R::new(vec![app("Spotify", false), app("Slack", false)]);
        let got = lines(r.route(said));
        assert!(r.media.presses().is_empty() && r.launcher.opens().is_empty(), "{said:?}");
        let name = said.rsplit(' ').next().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, format!("I couldn't find an app called {name}"));
    }
}

#[test]
fn f5_an_ambiguous_app_does_nothing_and_names_the_matches() {
    let r = R::new(vec![app("Google Drive", false), app("Adobe Drive", false)]);
    let got = lines(r.route("open drive"));
    assert!(r.media.presses().is_empty() && r.launcher.opens().is_empty(), "never guess");
    assert_eq!(got.len(), 1);
    let t = &got[0].text;
    assert!(t.contains("Adobe Drive") && t.contains("Google Drive"), "{t}");
    assert!(t.find("Adobe Drive") < t.find("Google Drive"), "sorted: {t}");
    assert!(t.contains("drive"), "{t}");
}

#[test]
fn f6_a_failed_media_key_warns_about_accessibility_and_a_failed_open_warns_with_the_name() {
    let r = R::new(vec![app("Slack", false)]);
    *r.media.err.lock().unwrap() = Some("ZXQERR enigo failed".into());
    let got = lines(r.route("pause"));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].kind, Kind::Warn);
    assert_eq!(got[0].text, "Media keys need Accessibility: System Settings → Privacy & Security → Accessibility");
    assert!(!got[0].text.contains("ZXQERR"));

    *r.launcher.err.lock().unwrap() = Some("ZXQERR no such file /Applications/Slack.app".into());
    let got = lines(r.route("open slack"));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].kind, Kind::Warn);
    assert_eq!(got[0].text, "Couldn't open slack");
    assert!(!got[0].text.contains("ZXQERR") && !got[0].text.contains("/Applications"));
}

#[test]
fn f7_routing_against_a_1000_app_list_takes_at_most_10_ms() {
    let mut apps: Vec<AppEntry> = (0..1000).map(|i| app(&format!("Some Application Number {i}"), i % 7 == 0)).collect();
    apps.push(app("Zebra Notes", false));
    let r = R::new(apps);
    let mut best = Duration::MAX;
    for text in ["open zebra notes", "open zebra notes", "open nothing here", "open number"] {
        let t = Instant::now();
        let _ = r.route(text);
        best = best.min(t.elapsed());
    }
    let t = Instant::now();
    let got = lines(r.route("switch to zebra notes"));
    let found_time = t.elapsed();
    assert!(best <= Duration::from_millis(10), "best of 4 took {best:?}");
    assert!(found_time <= Duration::from_millis(10), "found route took {found_time:?}");
    assert_eq!(got[0].text, "Switching to zebra notes");
    assert_eq!(r.launcher.opens().last().unwrap(), &PathBuf::from("/Applications/Zebra Notes.app"));
}

#[test]
fn f8_a_pass_through_text_makes_no_call_of_any_kind() {
    let r = R::new(vec![app("Slack", false)]);
    for said in [
        "play blinding lights",
        "stop",
        "hello world",
        "",
        "open",
        "pause in five minutes",
        "open one two three four five six",
    ] {
        assert_eq!(r.route(said), Route::PassThrough, "{said:?}");
    }
    assert!(r.no_calls(), "presses {:?}, opens {:?}, catalog scans {}", r.media.presses(), r.launcher.opens(), r.catalog.lists());
    // Positive control: a real command on the same fakes does act.
    let _ = r.route("pause");
    assert_eq!(r.media.presses().len(), 1);
}
