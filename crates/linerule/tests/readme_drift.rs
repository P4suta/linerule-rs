//! README drift gate.
//!
//! The user-facing `README.md` lists the default hotkey chords inside
//! a `<!-- BEGIN GENERATED: hotkeys -->` ... `<!-- END GENERATED:
//! hotkeys -->` fence. This test extracts that fence, parses it as a
//! markdown table, and compares the `(chord, description)` pairs
//! against `HotkeyMap::default()` + a fixed in-test description map.
//!
//! The comparison is *content-only* (whitespace within cells is
//! collapsed) so that a markdown formatter re-aligning the table
//! columns does not register as drift — the only thing the test
//! cares about is that every default chord is documented and that
//! no documented chord ghosts the actual binding.
//!
//! When this test fails, the diff in the assertion message lists the
//! `(chord, description)` pairs that should be in the README.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use linerule_config::HotkeyMap;

const HOTKEY_LABELS: &[(&str, &str)] = &[
    ("cycle_mode", "4 モード(+ なし)を順に切り替え"),
    ("pause", "一時的に **完全 OFF**(もう一度押すと元に戻る)"),
    ("thicker", "帯を太くする"),
    ("thinner", "帯を細くする"),
    ("more_opaque", "濃くする"),
    ("less_opaque", "薄くする"),
    ("quit", "linerule を終了する(緊急脱出用 — 必ず効きます)"),
];

fn readme_path() -> PathBuf {
    // CARGO_MANIFEST_DIR is `crates/linerule`; README is at the workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md")
}

fn extract_fence<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let begin = format!("<!-- BEGIN GENERATED: {name} -->");
    let end = format!("<!-- END GENERATED: {name} -->");
    let start = body.find(&begin)? + begin.len();
    let stop = body[start..].find(&end)?;
    Some(body[start..start + stop].trim_matches('\n'))
}

/// Parse a markdown pipe-table into a list of `(col0, col1)` cells
/// with whitespace collapsed inside each cell. Drops the header row,
/// the `| --- | --- |` separator, and any blank lines.
fn parse_two_col_table(fence: &str) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::new();
    for line in fence.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 2 {
            continue;
        }
        // skip the `--- | ---` separator: a cell that is only `-` chars.
        if cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-'))
        {
            continue;
        }
        rows.push((collapse_ws(cells[0]), collapse_ws(cells[1])));
    }
    if !rows.is_empty() {
        // Drop the header row.
        rows.remove(0);
    }
    rows
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn chord_for(map: &HotkeyMap, key: &str) -> String {
    match key {
        "cycle_mode" => map.cycle_mode.clone(),
        "pause" => map.pause.clone(),
        "thicker" => map.thicker.clone(),
        "thinner" => map.thinner.clone(),
        "more_opaque" => map.more_opaque.clone(),
        "less_opaque" => map.less_opaque.clone(),
        "quit" => map.quit.clone(),
        other => panic!("unknown hotkey label key {other:?} — keep HOTKEY_LABELS in sync"),
    }
}

#[test]
fn readme_hotkey_table_matches_hotkey_map_default() {
    let body = fs::read_to_string(readme_path()).expect("README.md must exist");
    let fence = extract_fence(&body, "hotkeys")
        .expect("README must contain a `<!-- BEGIN GENERATED: hotkeys -->` fence");

    let actual_rows = parse_two_col_table(fence);
    let actual: BTreeMap<String, String> = actual_rows.into_iter().collect();

    let map = HotkeyMap::default();
    let expected: BTreeMap<String, String> = HOTKEY_LABELS
        .iter()
        .map(|(key, desc)| (chord_for(&map, key), collapse_ws(desc)))
        .collect();

    assert_eq!(
        actual, expected,
        "\nREADME hotkey table is out of sync with HotkeyMap::default().\n\
         expected (chord, desc) pairs (whitespace collapsed):\n{expected:#?}\n\
         got (after parsing the fenced markdown):\n{actual:#?}\n",
    );
}

#[test]
fn readme_mentions_every_default_chord() {
    // Belt-and-braces: even if someone renames the fence away, every
    // chord listed in HotkeyMap::default() must appear *somewhere* in
    // the README so users can search for the binding they typed.
    let body = fs::read_to_string(readme_path()).expect("README.md must exist");
    let map = HotkeyMap::default();
    for chord in [
        &map.cycle_mode,
        &map.pause,
        &map.thicker,
        &map.thinner,
        &map.more_opaque,
        &map.less_opaque,
        &map.quit,
    ] {
        assert!(
            body.contains(chord.as_str()),
            "README must mention chord {chord:?} so users can search for it",
        );
    }
}
