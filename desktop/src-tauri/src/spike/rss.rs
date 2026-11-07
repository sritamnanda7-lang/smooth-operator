// SPDX-License-Identifier: FSL-1.1-MIT
//! Memory (RSS = resident memory) of this app and every child process it started
//! (the transcription worker is a child). Spike-only; shells out to the OS tools.
//! Does not include the system WebKit/WebView2 helper processes (gate 4 measures those).

/// Total resident memory of the app's process tree, in KB. `None` if the OS tool failed.
pub fn tree_kb() -> Option<u64> {
    imp::tree_kb()
}

/// Parse `ps -axo pid=,ppid=,rss=` output and sum the tree under `root`.
fn sum_tree(ps_output: &str, root: u32) -> u64 {
    let rows: Vec<(u32, u32, u64)> = ps_output
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect();
    let mut members = vec![root];
    let mut i = 0;
    while i < members.len() {
        let parent = members[i];
        for (pid, ppid, _) in &rows {
            if *ppid == parent && !members.contains(pid) {
                members.push(*pid);
            }
        }
        i += 1;
    }
    rows.iter().filter(|(pid, _, _)| members.contains(pid)).map(|(_, _, rss)| *rss).sum()
}

#[cfg(unix)]
mod imp {
    pub fn tree_kb() -> Option<u64> {
        let out = std::process::Command::new("/bin/ps").args(["-axo", "pid=,ppid=,rss="]).output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(super::sum_tree(&String::from_utf8_lossy(&out.stdout), std::process::id()))
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;

    /// Same idea through PowerShell: WorkingSetSize of the app and its descendants.
    pub fn tree_kb() -> Option<u64> {
        let script = "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId) $($_.ParentProcessId) $([math]::Round($_.WorkingSetSize/1KB))\" }";
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: no console flash
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(super::sum_tree(&String::from_utf8_lossy(&out.stdout), std::process::id()))
    }
}

#[cfg(test)]
mod tests {
    use super::sum_tree;

    #[test]
    fn sums_root_and_descendants_only() {
        let ps = "  10     1  1000\n  20    10   500\n  30    20   250\n  40     1  9999\n";
        assert_eq!(sum_tree(ps, 10), 1750);
        assert_eq!(sum_tree(ps, 40), 9999);
        assert_eq!(sum_tree(ps, 99), 0);
    }
}
