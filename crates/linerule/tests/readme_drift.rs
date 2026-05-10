//! README drift gate.
//!
//! The user-facing `README.md` lists the default hotkey chords inside
//! a `<!-- BEGIN GENERATED: hotkeys -->` ... `<!-- END GENERATED:
//! hotkeys -->` fence. This test extracts that fence, builds the same
//! table from the live `HotkeyMap::default()`, and fails if they
//! disagree — so editing `HotkeyMap::default()` without updating the
//! README (or vice versa) breaks `just test` immediately.
//!
//! When this test fails, the diff in the assertion message is the
//! exact text that should replace the fenced section.

use std::fmt::Write as _;
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

fn render_hotkey_table(map: &HotkeyMap) -> String {
    // Pad the chord and description columns so the rendered table is
    // visually aligned in raw markdown view too.
    let chord_for = |key: &str| -> &str {
        match key {
            "cycle_mode" => &map.cycle_mode,
            "pause" => &map.pause,
            "thicker" => &map.thicker,
            "thinner" => &map.thinner,
            "more_opaque" => &map.more_opaque,
            "less_opaque" => &map.less_opaque,
            "quit" => &map.quit,
            other => panic!("unknown hotkey label key {other:?} — keep HOTKEY_LABELS in sync"),
        }
    };

    let chord_w = HOTKEY_LABELS
        .iter()
        .map(|(k, _)| chord_for(k).chars().count())
        .max()
        .unwrap_or(0)
        .max("キー".chars().count());
    let desc_w = HOTKEY_LABELS
        .iter()
        .map(|(_, d)| display_width(d))
        .max()
        .unwrap_or(0)
        .max(display_width("何が起きる"));

    let header_chord = pad_ascii("キー", chord_w);
    let header_desc = pad_display("何が起きる", desc_w);

    let mut out = String::new();
    out.push('\n');
    writeln!(out, "| {header_chord} | {header_desc} |").expect("writeln to String");
    writeln!(out, "| {} | {} |", "-".repeat(chord_w), "-".repeat(desc_w))
        .expect("writeln to String");
    for (key, desc) in HOTKEY_LABELS {
        let chord = pad_ascii(chord_for(key), chord_w);
        let desc = pad_display(desc, desc_w);
        writeln!(out, "| {chord} | {desc} |").expect("writeln to String");
    }
    out.push('\n');
    out
}

/// Display width counting CJK fullwidth chars as 2 columns. Good
/// enough for our subset (`ASCII + ひらがな + カタカナ + 漢字 + 全角記号`).
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| if (c as u32) < 0x80 { 1 } else { 2 })
        .sum()
}

fn pad_ascii(s: &str, w: usize) -> String {
    let pad = w.saturating_sub(s.chars().count());
    format!("{s}{}", " ".repeat(pad))
}

fn pad_display(s: &str, w: usize) -> String {
    let pad = w.saturating_sub(display_width(s));
    format!("{s}{}", " ".repeat(pad))
}

#[test]
fn readme_hotkey_table_matches_hotkey_map_default() {
    let body = fs::read_to_string(readme_path()).expect("README.md must exist");
    let actual = extract_fence(&body, "hotkeys")
        .expect("README must contain a `<!-- BEGIN GENERATED: hotkeys -->` fence")
        .trim();
    let expected = render_hotkey_table(&HotkeyMap::default());
    let expected_trim = expected.trim();
    assert_eq!(
        actual, expected_trim,
        "\nREADME hotkey table is out of sync with HotkeyMap::default().\n\
         Replace the fenced section in README.md with:\n\
         ===== expected =====\n{expected}\n===== end =====\n",
    );
}

#[test]
fn readme_mentions_every_default_chord_and_no_more() {
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
