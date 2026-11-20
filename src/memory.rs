//! How much memory the computer has and how much a local server takes, with the levels the
//! window colours it by.

use std::sync::OnceLock;

/// Shares of the computer's memory from which the model's use shows yellow, then red.
pub const YELLOW: f64 = 0.25;
pub const RED: f64 = 0.50;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Green,
    Yellow,
    Red,
}

pub fn level(bytes: u64, total: u64) -> Level {
    let share = bytes as f64 / total.max(1) as f64;
    if share >= RED {
        Level::Red
    } else if share >= YELLOW {
        Level::Yellow
    } else {
        Level::Green
    }
}

/// `3.2 GB`, `512 MB`.
pub fn text(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1.0 { format!("{gb:.1} GB") } else { format!("{:.0} MB", bytes as f64 / 1e6) }
}

/// The memory of the computer, read once.
pub fn total() -> Option<u64> {
    static TOTAL: OnceLock<Option<u64>> = OnceLock::new();
    *TOTAL.get_or_init(read_total)
}

#[cfg(target_os = "macos")]
fn read_total() -> Option<u64> {
    let out = std::process::Command::new("/usr/sbin/sysctl").args(["-n", "hw.memsize"]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn read_total() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = info.lines().find_map(|l| l.strip_prefix("MemTotal:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
    Some(kb * 1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read_total() -> Option<u64> {
    None
}

/// The resident memory of the process listening on `port` and of the processes it started
/// (llama-server runs each model in a child of its own).
#[cfg(unix)]
pub fn of_server(port: u16) -> Option<u64> {
    let port_arg = format!("-iTCP:{port}");
    let out = std::process::Command::new("lsof").args(["-nP", "-t", &port_arg, "-sTCP:LISTEN"]).output().ok()?;
    let root: u32 = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().parse().ok()?;
    let ps = std::process::Command::new("ps").args(["-axo", "pid=,ppid=,rss="]).output().ok()?;
    Some(tree_rss(root, &String::from_utf8_lossy(&ps.stdout)))
}

#[cfg(not(unix))]
pub fn of_server(_port: u16) -> Option<u64> {
    None
}

#[cfg(unix)]
/// The resident memory of `root` and its descendants, from `ps -axo pid=,ppid=,rss=` (KiB).
fn tree_rss(root: u32, ps: &str) -> u64 {
    let rows: Vec<(u32, u32, u64)> = ps
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace().map(str::parse::<u64>);
            Some((fields.next()?.ok()? as u32, fields.next()?.ok()? as u32, fields.next()?.ok()?))
        })
        .collect();
    let mut tree = vec![root];
    let mut i = 0;
    while i < tree.len() {
        let parent = tree[i];
        tree.extend(rows.iter().filter(|(pid, ppid, _)| *ppid == parent && !tree.contains(pid)).map(|(pid, _, _)| *pid).collect::<Vec<_>>());
        i += 1;
    }
    rows.iter().filter(|(pid, _, _)| tree.contains(pid)).map(|(_, _, kb)| kb * 1024).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_by_share_of_the_memory() {
        let total = 24_000_000_000;
        assert_eq!(level(3_200_000_000, total), Level::Green);
        assert_eq!(level(6_000_000_000, total), Level::Yellow);
        assert_eq!(level(12_000_000_000, total), Level::Red);
        assert_eq!(text(3_244_566_772), "3.2 GB");
        assert_eq!(text(512_000_000), "512 MB");
    }

    #[cfg(unix)]
    #[test]
    fn sums_the_server_and_its_children() {
        let ps = "  1     0  100\n 10     1    4\n 11    10 2000\n 12    11   30\n 13     1  900\nnot a row\n";
        assert_eq!(tree_rss(10, ps), (4 + 2000 + 30) * 1024);
        assert_eq!(tree_rss(99, ps), 0);
    }
}
