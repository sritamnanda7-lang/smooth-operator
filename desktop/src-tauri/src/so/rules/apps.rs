// SPDX-License-Identifier: FSL-1.1-MIT
//! Installed-app list and name resolution. `resolve` is a pure function (same input, same output).

use super::phrase::{normalise, MAX_APP_CHARS};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub name: String,
    pub path: PathBuf,
    pub running: bool,
}

/// `Ambiguous` holds at most 3 names, sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolve {
    Found(AppEntry),
    NotFound,
    Ambiguous(Vec<String>),
}

pub trait AppCatalog: Send + Sync {
    fn list(&self) -> Vec<AppEntry>;
}

/// (alias, app name): fixed, ours. A hit counts only if the target app is installed.
pub const ALIASES: &[(&str, &str)] = &[
    ("vs code", "Visual Studio Code"),
    ("vscode", "Visual Studio Code"),
    ("code", "Visual Studio Code"),
    ("settings", "System Settings"),
    ("system preferences", "System Settings"),
    ("quicktime", "QuickTime Player"),
];
/// Leading vendor words that may be dropped from an installed app's name (tier 3).
pub const VENDORS: &[&str] = &["google", "microsoft", "adobe", "apple"];

/// Most names listed when more than one app matches.
const MAX_AMBIGUOUS: usize = 3;

fn squash(s: &str) -> String {
    s.chars().filter(|c| *c != ' ').collect()
}

/// Same text, ignoring case and spaces ("facetime" = "FaceTime").
fn same(a: &str, b: &str) -> bool {
    a == b || squash(a) == squash(b)
}

/// One distinct name per hit (the first entry listed wins). Sorted, so the result is stable.
fn verdict(mut hits: Vec<&AppEntry>) -> Option<Resolve> {
    hits.sort_by(|a, b| a.name.cmp(&b.name));
    hits.dedup_by(|a, b| a.name == b.name);
    match hits.len() {
        0 => None,
        1 => Some(Resolve::Found(hits[0].clone())),
        _ => Some(Resolve::Ambiguous(hits.iter().take(MAX_AMBIGUOUS).map(|a| a.name.clone()).collect())),
    }
}

/// Name forms of an installed app for tier 3: one leading vendor word and/or one trailing year dropped.
/// Empty when neither applies (then tier 1 already covered it).
fn trimmed_forms(norm: &str) -> Vec<String> {
    let words: Vec<&str> = norm.split(' ').collect();
    let vendor = words.len() > 1 && VENDORS.contains(&words[0]);
    let last = words[words.len() - 1];
    let year = words.len() > 1
        && last.len() == 4
        && (last.starts_with("19") || last.starts_with("20"))
        && last.chars().all(|c| c.is_ascii_digit());
    let mut forms = vec![];
    if vendor && year {
        forms.push(words[1..words.len() - 1].join(" "));
    }
    if vendor {
        forms.push(words[1..].join(" "));
    }
    if year {
        forms.push(words[..words.len() - 1].join(" "));
    }
    forms
}

/// Finds the installed app for X (the normalised text). Pure; no guessing: no substring, prefix or
/// spelling match. Candidate names, in order: X; X without a leading "the"; X without a trailing "app"
/// (both dropped too). The first candidate with a hit (found or ambiguous) is used. Tiers, first hit
/// wins: 1 exact (case/space-insensitive), 2 alias table (target must be installed), 3 vendor-trimmed.
pub fn resolve(x: &str, apps: &[AppEntry]) -> Resolve {
    let x = normalise(x);
    let no_the = x.strip_prefix("the ").unwrap_or(&x).to_string();
    let no_app = |s: &str| s.strip_suffix(" app").unwrap_or(s).to_string();
    let mut candidates: Vec<String> = vec![x.clone(), no_the.clone(), no_app(&x), no_app(&no_the)];
    candidates.retain(|c| !c.is_empty());
    let mut seen = HashSet::new();
    candidates.retain(|c| seen.insert(c.clone()));
    if candidates.is_empty() {
        return Resolve::NotFound;
    }
    let normed: Vec<String> = apps.iter().map(|a| normalise(&a.name)).collect();
    for c in &candidates {
        // Tier 1: exact.
        let hits = apps.iter().zip(&normed).filter(|(_, n)| same(n, c)).map(|(a, _)| a).collect();
        if let Some(r) = verdict(hits) {
            return r;
        }
        // Tier 2: alias, only if the target is installed.
        let hits = ALIASES
            .iter()
            .filter(|(alias, _)| same(&normalise(alias), c))
            .flat_map(|(_, target)| {
                let t = normalise(target);
                apps.iter().zip(&normed).filter(move |(_, n)| **n == t).map(|(a, _)| a)
            })
            .collect();
        if let Some(r) = verdict(hits) {
            return r;
        }
        // Tier 3: vendor/year trimmed.
        let hits = apps
            .iter()
            .zip(&normed)
            .filter(|(_, n)| trimmed_forms(n).iter().any(|f| same(f, c)))
            .map(|(a, _)| a)
            .collect();
        if let Some(r) = verdict(hits) {
            return r;
        }
    }
    Resolve::NotFound
}

/// Control chars removed, at most 40 chars plus "…".
pub fn shown_name(x: &str) -> String {
    let clean: String = x.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() <= MAX_APP_CHARS {
        return clean;
    }
    let mut cut: String = clean.chars().take(MAX_APP_CHARS).collect();
    cut.push('\u{2026}');
    cut
}

/// Scans folders for `.app` bundles: in each folder given, `.app` items at depth 1 and inside one level
/// of ordinary subfolders (vendor folders). Never looks inside a `.app`. The same name in two places is
/// one app; the first folder in the order given wins.
pub struct FsAppCatalog {
    dirs: Vec<PathBuf>,
}

impl FsAppCatalog {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        FsAppCatalog { dirs }
    }
}

/// Sorted children (so the result does not depend on the disk's order); empty if unreadable.
fn children(dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    v.sort();
    v
}

fn app_name(path: &std::path::Path) -> Option<String> {
    let file = path.file_name()?.to_str()?;
    let name = file.strip_suffix(".app")?;
    (!name.is_empty() && path.is_dir()).then(|| name.to_string())
}

impl AppCatalog for FsAppCatalog {
    fn list(&self) -> Vec<AppEntry> {
        let mut out: Vec<AppEntry> = vec![];
        let mut seen: HashSet<String> = HashSet::new();
        let mut add = |path: PathBuf| -> bool {
            let Some(name) = app_name(&path) else { return false };
            if seen.insert(name.clone()) {
                out.push(AppEntry { name, path, running: false });
            }
            true
        };
        for dir in &self.dirs {
            for item in children(dir) {
                if add(item.clone()) || !item.is_dir() || is_hidden(&item) {
                    continue;
                }
                for inner in children(&item) {
                    add(inner);
                }
            }
        }
        out
    }
}

fn is_hidden(p: &std::path::Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'))
}
