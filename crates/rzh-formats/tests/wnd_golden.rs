// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Rust port Copyright 2026 the RustZeroHour contributors.
//
//! Differential parity tests for the WND layout port (P0).
//!
//! `tests/golden/wnd.txt` is a script produced by
//! `tools/differential/gen_wnd_golden.cpp` — a verbatim transcription of
//! the Zero Hour window-layout load path
//! (`GameWindowManagerScript.cpp`'s `winCreateFromScript` walk with the
//! RAMFile scan primitives, the collapsing `readUntilSemicolon` read, the
//! strtok-based field parsers, the window stack, the creation routing and
//! the obsolete DATA field) over 57 hand-built corpora: the version and
//! layout-block families, the full-field USER window, nested and
//! over-deep children, the resolution-scaling family, the flag families,
//! the positional TEXTCOLOR and draw-data walks, the default-color
//! mid-file and in-child-block shapes, every gadget type's data block and
//! composite draw-data routing (the stale-routing second listbox
//! included), the obsolete DATA field family, the too-long labels, the
//! failure cascades, the double-CHILD orphan (with its duplicate id), the
//! dropped-unknown-token shifts at all three levels, and the
//! hang/crash/oob verdict sites.
//!
//! The test replays every corpus through [`rzh_formats::wnd::parse_wnd`]
//! at the same display resolution and re-dumps the layout in the golden
//! script's grammar, asserting line-by-line equality.

use std::path::PathBuf;

use rzh_formats::wnd::{
    WndDisplay, WndDrawState, WndGadget, WndRoot, WndSubSlot, WndVerdict, parse_wnd,
};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("wnd.txt")
}

// --- the dump helpers (the generator's grammar) -----------------------------

fn escape(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &c in bytes {
        match c {
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(c as char),
            _ => out.push_str(&format!("\\x{c:02x}")),
        }
    }
    out
}

fn esc(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        "-".to_string()
    } else {
        escape(bytes)
    }
}

fn img_name(image: &[u8]) -> String {
    if image.is_empty() {
        "NoImage".to_string()
    } else {
        escape(image)
    }
}

fn booln(b: bool) -> i32 {
    i32::from(b)
}

fn csv(values: &[i32]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn gadget_line(g: &WndGadget) -> String {
    match g {
        WndGadget::None => "none".to_string(),
        WndGadget::Radio(r) => format!("radio {} {}", r.group, r.screen),
        WndGadget::Slider(s) => format!("slider {} {}", s.min_val, s.max_val),
        WndGadget::Listbox(l) => {
            let colw = match &l.column_width_percentage {
                Some(p) => csv(p),
                None => "-".to_string(),
            };
            format!(
                "listbox {} {} {} {} {} {} {} {} {}",
                l.list_length,
                booln(l.auto_scroll),
                booln(l.scroll_if_at_end),
                booln(l.auto_purge),
                booln(l.scroll_bar),
                booln(l.multi_select),
                l.columns,
                colw,
                booln(l.force_select)
            )
        }
        WndGadget::ComboBox(c) => format!(
            "combo {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            booln(c.is_editable),
            c.max_chars,
            c.max_display,
            booln(c.ascii_only),
            booln(c.letters_and_numbers_only),
            c.entry_data.max_text_len,
            booln(c.entry_data.secret_text),
            booln(c.entry_data.numerical_only),
            booln(c.entry_data.alpha_numerical_only),
            booln(c.entry_data.ascii_only),
            c.listbox_data.list_length,
            booln(c.listbox_data.auto_scroll),
            booln(c.listbox_data.scroll_bar),
            booln(c.listbox_data.force_select),
            c.listbox_data.columns,
            c.entry_count
        ),
        WndGadget::Entry(e) => format!(
            "entry {} {} {} {} {}",
            e.max_text_len,
            booln(e.secret_text),
            booln(e.numerical_only),
            booln(e.alpha_numerical_only),
            booln(e.ascii_only)
        ),
        WndGadget::StaticText(t) => format!(
            "static {} {} {} {}",
            booln(t.centered),
            booln(t.centered_vertically),
            t.left_margin,
            t.top_margin
        ),
        WndGadget::Tab(t) => {
            let panes: Vec<i32> = t.sub_pane_disabled.iter().map(|&b| booln(b)).collect();
            // the generator dumps all eight slots: the fixup's children
            // then the zero-initialized tail (the original's null slots)
            let mut panes_all: Vec<i32> = match &t.sub_panes {
                Some(p) => p.iter().map(|v| *v as i32).collect(),
                None => Vec::new(),
            };
            panes_all.resize(8, 0);
            let subpanes = panes_all
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "tab {} {} {} {} {} {} {} {}",
                t.tab_orientation,
                t.tab_edge,
                t.tab_width,
                t.tab_height,
                t.tab_count,
                t.pane_border,
                csv(&panes),
                subpanes
            )
        }
    }
}

/// The recursive window dump (the generator's `dumpWindowTree`).
fn dump_tree(
    layout: &rzh_formats::wnd::WndLayout,
    c: &str,
    idx: usize,
    depth: usize,
    out: &mut String,
) {
    let w: &rzh_formats::wnd::WndWindow = &layout.arena[idx];
    let (sx, sy) = layout.screen_position(idx);
    let parent = w.parent.map(|p| p as i32).unwrap_or(-1);
    let owner = w.owner.map(|p| p as i32).unwrap_or(-1);
    out.push_str(&format!(
        "win {c} {idx} {parent} {depth} {} {} {owner} {}\n",
        w.window_type.word(),
        w.id,
        w.children.len()
    ));
    out.push_str(&format!(
        "rect {c} {idx} {} {} {} {} {sx} {sy}\n",
        w.x, w.y, w.width, w.height
    ));
    out.push_str(&format!(
        "stat {c} {idx} {:08x} {:08x}\n",
        w.status, w.style
    ));
    out.push_str(&format!("name {c} {idx} {}\n", esc(&w.name)));
    out.push_str(&format!(
        "cb {c} {idx} {} {} {} {}\n",
        esc(&w.system_callback),
        esc(&w.input_callback),
        esc(&w.tooltip_callback),
        esc(&w.draw_callback)
    ));
    match &w.font {
        Some(f) => out.push_str(&format!(
            "font {c} {idx} {} {} {}\n",
            escape(&f.name),
            f.size,
            f.bold
        )),
        None => out.push_str(&format!("font {c} {idx} -\n")),
    }
    out.push_str(&format!("ht {c} {idx} {}\n", esc(&w.header_template)));
    out.push_str(&format!(
        "label {c} {idx} {} {} {}\n",
        esc(&w.text_label),
        esc(&w.tooltip_label),
        w.tooltip_delay
    ));
    out.push_str(&format!(
        "imgoff {c} {idx} {} {}\n",
        w.image_offset.0, w.image_offset.1
    ));
    out.push_str(&format!(
        "tcol {c} {idx} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}\n",
        w.text_colors.enabled.0,
        w.text_colors.enabled.1,
        w.text_colors.disabled.0,
        w.text_colors.disabled.1,
        w.text_colors.hilite.0,
        w.text_colors.hilite.1
    ));
    for (arr, token) in [
        (&w.enabled_draw_data, "ENABLEDDRAWDATA"),
        (&w.disabled_draw_data, "DISABLEDDRAWDATA"),
        (&w.hilite_draw_data, "HILITEDRAWDATA"),
    ] {
        for (i, d) in arr.iter().enumerate() {
            out.push_str(&format!(
                "ddi {c} {idx} {token} {i} {} {:08x} {:08x}\n",
                img_name(&d.image),
                d.color,
                d.border_color
            ));
        }
    }
    out.push_str(&format!("gad {c} {idx} {}\n", gadget_line(&w.gadget)));
    for sub in &w.sub_draw {
        out.push_str(&format!("subdd {c} {idx} {}\n", sub.slot.word()));
        for (state, arr) in [
            (WndDrawState::Enabled, &sub.enabled),
            (WndDrawState::Disabled, &sub.disabled),
            (WndDrawState::Hilite, &sub.hilite),
        ] {
            let token = sub.slot.field_token(state);
            for (i, d) in arr.iter().enumerate() {
                out.push_str(&format!(
                    "ddi {c} {idx} {token} {i} {} {:08x} {:08x}\n",
                    img_name(&d.image),
                    d.color,
                    d.border_color
                ));
            }
        }
    }
    let children = w.children.clone();
    for ch in children {
        dump_tree(layout, c, ch, depth + 1, out);
    }
}

/// Re-dump a parsed layout in the generator's grammar.
fn dump(layout: &rzh_formats::wnd::WndLayout, corpus: &str) -> String {
    let mut out = String::new();
    let c = corpus;
    out.push_str(&format!("verdict {c} {}\n", layout.verdict.word()));
    out.push_str(&format!("ver {c} {}\n", layout.version));
    out.push_str(&format!(
        "layout {c} {} {} {}\n",
        esc(&layout.init_name),
        esc(&layout.update_name),
        esc(&layout.shutdown_name)
    ));
    out.push_str(&format!(
        "defcol {c} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}\n",
        layout.defaults.enabled_color,
        layout.defaults.disabled_color,
        layout.defaults.background_color,
        layout.defaults.hilite_color,
        layout.defaults.selected_color,
        layout.defaults.text_color
    ));
    match &layout.defaults.font_text {
        Some(t) => out.push_str(&format!("deffont {c} {}\n", escape(t))),
        None => out.push_str(&format!("deffont {c} none\n")),
    }
    out.push_str(&format!(
        "firstwin {c} {}\n",
        layout.first_window_root().map(|i| i as i32).unwrap_or(-1)
    ));
    out.push_str(&format!("roots {c} {}\n", layout.roots.len()));
    for (i, r) in layout.roots.iter().enumerate() {
        out.push_str(&format!(
            "root {c} {i} {}\n",
            if matches!(r, WndRoot::Window(_)) {
                "ok"
            } else {
                "failed"
            }
        ));
    }

    // the window tree dump (the recursive walker)
    let mut tree = String::new();
    for r in &layout.roots {
        if let WndRoot::Window(idx) = r {
            dump_tree(layout, c, *idx, 0, &mut tree);
        }
    }
    out.push_str(&tree);

    // the orphans (only the subtree roots — an orphan whose parent is
    // itself orphaned is covered by that parent's dump)
    let mut reachable: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = layout
        .roots
        .iter()
        .filter_map(|r| match r {
            WndRoot::Window(i) => Some(*i),
            _ => None,
        })
        .collect();
    while let Some(i) = stack.pop() {
        reachable.push(i);
        let children = layout.arena[i].children.clone();
        for ch in children {
            stack.push(ch);
        }
    }
    let orphans: Vec<usize> = (0..layout.arena.len())
        .filter(|i| !reachable.contains(i))
        .collect();
    if !orphans.is_empty() {
        out.push_str(&format!("orphans {c} {}\n", orphans.len()));
        let mut orphan_tree = String::new();
        for &i in &orphans {
            let parent_orphaned = layout.arena[i].parent.is_some_and(|p| orphans.contains(&p));
            if !parent_orphaned {
                dump_tree(layout, c, i, 0, &mut orphan_tree);
            }
        }
        out.push_str(&orphan_tree);
    }

    out.push_str(&format!(
        "namekeys {c} {}\n",
        layout.name_keys.names().count()
    ));
    for (id, name) in layout.name_keys.names() {
        out.push_str(&format!("nk {c} {id} {}\n", escape(name)));
    }
    out.push_str(&format!(
        "stream {c} {} {}\n",
        layout.stream_pos, layout.stream_len
    ));
    out
}

// --- the script reader --------------------------------------------------------

fn unhex(text: &str) -> Vec<u8> {
    if text == "-" {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).expect("hex digit");
        let lo = (pair[1] as char).to_digit(16).expect("hex digit");
        out.push((hi * 16 + lo) as u8);
    }
    out
}

#[test]
fn wnd_golden_replay() {
    let raw = std::fs::read_to_string(golden_path()).expect("the golden script exists");
    let lines = raw.lines().filter(|l| !l.starts_with('#')).peekable();

    let mut corpora = Vec::new();
    // collect (name, display, text, expected-dump-lines)
    let mut current: Option<(String, WndDisplay, Vec<u8>, Vec<String>)> = None;
    for line in lines {
        let mut parts = line.splitn(3, ' ');
        let cmd = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("").to_string();
        match cmd {
            "file" => {
                if let Some(cur) = current.take() {
                    corpora.push(cur);
                }
                let hex = parts.next().unwrap_or("").trim();
                current = Some((name, WndDisplay::new(800, 600), unhex(hex), Vec::new()));
            }
            "disp" => {
                // disp <name> <w> <h> — the display for the corpus
                let rest = parts.next().unwrap_or("").trim();
                let mut it = rest.split_whitespace();
                let w: i32 = it.next().unwrap().parse().unwrap();
                let h: i32 = it.next().unwrap().parse().unwrap();
                if let Some((_, disp, _, _)) = &mut current {
                    *disp = WndDisplay::new(w, h);
                }
            }
            _ => {
                if let Some((_, _, _, expected)) = &mut current {
                    expected.push(line.to_string());
                }
            }
        }
    }
    if let Some(cur) = current.take() {
        corpora.push(cur);
    }

    assert!(
        corpora.len() >= 50,
        "expected the full corpus set, found {}",
        corpora.len()
    );

    let mut failures = Vec::new();
    for (name, display, text, expected) in &corpora {
        let layout = parse_wnd(text, *display);
        let got = dump(&layout, name);
        let got_lines: Vec<&str> = got.lines().collect();
        let exp_lines: Vec<&str> = expected.iter().map(|s| s.as_str()).collect();
        if got_lines != exp_lines {
            // find the first differing line for the report
            let mut diff_at = 0;
            for (i, (g, e)) in got_lines.iter().zip(exp_lines.iter()).enumerate() {
                if g != e {
                    diff_at = i;
                    break;
                }
            }
            if got_lines.len() != exp_lines.len() && diff_at >= got_lines.len().min(exp_lines.len())
            {
                diff_at = got_lines.len().min(exp_lines.len());
            }
            failures.push(format!(
                "corpus {name}: line {} differs\n  golden: {}\n  port:   {}",
                diff_at + 1,
                exp_lines.get(diff_at).unwrap_or(&"<missing>"),
                got_lines.get(diff_at).unwrap_or(&"<missing>")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} corpora diverged:\n{}",
        failures.len(),
        corpora.len(),
        failures.join("\n")
    );
}

#[test]
fn wnd_verdict_words() {
    // the stop verdicts are the words the golden script pins
    assert_eq!(WndVerdict::Ok.word(), "ok");
    assert_eq!(WndVerdict::LoadFailed.word(), "load-failed");
    assert_eq!(
        WndVerdict::Hang("readuntil-eof").word(),
        "hang:readuntil-eof"
    );
    assert_eq!(
        WndVerdict::Crash("drawdata-image").word(),
        "crash:drawdata-image"
    );
    assert_eq!(
        WndVerdict::Oob("subpane-disabled-write").word(),
        "oob:subpane-disabled-write"
    );
}

#[test]
fn wnd_orphan_and_duplicate_id() {
    // the double-CHILD corpus: the second creation orphans the first, and
    // both windows carry the same id (the instance data is not re-parsed
    // between them)
    let text = b"FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n\
WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             BOTTOMRIGHT: 200 200,\n\
             CREATIONRESOLUTION: 800 600;\nNAME = \"C2.wnd:Orphan\";\n\
CHILD\nWINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 5 5,\n             \
BOTTOMRIGHT: 25 25,\n             CREATIONRESOLUTION: 800 600;\nNAME = \"C2.wnd:OrphanKid\";\n\
END\nENDALLCHILDREN\n\
CHILD\nWINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 10 10,\n             \
BOTTOMRIGHT: 30 30,\n             CREATIONRESOLUTION: 800 600;\nNAME = \"C2.wnd:Kept\";\n\
END\nENDALLCHILDREN\nEND\n";
    let layout = parse_wnd(text, WndDisplay::new(800, 600));
    assert!(layout.loaded());
    assert_eq!(layout.roots.len(), 1);
    let WndRoot::Window(root) = layout.roots[0] else {
        panic!("the root parsed");
    };
    // the orphaned first creation and its child exist in the arena
    assert_eq!(layout.arena.len(), 4);
    let orphan = &layout.arena[0];
    assert_eq!(orphan.name, b"C2.wnd:Orphan");
    // the second creation (the returned root) re-snapshotted the instance
    // data BEFORE the Kept window's NAME parsed — it carries the root's
    // name and id, not Kept's
    let second = &layout.arena[root];
    assert_eq!(second.name, b"C2.wnd:Orphan");
    assert_eq!(second.id, orphan.id);
    assert_eq!(second.id, 1);
    // the Kept window is the second creation's child, with its own id
    assert_eq!(second.children, vec![3]);
    assert_eq!(layout.arena[3].name, b"C2.wnd:Kept");
    assert_eq!(layout.arena[3].id, 3);
}

#[test]
fn wnd_stale_composite_routing() {
    // a listbox without its own composite fields inherits the previous
    // listbox's arrays (the original's load-global statics)
    let stanza = |token: &str| -> String {
        let mut s = format!("{token} = IMAGE: NoImage, COLOR: 1 2 3 4, BORDERCOLOR: 5 6 7 8,\n");
        for _ in 1..8 {
            s.push_str("             IMAGE: NoImage, COLOR: 1 2 3 4, BORDERCOLOR: 5 6 7 8,\n");
        }
        s.push_str("             IMAGE: NoImage, COLOR: 1 2 3 4, BORDERCOLOR: 5 6 7 8;\n");
        s
    };
    let mut text = String::from("FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n");
    text.push_str("WINDOW\nWINDOWTYPE = SCROLLLISTBOX;\nSCREENRECT = UPPERLEFT: 10 10,\n             BOTTOMRIGHT: 210 310,\n             CREATIONRESOLUTION: 800 600;\nNAME = \"Lb.wnd:Main\";\n");
    text.push_str(&stanza("LISTBOXENABLEDUPBUTTONDRAWDATA"));
    text.push_str("END\n");
    text.push_str("WINDOW\nWINDOWTYPE = SCROLLLISTBOX;\nSCREENRECT = UPPERLEFT: 10 320,\n             BOTTOMRIGHT: 210 420,\n             CREATIONRESOLUTION: 800 600;\nNAME = \"Lb.wnd:Bare\";\nEND\n");
    let layout = parse_wnd(text.as_bytes(), WndDisplay::new(800, 600));
    let WndRoot::Window(a) = layout.roots[0] else {
        panic!("root a");
    };
    let WndRoot::Window(b) = layout.roots[1] else {
        panic!("root b");
    };
    let slots = |w: usize| -> Vec<String> {
        layout.arena[w]
            .sub_draw
            .iter()
            .map(|s| s.slot.word().to_string())
            .collect()
    };
    assert_eq!(slots(a), ["upbtn", "downbtn", "slider", "thumb"]);
    assert_eq!(slots(b), ["upbtn", "downbtn", "slider", "thumb"]);
    // both listboxes' up-button arrays agree — the bare one inherited them
    let up_a = &layout.arena[a].sub_draw[0];
    let up_b = &layout.arena[b].sub_draw[0];
    assert_eq!(up_a.slot, WndSubSlot::UpButton);
    // "COLOR: 1 2 3 4" packs as (a=4, r=1, g=2, b=3)
    assert_eq!(up_a.enabled[0].color, 0x0401_0203);
    assert_eq!(up_a.enabled[0].border_color, 0x0805_0607);
    assert_eq!(up_b.enabled[0].color, 0x0401_0203);
    assert_eq!(up_b.enabled[0].border_color, 0x0805_0607);
}
