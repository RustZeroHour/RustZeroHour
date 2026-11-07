// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// Ported from the Zero Hour source release (baseline 571a7c5):
//   Core/GameEngine/Source/GameClient/GUI/GameWindowManagerScript.cpp
//     (GameWindowManager::winCreateFromScript — the whole .wnd load path:
//     the version read, the version-2 layout block with
//     parseInit/parseUpdate/parseShutdown, the default color/font blocks,
//     the WINDOW loop, parseWindow with the WINDOWTYPE/SCREENRECT head,
//     the gameWindowFieldList walk and the DATA/END/CHILD routing,
//     parseChildWindows with the window stack and the tab-control
//     sub-pane fixup, createWindow/createGadget with their style-bit
//     routing, the radio-button screen id and the composite-gadget draw
//     data routing, getDataTemplate, parseData — the obsolete DATA field
//     — and every field parser: parseName, parseStatus, parseStyle, the
//     four callback parsers, parseFont, parseHeaderTemplate,
//     parseListboxData, parseComboBoxData, parseSliderData,
//     parseRadioButtonData, parseTooltipText, parseTooltipDelay, parseText,
//     parseTextColor, parseStaticTextData, parseTextEntryData,
//     parseTabControlData, parseDrawData, parseImageOffset, parseTooltip,
//     parseDefaultColor, parseDefaultFont, parseColor, parseBitString,
//     parseBitFlag, readUntilSemicolon and the scan* wrappers)
//   Core/GameEngine/Source/Common/System/RAMFile.cpp
//     (RAMFile::read, scanInt, scanString, nextLine — the tokenizing
//     primitives the walk drives)
//   Core/GameEngine/Source/GameClient/GUI/WinInstanceData.cpp
//     (WinInstanceData::init — the per-window defaults, the
//     WIN_COLOR_UNDEFINED sentinels included)
//   Core/GameEngine/Source/GameClient/GUI/GameWindow.cpp
//     (GameWindow::winGetScreenPosition — the ancestor-summing walk the
//     parent-relative SCREENRECT adjustment resolves through)
//   Core/GameEngine/Source/GameClient/GUI/GameWindowGlobal.cpp +
//   Core/GameEngine/Include/GameClient/Color.h
//     (winMakeColor / GameMakeColor — the ARGB pack)
//   Core/GameEngine/Include/GameClient/GameWindow.h
//     (the WIN_STATUS_* flag values, WIN_COLOR_UNDEFINED)
//   Core/GameEngine/Include/GameClient/Gadget.h
//     (the GWS_* style values, the gadget data structs, NUM_TAB_PANES)
//   Core/GameEngine/Include/GameClient/WinInstanceData.h
//     (WinDrawData, TextDrawData, MAX_DRAW_DATA = 9, MAX_TEXT_LABEL = 128,
//     MAX_WINDOW_NAME_LEN = 64)
//   Core/GameEngine/Include/GameClient/GameWindowManager.h
//     (WindowLayoutInfo, WindowLayoutCurrentVersion = 2)
//   Core/GameEngine/Source/Common/NameKeyGenerator.cpp
//     (nameToKey — the first-seen sequential interning the window ids and
//     the radio screen ids mint through)
//
// See the module docs for the format layout and the documented
// differences from the C++ (the environment singletons as explicit
// state; the per-load statics; the hang/crash/oob verdicts).

//! The **WND window layouts** — the `.wnd` definition files the game's
//! retained-mode GUI loads, the format GUIEdit writes.
//!
//! A `.wnd` file is a *text* script: a version word, a layout block
//! (from version 2) naming the layout's init/update/shutdown callbacks,
//! the file-scoped default colors and font, and a run of `WINDOW`
//! blocks — each a `WINDOWTYPE`, a `SCREENRECT` scaled from its creation
//! resolution to the current one, and a field list (name, status and
//! style flag strings, callbacks, font, text colors, three draw-data
//! arrays, the per-gadget data blocks), with nested `CHILD` blocks
//! forming the window tree. The reader is
//! `GameWindowManager::winCreateFromScript`; this module ports its whole
//! load path over the [`RAMFile`](#) scan primitives, token-walk quirks
//! included (parity level P0, pinned by golden vectors generated from
//! the transcription in `tools/differential/gen_wnd_golden.cpp`).
//!
//! # The format in one page
//!
//! ```text
//! FILE_VERSION = 2;                    the version word (2 = the editor's)
//! STARTLAYOUTBLOCK
//!   LAYOUTINIT = name;                 the layout callbacks (v2+)
//!   LAYOUTUPDATE = name;
//!   LAYOUTSHUTDOWN = name;
//! ENDLAYOUTBLOCK
//! ENABLEDCOLOR = r g b;                the file-scoped defaults
//! FONT = NAME: "x", SIZE: n, BOLD: n;  (parsed and DISCARDED — a no-op)
//! WINDOW
//!   WINDOWTYPE = USER|TABPANE|PUSHBUTTON|RADIOBUTTON|CHECKBOX|TABCONTROL|
//!                VERTSLIDER|HORZSLIDER|SCROLLLISTBOX|COMBOBOX|ENTRYFIELD|
//!                STATICTEXT|PROGRESSBAR;
//!   SCREENRECT = UPPERLEFT: x y, BOTTOMRIGHT: x y,
//!                CREATIONRESOLUTION: w h;
//!   NAME = "file.wnd:control";         the id is the interned name key
//!   STATUS = ENABLED+DRAGABLE;         '+'-joined flag names
//!   STYLE = MOUSETRACK+ANIMATED;
//!   SYSTEMCALLBACK|INPUTCALLBACK|TOOLTIPCALLBACK|DRAWCALLBACK = "name";
//!   FONT = NAME: "face", SIZE: n, BOLD: n;
//!   HEADERTEMPLATE = "name";
//!   TOOLTIPTEXT = "label";  TOOLTIPDELAY = n;  TOOLTIP;
//!   TEXT = "label";
//!   TEXTCOLOR = ENABLED: r g b a, ENABLEDBORDER: ..., DISABLED: ...,
//!               HILITE: ...;            six positional records
//!   ENABLEDDRAWDATA = IMAGE: name, COLOR: r g b a, BORDERCOLOR: ...
//!              (nine triplets, one stanza; the collapsing read joins
//!               the writer's nine lines into one)
//!   DISABLEDDRAWDATA = ...;  HILITEDRAWDATA = ...;
//!   <gadget data block: LISTBOXDATA, COMBOBOXDATA, SLIDERDATA,
//!    RADIOBUTTONDATA, STATICTEXTDATA, TEXTENTRYDATA, TABCONTROLDATA>
//!   <LISTBOX*/COMBOBOX*/SLIDERTHUMB*DRAWDATA — the composite sub-gadget
//!    arrays, routed into the constructed sub-windows>
//!   IMAGEOFFSET = x y;
//!   DATA = ...;                        the obsolete pre-editor field
//!   CHILD <nested WINDOW blocks> ENDALLCHILDREN
//! END
//! ```
//!
//! # The load-path quirks (transcribed)
//!
//! - **The collapsing read**: `readUntilSemicolon` reads *across* lines
//!   until the `;`, squashing interior whitespace (newlines included) to
//!   single spaces — this is what re-joins the writer's multi-line
//!   SCREENRECT, TEXTCOLOR and draw-data stanzas into the one buffer the
//!   strtok walks run over.
//! - **The eat-'=' is an unchecked `scanString`**: a field whose `=` is
//!   missing has the *next token* eaten instead — a bare `TOOLTIP;`
//!   swallows the following field's name token whole, and that field
//!   never parses (the goldens pin the shift).
//! - **SCREENRECT scales at parse time**: the region is scaled by
//!   current-display ÷ creation-resolution (both axes, `f32` multiply,
//!   truncating cast) and then made **parent-relative** by subtracting
//!   the parent's accumulated screen position — a parent's own scaling
//!   shows up in every child's stored offset.
//! - **The window id is the interned name key**: `NAME` mints an id
//!   through the first-seen sequential generator; an unnamed window
//!   keeps id 0, and a second `NAME` on one window re-interns (the ids
//!   agree only if the names do).
//! - **The flags are `stricmp`-matched '+'-split words** over a 256-byte
//!   truncated copy; `NULL` is matched by its first four bytes only
//!   (`NULLIFY` skips the parse too), unknown flags are dropped, and
//!   `STATUS`/`STYLE` are *assigned*, not OR-ed — a repeat wins.
//! - **The draw data is positional**: nine triplets of IMAGE/COLOR/
//!   BORDERCOLOR, twelve tokens each; the labels are never checked, and
//!   the color components truncate through `int` into bytes (300 wraps
//!   to 44). `NoImage` is the null image; any other name is *requested*
//!   (the resolution against the mapped-image collection is client
//!   state — the name is the parse observable).
//! - **TEXTCOLOR is positional too**: three states (enabled, disabled,
//!   hilite), two records each, the labels discarded — mismatched label
//!   words still parse.
//! - **The gadget type ORs style bits in at creation** (`USER` gains
//!   `GWS_USER_WINDOW`, `TABPANE` `GWS_TAB_PANE`, each gadget its own
//!   bit) — the STYLE field and the type bits combine.
//! - **The radio button's screen id** is the name's `file.wnd:` prefix
//!   (the part before the colon; the whole name when there is none),
//!   interned through the same key generator.
//! - **The composite-gadget draw arrays are load-global running state**:
//!   the LISTBOX*/COMBOBOX*/SLIDERTHUMB* fields overwrite them, and each
//!   slider/listbox/combo creation snapshots them into the window — a
//!   listbox *without* its own composite fields inherits the previous
//!   listbox's values (the goldens pin the stale routing).
//! - **The gadget data templates are shared per load**: every window
//!   starts from a zeroed template, but the obsolete `DATA` field
//!   *replaces* the data pointer whole (a LISTBOXDATA parsed after a
//!   DATA writes into DATA's own static), and static-text/radio `DATA`
//!   read their never-initialized members as zeros forever.
//! - **The window stack is ten deep**: the root is never pushed, so the
//!   refusal lands on the eleventh window of a chain — the refused push
//!   leaves the pop unmatched, and the mismatch cascades the whole root
//!   to a parse failure (a `nullptr` entry in the window list, the load
//!   itself still succeeds if an earlier root parsed).
//! - **A double `CHILD` creates two windows**: the field loop re-creates
//!   at every CHILD, so the second creation orphans the first (still in
//!   the manager's list, outside the layout's roots) — and both carry
//!   the *same* id, the instance data not being re-parsed between them.
//! - **The tab control's sub-pane list is the children reversed**, capped
//!   at [`NUM_TAB_PANES`] (a longer child list writes past the array in
//!   the original).
//! - **Default colors are load state**: a default set mid-file (or
//!   inside a child block) only reaches the windows parsed after it; a
//!   default color inside a *window's* field list is not a field at all
//!   — it is eaten as an unrecognized token, value and all.
//! - **The default `FONT` block is a no-op**: the parsed font is
//!   discarded (the original's `@todo`), so `defFont` stays null and a
//!   window without its own `FONT` field has none.
//! - Unknown tokens at the top level and inside child blocks are dropped
//!   **without consuming their value** (the value tokens become
//!   subsequent tokens); inside a window's field list the value *is*
//!   consumed, to the `;`.
//!
//! # Where the original leaves defined behaviour
//!
//! The walk's release build has no failure mode of its own — a field
//! parser returning `false` (an over-length `TEXT`/`TOOLTIPTEXT` label)
//! fails the window, and a missing layout block fails the load before
//! any window is read. Everything else the port states as a
//! [`WndVerdict`] on the returned layout, recorded where the original
//! stops being defined (the transcription detects each site before
//! acting on it):
//!
//! - `hang:…` — an EOF the original spins on forever: `readUntilSemicolon`
//!   with only leading whitespace seen re-reads the stale whitespace
//!   byte without advancing; the layout-block loop and the window field
//!   loop re-scan the empty EOF token forever.
//! - `crash:…` — a null `strtok` result the original dereferences (a
//!   short field, an empty quoted name, a DATA field with no tokens), a
//!   quote scan walked off the end of the read buffer, a gadget-data
//!   field on a window whose type has no data template, or a `DATA`
//!   field on a tab control/combo box (whose construction dereferences
//!   the null data).
//! - `oob:…` — a write past a fixed array: a `PANEDISABLED` list longer
//!   than eight panes, or a tab control with more than eight children
//!   (the sub-pane fixup).
//!
//! The layout keeps every partial result up to the stop, so the verdict
//! pins the exact site the way a byte-exact dump would.
//!
//! # Differences from the C++ (documented)
//!
//! - **The environment singletons are explicit state**: the current
//!   display resolution is the [`WndDisplay`] parameter (the SCREENRECT
//!   scaling divides by it), the mapped-image collection / font library /
//!   function lexicon / header-template manager / game text are *not
//!   modeled* — the image **name**, the font **request** and the
//!   callback/template/label **strings** are the parse observables (the
//!   resolutions are the later client-layer slices; the fonts are their
//!   own plan item).
//! - **The name-key generator is a fresh table per load** — the same
//!   documented divergence the map slices carry: the original pre-seeds
//!   the process-global generator with the whole well-known-keys table
//!   at startup, so its window ids depend on process history; the
//!   fresh-table contract (first-seen sequential ids from 1) is what the
//!   goldens pin. The table is exposed on the layout as [`NameKeys`]
//!   (shared with [`crate::map_chunk`] — the same interning algorithm,
//!   already golden-pinned there).
//! - **The process-global statics are per-load state**: the 2048-byte
//!   read buffer, the gadget-data templates and the composite draw-data
//!   arrays are reset per load (the original's survive across loads in
//!   one process — the cross-load contamination is out of scope). Within
//!   a load their aliasing is transcribed exactly, buffer stale bytes
//!   included.
//! - **The window arena replaces the manager's window list**: the
//!   created windows are indexed (parent/owner/child links are arena
//!   indices), the roots list keeps the original's nullptr pushes as
//!   [`WndRoot::Failed`] entries, and the orphans of a double-CHILD are
//!   reachable on the arena (dumped by the goldens, ignored by the
//!   original's layout result).
//! - The version word on a failed `scanInt` is read as 0 (the original
//!   leaves it uninitialized — the v1 shape either way).
//! - The gadget *construction* the load path performs (the sub-windows
//!   the composite gadgets build, their positions, the `setWindowText`
//!   translation dispatch, the `GWM_SCRIPT_CREATE` messages) is the
//!   client layer, not the format: the port keeps the style bits, the
//!   gadget data and the routed draw-data snapshots, and states the
//!   construction boundary here.
//!
//! # Examples
//!
//! ```
//! use rzh_formats::wnd::{parse_wnd, WndDisplay};
//!
//! // A one-window layout at its creation resolution.
//! let text = b"FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n\
//! WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 10 20,\n\
//!              BOTTOMRIGHT: 110 220,\n\
//!              CREATIONRESOLUTION: 800 600;\n\
//! NAME = \"Menus.wnd:MainWindow\";\nSTATUS = ENABLED;\nEND\n";
//! let layout = parse_wnd(text, WndDisplay::new(800, 600));
//! assert!(layout.loaded()); // a root window parsed
//! assert_eq!(layout.roots.len(), 1);
//! let win = layout.root_window(0).expect("the root parsed");
//! assert_eq!(win.window_type.word(), "USER");
//! assert_eq!((win.x, win.y, win.width, win.height), (10, 20, 100, 200));
//! assert_eq!(win.name, b"Menus.wnd:MainWindow");
//! assert_eq!(win.status, 0x8); // WIN_STATUS_ENABLED
//! ```

use crate::map_chunk::NameKeys;

// ---------------------------------------------------------------------------
// constants (GameWindowManagerScript.cpp, WinInstanceData.h, Gadget.h,
// GameWindowManager.h)
// ---------------------------------------------------------------------------

/// The persistent read buffer length (`WIN_BUFFER_LENGTH`).
pub const WIN_BUFFER_LENGTH: usize = 2048;

/// The draw-data entries per state (`MAX_DRAW_DATA`).
pub const MAX_DRAW_DATA: usize = 9;

/// The label length bound (`MAX_TEXT_LABEL`).
pub const MAX_TEXT_LABEL: usize = 128;

/// The decorated-name bound (`MAX_WINDOW_NAME_LEN`).
pub const MAX_WINDOW_NAME_LEN: usize = 64;

/// The window-stack depth (`WIN_STACK_DEPTH`).
pub const WIN_STACK_DEPTH: usize = 10;

/// The tab-control pane slots (`NUM_TAB_PANES`).
pub const NUM_TAB_PANES: usize = 8;

/// The undefined-color sentinel (`WIN_COLOR_UNDEFINED` — white with zero
/// alpha).
pub const WIN_COLOR_UNDEFINED: u32 = 0x00FF_FFFF;

/// The editor's current file version (`WindowLayoutCurrentVersion`).
pub const WINDOW_LAYOUT_CURRENT_VERSION: u32 = 2;

/// The window status flag words (`WindowStatusNames[]`, in bit order —
/// the i-th word sets bit i).
pub const WINDOW_STATUS_NAMES: [&[u8]; 26] = [
    b"ACTIVE",
    b"TOGGLE",
    b"DRAGABLE",
    b"ENABLED",
    b"HIDDEN",
    b"ABOVE",
    b"BELOW",
    b"IMAGE",
    b"TABSTOP",
    b"NOINPUT",
    b"NOFOCUS",
    b"DESTROYED",
    b"BORDER",
    b"SMOOTH_TEXT",
    b"ONE_LINE",
    b"NO_FLUSH",
    b"SEE_THRU",
    b"RIGHT_CLICK",
    b"WRAP_CENTERED",
    b"CHECK_LIKE",
    b"HOTKEY_TEXT",
    b"USE_OVERLAY_STATES",
    b"NOT_READY",
    b"FLASHING",
    b"ALWAYS_COLOR",
    b"ON_MOUSE_DOWN",
];

/// The window style flag words (`WindowStyleNames[]`, in bit order).
pub const WINDOW_STYLE_NAMES: [&[u8]; 16] = [
    b"PUSHBUTTON",
    b"RADIOBUTTON",
    b"CHECKBOX",
    b"VERTSLIDER",
    b"HORZSLIDER",
    b"SCROLLLISTBOX",
    b"ENTRYFIELD",
    b"STATICTEXT",
    b"PROGRESSBAR",
    b"USER",
    b"MOUSETRACK",
    b"ANIMATED",
    b"TABSTOP",
    b"TABCONTROL",
    b"TABPANE",
    b"COMBOBOX",
];

/// `GWS_PUSH_BUTTON` — the style bit a PUSHBUTTON window gains at
/// creation.
pub const GWS_PUSH_BUTTON: u32 = 0x0000_0001;
/// `GWS_RADIO_BUTTON`.
pub const GWS_RADIO_BUTTON: u32 = 0x0000_0002;
/// `GWS_CHECK_BOX`.
pub const GWS_CHECK_BOX: u32 = 0x0000_0004;
/// `GWS_VERT_SLIDER`.
pub const GWS_VERT_SLIDER: u32 = 0x0000_0008;
/// `GWS_HORZ_SLIDER`.
pub const GWS_HORZ_SLIDER: u32 = 0x0000_0010;
/// `GWS_SCROLL_LISTBOX`.
pub const GWS_SCROLL_LISTBOX: u32 = 0x0000_0020;
/// `GWS_ENTRY_FIELD`.
pub const GWS_ENTRY_FIELD: u32 = 0x0000_0040;
/// `GWS_STATIC_TEXT`.
pub const GWS_STATIC_TEXT: u32 = 0x0000_0080;
/// `GWS_PROGRESS_BAR`.
pub const GWS_PROGRESS_BAR: u32 = 0x0000_0100;
/// `GWS_USER_WINDOW` — the bit a USER window gains.
pub const GWS_USER_WINDOW: u32 = 0x0000_0200;
/// `GWS_TAB_CONTROL`.
pub const GWS_TAB_CONTROL: u32 = 0x0000_2000;
/// `GWS_TAB_PANE`.
pub const GWS_TAB_PANE: u32 = 0x0000_4000;
/// `GWS_COMBO_BOX`.
pub const GWS_COMBO_BOX: u32 = 0x0000_8000;

/// `GameMakeColor` — the ARGB pack the color readers drive.
#[must_use]
pub fn game_make_color(red: u8, green: u8, blue: u8, alpha: u8) -> u32 {
    (u32::from(alpha) << 24) | (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
}

// ---------------------------------------------------------------------------
// the verdict (the sites where the original leaves defined behaviour)
// ---------------------------------------------------------------------------

/// How the load ended.
///
/// The original's release build never fails mid-walk (a field refusal
/// fails the window, a missing layout block fails the load), so an
/// ordinary load is [`Ok`](Self::Ok) or [`LoadFailed`](Self::LoadFailed).
/// The other variants state the sites where the original spins, crashes
/// or writes out of bounds — recorded at the exact stop, with every
/// partial result kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WndVerdict {
    /// The walk completed and at least one root window parsed
    /// (`WindowLayout::load` succeeds).
    Ok,
    /// The walk completed but no root window parsed — either the layout
    /// block was missing/refused (the load fails before any window is
    /// read) or every root failed.
    LoadFailed,
    /// The original's loop spins forever at an EOF
    /// (`readUntilSemicolon`, the layout-block loop, the window field
    /// loop).
    Hang(&'static str),
    /// The original dereferences a null token or walks a quote scan off
    /// the buffer end.
    Crash(&'static str),
    /// The original writes past a fixed array.
    Oob(&'static str),
}

impl WndVerdict {
    /// The golden script's word for this verdict.
    #[must_use]
    pub fn word(self) -> String {
        match self {
            WndVerdict::Ok => "ok".to_string(),
            WndVerdict::LoadFailed => "load-failed".to_string(),
            WndVerdict::Hang(site) => format!("hang:{site}"),
            WndVerdict::Crash(site) => format!("crash:{site}"),
            WndVerdict::Oob(site) => format!("oob:{site}"),
        }
    }
}

// ---------------------------------------------------------------------------
// the data model (the parse observables)
// ---------------------------------------------------------------------------

/// The current display resolution (`TheDisplay`) the SCREENRECT scaling
/// divides by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WndDisplay {
    /// The display width in pixels.
    pub width: i32,
    /// The display height in pixels.
    pub height: i32,
}

impl WndDisplay {
    /// A display of `width` × `height`.
    #[must_use]
    pub fn new(width: i32, height: i32) -> Self {
        WndDisplay { width, height }
    }
}

/// One draw-data entry (`WinDrawData`): the requested image (the name —
/// the resolution against the mapped-image collection is client state)
/// or the null image, and the two packed colors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WndDrawData {
    /// The requested image name; empty is the null image (`NoImage`).
    pub image: Vec<u8>,
    /// The fill color.
    pub color: u32,
    /// The border color.
    pub border_color: u32,
}

/// The three-state text colors (`TextDrawData` × 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WndTextColors {
    /// The enabled state's color and border.
    pub enabled: (u32, u32),
    /// The disabled state's color and border.
    pub disabled: (u32, u32),
    /// The hilite state's color and border.
    pub hilite: (u32, u32),
}

impl Default for WndTextColors {
    fn default() -> Self {
        WndTextColors {
            enabled: (WIN_COLOR_UNDEFINED, WIN_COLOR_UNDEFINED),
            disabled: (WIN_COLOR_UNDEFINED, WIN_COLOR_UNDEFINED),
            hilite: (WIN_COLOR_UNDEFINED, WIN_COLOR_UNDEFINED),
        }
    }
}

/// A parsed font request (`parseFont`'s `getFont(name, size, bold)` —
/// the resolution is the fonts slice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WndFont {
    /// The requested face name.
    pub name: Vec<u8>,
    /// The requested point size.
    pub size: i32,
    /// The requested bold flag.
    pub bold: i32,
}

/// The window type word (`WINDOWTYPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WndWindowType {
    /// `USER` — a plain window (gains [`GWS_USER_WINDOW`]).
    User,
    /// `TABPANE` — a tab-control pane (gains [`GWS_TAB_PANE`]).
    TabPane,
    /// `PUSHBUTTON`.
    PushButton,
    /// `RADIOBUTTON`.
    RadioButton,
    /// `CHECKBOX`.
    CheckBox,
    /// `TABCONTROL`.
    TabControl,
    /// `VERTSLIDER`.
    VertSlider,
    /// `HORZSLIDER`.
    HorzSlider,
    /// `SCROLLLISTBOX`.
    ScrollListBox,
    /// `COMBOBOX`.
    ComboBox,
    /// `ENTRYFIELD`.
    EntryField,
    /// `STATICTEXT`.
    StaticText,
    /// `PROGRESSBAR`.
    ProgressBar,
}

impl WndWindowType {
    /// The word as it appears in the file.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            WndWindowType::User => "USER",
            WndWindowType::TabPane => "TABPANE",
            WndWindowType::PushButton => "PUSHBUTTON",
            WndWindowType::RadioButton => "RADIOBUTTON",
            WndWindowType::CheckBox => "CHECKBOX",
            WndWindowType::TabControl => "TABCONTROL",
            WndWindowType::VertSlider => "VERTSLIDER",
            WndWindowType::HorzSlider => "HORZSLIDER",
            WndWindowType::ScrollListBox => "SCROLLLISTBOX",
            WndWindowType::ComboBox => "COMBOBOX",
            WndWindowType::EntryField => "ENTRYFIELD",
            WndWindowType::StaticText => "STATICTEXT",
            WndWindowType::ProgressBar => "PROGRESSBAR",
        }
    }

    fn from_word(word: &[u8]) -> Option<Self> {
        Some(match word {
            b"USER" => WndWindowType::User,
            b"TABPANE" => WndWindowType::TabPane,
            b"PUSHBUTTON" => WndWindowType::PushButton,
            b"RADIOBUTTON" => WndWindowType::RadioButton,
            b"CHECKBOX" => WndWindowType::CheckBox,
            b"TABCONTROL" => WndWindowType::TabControl,
            b"VERTSLIDER" => WndWindowType::VertSlider,
            b"HORZSLIDER" => WndWindowType::HorzSlider,
            b"SCROLLLISTBOX" => WndWindowType::ScrollListBox,
            b"COMBOBOX" => WndWindowType::ComboBox,
            b"ENTRYFIELD" => WndWindowType::EntryField,
            b"STATICTEXT" => WndWindowType::StaticText,
            b"PROGRESSBAR" => WndWindowType::ProgressBar,
            _ => return None,
        })
    }
}

/// The slider gadget data (`SliderData`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WndSliderData {
    /// `MINVALUE`.
    pub min_val: i32,
    /// `MAXVALUE`.
    pub max_val: i32,
}

/// The entry-field gadget data (`EntryData`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WndEntryData {
    /// `MAXLEN`.
    pub max_text_len: i16,
    /// `SECRETTEXT`.
    pub secret_text: bool,
    /// `NUMERICALONLY`.
    pub numerical_only: bool,
    /// `ALPHANUMERICALONLY`.
    pub alpha_numerical_only: bool,
    /// `ASCIIONLY`.
    pub ascii_only: bool,
}

/// The static-text gadget data (`TextData`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WndTextData {
    /// `CENTERED`.
    pub centered: bool,
    /// The forced vertical centering the parse writes (`TRUE`).
    pub centered_vertically: bool,
    /// The forced left margin the parse writes (7).
    pub left_margin: i32,
    /// The forced top margin the parse writes (7).
    pub top_margin: i32,
}

/// The listbox gadget data (`ListboxData`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WndListboxData {
    /// `LENGTH`.
    pub list_length: i16,
    /// `COLUMNS`.
    pub columns: i16,
    /// `COLUMNSWIDTH%` — one per column past the first (`None` is the
    /// null array).
    pub column_width_percentage: Option<Vec<i32>>,
    /// `AUTOSCROLL`.
    pub auto_scroll: bool,
    /// `AUTOPURGE`.
    pub auto_purge: bool,
    /// `SCROLLBAR`.
    pub scroll_bar: bool,
    /// `MULTISELECT`.
    pub multi_select: bool,
    /// `FORCESELECT`.
    pub force_select: bool,
    /// `SCROLLIFATEND` (the optional gate; `false` when absent).
    pub scroll_if_at_end: bool,
}

/// The combo-box gadget data (`ComboBoxData`): the five parsed fields
/// plus the constructed halves `createGadget` mints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WndComboBoxData {
    /// `ISEDITABLE`.
    pub is_editable: bool,
    /// `MAXCHARS`.
    pub max_chars: i32,
    /// `MAXDISPLAY`.
    pub max_display: i32,
    /// `ASCIIONLY`.
    pub ascii_only: bool,
    /// `LETTERSANDNUMBERS`.
    pub letters_and_numbers_only: bool,
    /// The constructed entry data (the `NEW EntryData` with the copied
    /// gates and `maxChars`).
    pub entry_data: WndEntryData,
    /// The constructed listbox data (the `NEW ListboxData` with its
    /// fixed starts).
    pub listbox_data: WndListboxData,
    /// The zeroed entry count.
    pub entry_count: i32,
}

/// The radio-button gadget data (`RadioButtonData`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WndRadioButtonData {
    /// The group (`RADIOBUTTONDATA`'s `GROUP`).
    pub group: i32,
    /// The screen id — the interned name prefix before the `:`.
    pub screen: i32,
}

/// The tab-control gadget data (`TabControlData`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WndTabControlData {
    /// `TABORIENTATION`.
    pub tab_orientation: i32,
    /// `TABEDGE`.
    pub tab_edge: i32,
    /// `TABWIDTH`.
    pub tab_width: i32,
    /// `TABHEIGHT`.
    pub tab_height: i32,
    /// `TABCOUNT`.
    pub tab_count: i32,
    /// `PANEBORDER`.
    pub pane_border: i32,
    /// `PANEDISABLED` — the eight pane disable flags.
    pub sub_pane_disabled: [bool; NUM_TAB_PANES],
    /// The sub-pane list the child-block close re-fills: the children in
    /// reverse creation order (capped at [`NUM_TAB_PANES`]; `None` when
    /// the window has no children).
    pub sub_panes: Option<Vec<usize>>,
}

impl Default for WndTabControlData {
    fn default() -> Self {
        WndTabControlData {
            tab_orientation: 0,
            tab_edge: 0,
            tab_width: 0,
            tab_height: 0,
            tab_count: 0,
            pane_border: 0,
            sub_pane_disabled: [false; NUM_TAB_PANES],
            sub_panes: None,
        }
    }
}

/// The gadget data a created window carries (the snapshot the shared
/// template hands the gadget).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WndGadget {
    /// No gadget data (USER, TABPANE, PUSHBUTTON, CHECKBOX, PROGRESSBAR).
    None,
    /// The slider data.
    Slider(WndSliderData),
    /// The listbox data.
    Listbox(WndListboxData),
    /// The combo-box data (with its constructed halves).
    ComboBox(WndComboBoxData),
    /// The entry-field data.
    Entry(WndEntryData),
    /// The static-text data.
    StaticText(WndTextData),
    /// The radio-button data.
    Radio(WndRadioButtonData),
    /// The tab-control data.
    Tab(WndTabControlData),
}

/// The composite sub-gadget slot a window's construction routes draw
/// data into (which sub-window of the gadget the original copies the
/// arrays into).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WndSubSlot {
    /// The slider thumb (`SLIDERTHUMB*DRAWDATA`).
    Thumb,
    /// The listbox up button (`LISTBOX*UPBUTTONDRAWDATA`).
    UpButton,
    /// The listbox down button (`LISTBOX*DOWNBUTTONDRAWDATA`).
    DownButton,
    /// The listbox slider (`LISTBOX*SLIDERDRAWDATA`).
    Slider,
    /// The combo-box dropdown button (`COMBOBOXDROPDOWNBUTTON*DRAWDATA`).
    DropDownButton,
    /// The combo-box edit box (`COMBOBOXEDITBOX*DRAWDATA`).
    EditBox,
    /// The combo-box listbox (`COMBOBOXLISTBOX*DRAWDATA`).
    ListBox,
}

impl WndSubSlot {
    /// The slot's dump word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            WndSubSlot::Thumb => "thumb",
            WndSubSlot::UpButton => "upbtn",
            WndSubSlot::DownButton => "downbtn",
            WndSubSlot::Slider => "slider",
            WndSubSlot::DropDownButton => "dropdown",
            WndSubSlot::EditBox => "editbox",
            WndSubSlot::ListBox => "cblistbox",
        }
    }

    /// The field token that fills the slot's array, per state.
    #[must_use]
    pub fn field_token(self, state: WndDrawState) -> &'static str {
        match (self, state) {
            (WndSubSlot::Thumb, WndDrawState::Enabled) => "SLIDERTHUMBENABLEDDRAWDATA",
            (WndSubSlot::Thumb, WndDrawState::Disabled) => "SLIDERTHUMBDISABLEDDRAWDATA",
            (WndSubSlot::Thumb, WndDrawState::Hilite) => "SLIDERTHUMBHILITEDRAWDATA",
            (WndSubSlot::UpButton, WndDrawState::Enabled) => "LISTBOXENABLEDUPBUTTONDRAWDATA",
            (WndSubSlot::UpButton, WndDrawState::Disabled) => "LISTBOXDISABLEDUPBUTTONDRAWDATA",
            (WndSubSlot::UpButton, WndDrawState::Hilite) => "LISTBOXHILITEUPBUTTONDRAWDATA",
            (WndSubSlot::DownButton, WndDrawState::Enabled) => "LISTBOXENABLEDDOWNBUTTONDRAWDATA",
            (WndSubSlot::DownButton, WndDrawState::Disabled) => "LISTBOXDISABLEDDOWNBUTTONDRAWDATA",
            (WndSubSlot::DownButton, WndDrawState::Hilite) => "LISTBOXHILITEDOWNBUTTONDRAWDATA",
            (WndSubSlot::Slider, WndDrawState::Enabled) => "LISTBOXENABLEDSLIDERDRAWDATA",
            (WndSubSlot::Slider, WndDrawState::Disabled) => "LISTBOXDISABLEDSLIDERDRAWDATA",
            (WndSubSlot::Slider, WndDrawState::Hilite) => "LISTBOXHILITESLIDERDRAWDATA",
            (WndSubSlot::DropDownButton, WndDrawState::Enabled) => {
                "COMBOBOXDROPDOWNBUTTONENABLEDDRAWDATA"
            }
            (WndSubSlot::DropDownButton, WndDrawState::Disabled) => {
                "COMBOBOXDROPDOWNBUTTONDISABLEDDRAWDATA"
            }
            (WndSubSlot::DropDownButton, WndDrawState::Hilite) => {
                "COMBOBOXDROPDOWNBUTTONHILITEDRAWDATA"
            }
            (WndSubSlot::EditBox, WndDrawState::Enabled) => "COMBOBOXEDITBOXENABLEDDRAWDATA",
            (WndSubSlot::EditBox, WndDrawState::Disabled) => "COMBOBOXEDITBOXDISABLEDDRAWDATA",
            (WndSubSlot::EditBox, WndDrawState::Hilite) => "COMBOBOXEDITBOXHILITEDRAWDATA",
            (WndSubSlot::ListBox, WndDrawState::Enabled) => "COMBOBOXLISTBOXENABLEDDRAWDATA",
            (WndSubSlot::ListBox, WndDrawState::Disabled) => "COMBOBOXLISTBOXDISABLEDDRAWDATA",
            (WndSubSlot::ListBox, WndDrawState::Hilite) => "COMBOBOXLISTBOXHILITEDRAWDATA",
        }
    }
}

/// The draw-data state (which of the three arrays a token fills).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WndDrawState {
    /// The enabled state.
    Enabled,
    /// The disabled state.
    Disabled,
    /// The hilite state.
    Hilite,
}

impl WndDrawState {
    /// The state's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            WndDrawState::Enabled => "enabled",
            WndDrawState::Disabled => "disabled",
            WndDrawState::Hilite => "hilite",
        }
    }
}

/// One routed composite slot: the arrays snapshotted at the window's
/// creation (what the original copies into that sub-window).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WndSubDraw {
    /// The sub-gadget slot.
    pub slot: WndSubSlot,
    /// The enabled-state array.
    pub enabled: [WndDrawData; MAX_DRAW_DATA],
    /// The disabled-state array.
    pub disabled: [WndDrawData; MAX_DRAW_DATA],
    /// The hilite-state array.
    pub hilite: [WndDrawData; MAX_DRAW_DATA],
}

/// One created window: the instance-data snapshot at its creation plus
/// the tree links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WndWindow {
    /// The `WINDOWTYPE` word.
    pub window_type: WndWindowType,
    /// The window id (`NAME`'s interned key, or 0 unnamed; the root of a
    /// double-CHILD pair reuses the first's).
    pub id: i32,
    /// The arena index of the parent (`None` at the top level).
    pub parent: Option<usize>,
    /// The arena index of the owner (gadgets own their parent; USER and
    /// TABPANE windows leave it unset — the original only sets it in
    /// `createGadget`).
    pub owner: Option<usize>,
    /// The stored region: `x`/`y` relative to the parent (post-scaling),
    /// `width`/`height` from the scaled corners (possibly negative — the
    /// original never swaps them).
    pub x: i32,
    /// See [`x`](Self::x).
    pub y: i32,
    /// See [`x`](Self::x).
    pub width: i32,
    /// See [`x`](Self::x).
    pub height: i32,
    /// The parsed `STATUS` flags.
    pub status: u32,
    /// The parsed `STYLE` flags OR-ed with the type's creation bit.
    pub style: u32,
    /// The decorated name (`NAME`'s value, `"file.wnd:control"`).
    pub name: Vec<u8>,
    /// The four callback names.
    pub system_callback: Vec<u8>,
    /// See [`system_callback`](Self::system_callback).
    pub input_callback: Vec<u8>,
    /// See [`system_callback`](Self::system_callback).
    pub tooltip_callback: Vec<u8>,
    /// See [`system_callback`](Self::system_callback).
    pub draw_callback: Vec<u8>,
    /// The font request (`FONT`), if the field parsed.
    pub font: Option<WndFont>,
    /// The header-template name (`HEADERTEMPLATE`).
    pub header_template: Vec<u8>,
    /// The text label (`TEXT`).
    pub text_label: Vec<u8>,
    /// The tooltip label (`TOOLTIPTEXT`).
    pub tooltip_label: Vec<u8>,
    /// The tooltip delay (`TOOLTIPDELAY`; −1 when absent).
    pub tooltip_delay: i32,
    /// The image offset (`IMAGEOFFSET`).
    pub image_offset: (i32, i32),
    /// The three-state text colors.
    pub text_colors: WndTextColors,
    /// The enabled-state draw data.
    pub enabled_draw_data: [WndDrawData; MAX_DRAW_DATA],
    /// The disabled-state draw data.
    pub disabled_draw_data: [WndDrawData; MAX_DRAW_DATA],
    /// The hilite-state draw data.
    pub hilite_draw_data: [WndDrawData; MAX_DRAW_DATA],
    /// The gadget data snapshot.
    pub gadget: WndGadget,
    /// The routed composite draw data (in the original's routing order).
    pub sub_draw: Vec<WndSubDraw>,
    /// The arena indices of the children, in creation order.
    pub children: Vec<usize>,
}

impl Default for WndWindow {
    fn default() -> Self {
        WndWindow {
            window_type: WndWindowType::User,
            id: 0,
            parent: None,
            owner: None,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            status: 0,
            style: 0,
            name: Vec::new(),
            system_callback: Vec::new(),
            input_callback: Vec::new(),
            tooltip_callback: Vec::new(),
            draw_callback: Vec::new(),
            font: None,
            header_template: Vec::new(),
            text_label: Vec::new(),
            tooltip_label: Vec::new(),
            tooltip_delay: -1,
            image_offset: (0, 0),
            text_colors: WndTextColors::default(),
            enabled_draw_data: empty_draw(),
            disabled_draw_data: empty_draw(),
            hilite_draw_data: empty_draw(),
            gadget: WndGadget::None,
            sub_draw: Vec::new(),
            children: Vec::new(),
        }
    }
}

/// The file-scoped defaults the top-level block list parses (and the
/// child blocks re-parse).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WndDefaults {
    /// `ENABLEDCOLOR`.
    pub enabled_color: u32,
    /// `DISABLEDCOLOR`.
    pub disabled_color: u32,
    /// `BACKGROUNDCOLOR`.
    pub background_color: u32,
    /// `HILITECOLOR`.
    pub hilite_color: u32,
    /// `SELECTEDCOLOR`.
    pub selected_color: u32,
    /// `TEXTCOLOR` — the one every later window's text colors start from.
    pub text_color: u32,
    /// The default `FONT` stanza's text (parsed and discarded — the
    /// original's no-op; kept as the observable).
    pub font_text: Option<Vec<u8>>,
}

/// One root entry of the layout's window list: the original pushes a
/// `nullptr` for a root whose parse failed and keeps walking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WndRoot {
    /// The root's arena index.
    Window(usize),
    /// The failed root (the original's `nullptr` push).
    Failed,
}

/// A loaded `.wnd` layout — the whole `WindowLayoutInfo` plus the
/// walk's terminal verdict and the window arena (the name-key table does
/// not compare).
#[derive(Debug, Clone)]
pub struct WndLayout {
    /// How the load ended (the stop site for the hang/crash/oob shapes,
    /// with the partial results kept).
    pub verdict: WndVerdict,
    /// The parsed `FILE_VERSION` word (0 when the scan fails — the
    /// original leaves it uninitialized).
    pub version: u32,
    /// The layout callbacks' names (`[None]` below version 2, empty when
    /// the layout block never parsed).
    pub init_name: Vec<u8>,
    /// See [`init_name`](Self::init_name).
    pub update_name: Vec<u8>,
    /// See [`init_name`](Self::init_name).
    pub shutdown_name: Vec<u8>,
    /// The file-scoped defaults as the walk left them.
    pub defaults: WndDefaults,
    /// The root list in file order (failed roots included).
    pub roots: Vec<WndRoot>,
    /// Every created window (the manager's window list); the roots'
    /// trees and the orphans of a double-CHILD are all here.
    pub arena: Vec<WndWindow>,
    /// The name-key table the window ids and radio screen ids interned
    /// through (fresh per load — see the module docs).
    pub name_keys: NameKeys,
    /// The stream position the walk stopped at.
    pub stream_pos: usize,
    /// The input length.
    pub stream_len: usize,
    /// The display the SCREENRECT scaling divided by.
    pub display: WndDisplay,
}

impl WndLayout {
    /// `WindowLayout::load`'s verdict: a root window parsed.
    #[must_use]
    pub fn loaded(&self) -> bool {
        self.roots.iter().any(|r| matches!(r, WndRoot::Window(_)))
    }

    /// The first successfully parsed root (the original's `firstWindow`).
    #[must_use]
    pub fn first_window_root(&self) -> Option<usize> {
        self.roots
            .iter()
            .position(|r| matches!(r, WndRoot::Window(_)))
    }

    /// The root window at root-ordinal `index` (the arena entry of a
    /// [`WndRoot::Window`]).
    #[must_use]
    pub fn root_window(&self, index: usize) -> Option<&WndWindow> {
        match self.roots.get(index) {
            Some(WndRoot::Window(idx)) => self.arena.get(*idx),
            _ => None,
        }
    }

    /// The window's accumulated screen position — the
    /// `winGetScreenPosition` ancestor sum (this is what a child's
    /// parent-relative SCREENRECT subtracted).
    #[must_use]
    pub fn screen_position(&self, index: usize) -> (i32, i32) {
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut idx = Some(index);
        while let Some(i) = idx {
            let w = &self.arena[i];
            x = x.wrapping_add(w.x);
            y = y.wrapping_add(w.y);
            idx = w.parent;
        }
        (x, y)
    }
}

// ---------------------------------------------------------------------------
// the parser (the transcription's walk, function by function)
// ---------------------------------------------------------------------------

/// The walk's stop signal: the verdict is already recorded on the parser
/// (the transcription throws; the port propagates).
struct WalkStop;

type R<T> = Result<T, WalkStop>;

/// `isspace` over the C locale (the six ASCII whitespace bytes; the
/// high bytes are never spaces there).
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// An all-default draw-data array (`WinInstanceData::init`'s
/// `WIN_COLOR_UNDEFINED` sentinels).
fn empty_draw() -> [WndDrawData; MAX_DRAW_DATA] {
    std::array::from_fn(|_| WndDrawData {
        image: Vec::new(),
        color: WIN_COLOR_UNDEFINED,
        border_color: WIN_COLOR_UNDEFINED,
    })
}

/// `atoi` over a token: an optional sign then digits, wrapping through
/// `i32` (the overflow is UB in the original; the corpora never reach
/// it).
fn atoi(bytes: &[u8]) -> i32 {
    let mut i = 0;
    let mut neg = false;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        neg = bytes[i] == b'-';
        i += 1;
    }
    let mut acc: i32 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        acc = acc
            .wrapping_mul(10)
            .wrapping_add(i32::from(bytes[i] - b'0'));
        i += 1;
    }
    if neg { acc.wrapping_neg() } else { acc }
}

/// `sscanf("%d")` over a token: leading whitespace, an optional sign,
/// then digits; `None` when no digit matched (the target keeps its
/// previous value, as the original's does).
fn scan_d(bytes: &[u8]) -> Option<i32> {
    let mut i = 0;
    while i < bytes.len() && is_space(bytes[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        neg = bytes[i] == b'-';
        i += 1;
    }
    let start = i;
    let mut acc: i64 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        acc = (acc * 10 + i64::from(bytes[i] - b'0')).min(i32::MAX as i64 + 1);
        i += 1;
    }
    if i == start {
        return None; // no match: the caller's target is untouched
    }
    let v = if neg {
        (-acc).max(i32::MIN as i64)
    } else {
        acc.min(i32::MAX as i64)
    };
    Some(v as i32)
}

/// `stricmp == 0` over tokens.
fn eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// `strtok` over a caller-owned buffer: a token is a non-separator run,
/// and the separator it stops at is replaced with a NUL (the quote scans
/// walk *through* those NULs, so the writes are load state). `start`
/// resets the chain (the C `strtok(str, …)` form); `None` continues it.
fn strtok_in(
    buf: &mut [u8],
    save: &mut usize,
    start: Option<usize>,
    delims: &[u8],
) -> Option<(usize, usize)> {
    let mut p = start.unwrap_or(*save);
    while p < buf.len() && buf[p] != 0 && delims.contains(&buf[p]) {
        p += 1;
    }
    if p >= buf.len() || buf[p] == 0 {
        *save = p;
        return None;
    }
    let tok = p;
    while p < buf.len() && buf[p] != 0 && !delims.contains(&buf[p]) {
        p += 1;
    }
    if p < buf.len() && buf[p] != 0 {
        buf[p] = 0;
        *save = p + 1;
    } else {
        *save = p;
    }
    Some((tok, p))
}

/// The gadget-data templates and the DATA statics, as one movable value
/// (the original's shared statics the `data` pointer walks over).
#[derive(Debug, Clone, PartialEq, Eq)]
enum DataPayload {
    None,
    Slider(WndSliderData),
    Listbox(WndListboxData),
    Entry(WndEntryData),
    Text(WndTextData),
    Radio(WndRadioButtonData),
    Tab(WndTabControlData),
    ComboBox(WndComboBoxData),
}

/// The composite draw-data arrays (the load's running state the
/// LISTBOX*/COMBOBOX*/SLIDERTHUMB* fields overwrite and the creations
/// snapshot).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CompositeArrays {
    drop_down: [[WndDrawData; MAX_DRAW_DATA]; 3],
    edit_box: [[WndDrawData; MAX_DRAW_DATA]; 3],
    list_box: [[WndDrawData; MAX_DRAW_DATA]; 3],
    up_button: [[WndDrawData; MAX_DRAW_DATA]; 3],
    down_button: [[WndDrawData; MAX_DRAW_DATA]; 3],
    slider: [[WndDrawData; MAX_DRAW_DATA]; 3],
    thumb: [[WndDrawData; MAX_DRAW_DATA]; 3],
}

/// The working instance data a window's field list edits (the original's
/// stack `WinInstanceData`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct InstData {
    id: i32,
    status: u32,
    style: u32,
    name: Vec<u8>,
    text_label: Vec<u8>,
    tooltip_label: Vec<u8>,
    tooltip_delay: i32,
    image_offset: (i32, i32),
    font: Option<WndFont>,
    header_template: Vec<u8>,
    text_colors: WndTextColors,
    enabled: [WndDrawData; MAX_DRAW_DATA],
    disabled: [WndDrawData; MAX_DRAW_DATA],
    hilite: [WndDrawData; MAX_DRAW_DATA],
}

impl Default for InstData {
    fn default() -> Self {
        // WinInstanceData::init
        InstData {
            id: 0,
            status: 0,
            style: 0,
            name: Vec::new(),
            text_label: Vec::new(),
            tooltip_label: Vec::new(),
            tooltip_delay: -1,
            image_offset: (0, 0),
            font: None,
            header_template: Vec::new(),
            text_colors: WndTextColors::default(),
            enabled: empty_draw(),
            disabled: empty_draw(),
            hilite: empty_draw(),
        }
    }
}

/// The separator sets (the original's `seps` constants).
const FIELD_SEPS: &[u8] = b" =;\n\r\t";
const RECT_SEPS: &[u8] = b" ,:=\n\r\t";
const GADGET_SEPS: &[u8] = b" :,\n\r\t";
const SPACE_SEPS: &[u8] = b" \t\n\r";
const QUOTE_SEPS: &[u8] = b"\n\r\t\"";
const FONT_SEPS: &[u8] = b" ,\n\r\t";
const FONT_VALUE_SEPS: &[u8] = b":,\n\r\t\"";
const LAYOUT_WORD_SEPS: &[u8] = b" =";
const LAYOUT_NAME_SEPS: &[u8] = b" \n\r\t";
const DATA_SEPS: &[u8] = b" \t\n\r";
const PLUS_SEPS: &[u8] = b"+";

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    display: WndDisplay,

    /// The persistent read buffer (per load — the original's is a
    /// process global; see the module docs).
    buffer: Vec<u8>, // WIN_BUFFER_LENGTH long
    tok_save: usize,

    // the file-scoped defaults
    defaults: WndDefaults,

    // the gadget-data statics
    t_slider: WndSliderData,
    t_listbox: WndListboxData,
    t_entry: WndEntryData,
    t_text: WndTextData,
    t_radio: WndRadioButtonData,
    t_tab: WndTabControlData,
    t_combo: WndComboBoxData,
    d_slider: WndSliderData,
    d_listbox: WndListboxData,
    d_entry: WndEntryData,
    d_text: WndTextData,
    d_radio: WndRadioButtonData,

    arrays: CompositeArrays,

    arena: Vec<WndWindow>,
    window_stack: [Option<usize>; WIN_STACK_DEPTH],
    stack_ptr: usize,

    version: u32,
    init_name: Vec<u8>,
    update_name: Vec<u8>,
    shutdown_name: Vec<u8>,
    roots: Vec<WndRoot>,

    keys: NameKeys,

    verdict: Option<WndVerdict>,
}

impl<'a> Parser<'a> {
    fn new(data: &'a [u8], display: WndDisplay) -> Self {
        Parser {
            data,
            pos: 0,
            display,
            buffer: vec![0; WIN_BUFFER_LENGTH],
            tok_save: 0,
            defaults: WndDefaults::default(),
            t_slider: WndSliderData::default(),
            t_listbox: WndListboxData::default(),
            t_entry: WndEntryData::default(),
            t_text: WndTextData::default(),
            t_radio: WndRadioButtonData::default(),
            t_tab: WndTabControlData::default(),
            t_combo: WndComboBoxData::default(),
            d_slider: WndSliderData::default(),
            d_listbox: WndListboxData::default(),
            d_entry: WndEntryData::default(),
            d_text: WndTextData::default(),
            d_radio: WndRadioButtonData::default(),
            arrays: CompositeArrays::default(),
            arena: Vec::new(),
            window_stack: [None; WIN_STACK_DEPTH],
            stack_ptr: 0,
            version: 0,
            init_name: Vec::new(),
            update_name: Vec::new(),
            shutdown_name: Vec::new(),
            roots: Vec::new(),
            keys: NameKeys::new(),
            verdict: None,
        }
    }

    // --- the stop --------------------------------------------------------

    /// Record the verdict and unwind (the transcription throws before
    /// the undefined act).
    fn stop(&mut self, verdict: WndVerdict) -> R<()> {
        if self.verdict.is_none() {
            self.verdict = Some(verdict);
        }
        Err(WalkStop)
    }

    // --- the RAMFile cursor ----------------------------------------------

    /// `RAMFile::read(buffer + i, 1)` for one byte: writes the byte and
    /// advances, or leaves the stale byte and returns 0 at EOF.
    fn read_byte_into(&mut self, at: usize) -> usize {
        if self.pos < self.data.len() {
            self.buffer[at] = self.data[self.pos];
            self.pos += 1;
            1
        } else {
            0
        }
    }

    /// `RAMFile::scanInt`.
    fn scan_int(&mut self) -> (bool, i32) {
        while self.pos < self.data.len()
            && !(self.data[self.pos].is_ascii_digit() || self.data[self.pos] == b'-')
        {
            self.pos += 1;
        }
        if self.pos >= self.data.len() {
            self.pos = self.data.len();
            return (false, 0);
        }
        let mut tmp = Vec::new();
        loop {
            tmp.push(self.data[self.pos]);
            self.pos += 1;
            if !(self.pos < self.data.len() && self.data[self.pos].is_ascii_digit()) {
                break;
            }
        }
        (true, atoi(&tmp))
    }

    /// `RAMFile::scanString` — the token (empty at EOF, whose return the
    /// callers that ignore still compare).
    fn scan_string(&mut self) -> (bool, Vec<u8>) {
        let mut out = Vec::new();
        while self.pos < self.data.len() && is_space(self.data[self.pos]) {
            self.pos += 1;
        }
        if self.pos >= self.data.len() {
            self.pos = self.data.len();
            return (false, out);
        }
        loop {
            out.push(self.data[self.pos]);
            self.pos += 1;
            if !(self.pos < self.data.len() && !is_space(self.data[self.pos])) {
                break;
            }
        }
        (true, out)
    }

    /// `RAMFile::nextLine` — seek past the next `\n`.
    fn next_line(&mut self) {
        while self.pos < self.data.len() && self.data[self.pos] != b'\n' {
            self.pos += 1;
        }
        if self.pos < self.data.len() {
            self.pos += 1;
        }
        if self.pos >= self.data.len() {
            self.pos = self.data.len();
        }
    }

    // --- the read buffer's strtok / quote scan ---------------------------

    /// `strtok` over the read buffer.
    fn strtok(&mut self, start: Option<usize>, delims: &[u8]) -> Option<(usize, usize)> {
        strtok_in(&mut self.buffer, &mut self.tok_save, start, delims)
    }

    /// The token's bytes (a view into the read buffer).
    fn tok(&self, r: (usize, usize)) -> &[u8] {
        &self.buffer[r.0..r.1]
    }

    /// `strtok` result the original dereferences: `None` is the crash
    /// verdict at `site`.
    fn require(&mut self, r: Option<(usize, usize)>, site: &'static str) -> R<(usize, usize)> {
        match r {
            Some(x) => Ok(x),
            None => {
                self.stop(WndVerdict::Crash(site))?;
                unreachable!("stop always unwinds")
            }
        }
    }

    /// The quote scan: `while (*ptr != '"') ptr++` from the buffer start
    /// (walking through the strtok NULs), bounded by the array — the
    /// original walks off the end when no quote exists.
    fn scan_to_quote(&mut self, site: &'static str) -> R<usize> {
        let mut p = 0;
        while self.buffer[p] != b'"' {
            p += 1;
            if p >= WIN_BUFFER_LENGTH {
                self.stop(WndVerdict::Crash(site))?;
            }
        }
        Ok(p)
    }

    /// `readUntilSemicolon` — the collapsing read over the persistent
    /// buffer, stale EOF bytes included.
    fn read_until_semicolon(&mut self) -> R<()> {
        let mut i = 0usize;
        let mut start = true;
        while i < WIN_BUFFER_LENGTH {
            let got = self.read_byte_into(i);
            let b = self.buffer[i];
            if is_space(b) {
                if !start {
                    self.buffer[i] = b' ';
                    i += 1;
                } else if got == 0 {
                    // the stale leading whitespace at EOF: the original
                    // spins forever
                    self.stop(WndVerdict::Hang("readuntil-eof"))?;
                }
            } else {
                start = false;
                if b == b';' {
                    self.buffer[i] = 0;
                    return Ok(());
                }
                i += 1;
            }
        }
        // "Read buffer overflow - input truncated" (DEBUG_LOG only)
        self.buffer[WIN_BUFFER_LENGTH - 1] = 0;
        Ok(())
    }

    // --- the small field parsers -----------------------------------------

    /// `parseBitFlag` — `stricmp` over the flag list.
    fn parse_bit_flag(&self, flag: &[u8], list: &[&[u8]], bits: &mut u32) -> bool {
        for (i, name) in list.iter().enumerate() {
            if eq_ignore_case(name, flag) {
                *bits |= 1u32 << i;
                return true;
            }
        }
        false
    }

    /// `parseBitString` — the '+'-split over a 256-byte truncated copy;
    /// `NULL` matches on its first four bytes only.
    fn parse_bit_string(&self, list: &[&[u8]], bits: &mut u32) {
        let mut copy = [0u8; 256];
        // strlcpy from buffer[0] up to the first NUL, capped at 255 bytes
        let mut len = 0;
        while len < 255 && self.buffer[len] != 0 {
            copy[len] = self.buffer[len];
            len += 1;
        }
        if copy.starts_with(b"NULL") {
            return;
        }
        let mut save = 0usize;
        // strtok(buffer, "+") for the first flag, strtok(nullptr, "+) for
        // the rest — the chain continues through the copy
        let mut start = Some(0);
        while let Some((a, b)) = strtok_in(&mut copy, &mut save, start, PLUS_SEPS) {
            start = None;
            let flag: Vec<u8> = copy[a..b].to_vec();
            self.parse_bit_flag(&flag, list, bits);
            // "ParseBitString: Invalid flag '%s'." (DEBUG_LOG only)
        }
    }

    /// `parseColor` — `atoi` into bytes, packed opaque.
    fn parse_color(&mut self) -> R<u32> {
        let __tok = self.strtok(Some(0), SPACE_SEPS);
        let a = self.require(__tok, "color-token")?;
        let red = atoi(self.tok(a)) as u8;
        let __tok = self.strtok(None, SPACE_SEPS);
        let b = self.require(__tok, "color-token")?;
        let green = atoi(self.tok(b)) as u8;
        let __tok = self.strtok(None, SPACE_SEPS);
        let c = self.require(__tok, "color-token")?;
        let blue = atoi(self.tok(c)) as u8;
        Ok(game_make_color(red, green, blue, 255))
    }

    /// `parseDefaultColor` — the '=' eat is a whole scanString token;
    /// `TRANSPARENT` is the undefined color.
    fn parse_default_color(&mut self) -> R<u32> {
        let _ = self.scan_string(); // eat '='
        self.read_until_semicolon()?;
        // strcmp(buffer, "TRANSPARENT") == 0 — the buffer's C string
        let end = self
            .buffer
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(WIN_BUFFER_LENGTH);
        if &self.buffer[..end] == b"TRANSPARENT" {
            return Ok(WIN_COLOR_UNDEFINED);
        }
        self.parse_color()
    }

    /// `parseDefaultFont` — the stanza is parsed and DISCARDED (the
    /// original's `@todo`); the text is the observable.
    fn parse_default_font(&mut self) -> R<Vec<u8>> {
        let _ = self.scan_string(); // eat '='
        self.read_until_semicolon()?;
        let end = self
            .buffer
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(WIN_BUFFER_LENGTH);
        Ok(self.buffer[..end].to_vec())
    }

    /// `parseScreenRect` — the strtok continuation, the resolution
    /// scaling, the parent-relative adjustment.
    fn parse_screen_rect(&mut self) -> R<(i32, i32, i32, i32)> {
        // the chain continues from parseWindow's SCREENRECT label read:
        // three label/value/value groups (UPPERLEFT, BOTTOMRIGHT,
        // CREATIONRESOLUTION)
        let mut vals = [0i32; 6];
        for chunk in vals.chunks_mut(2) {
            let __tok = self.strtok(None, RECT_SEPS);
            let _ = self.require(__tok, "screenrect-token")?; // label
            for v in chunk {
                let __tok = self.strtok(None, RECT_SEPS);
                let r = self.require(__tok, "screenrect-token")?;
                if let Some(n) = scan_d(self.tok(r)) {
                    *v = n;
                }
            }
        }

        let (lo_x, lo_y, hi_x, hi_y, create_x, create_y) =
            (vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]);

        // shrink or expand by the current/creation resolution ratio
        let x_scale = self.display.width as f32 / create_x as f32;
        let y_scale = self.display.height as f32 / create_y as f32;
        let s_lo_x = (lo_x as f32 * x_scale) as i32;
        let s_lo_y = (lo_y as f32 * y_scale) as i32;
        let s_hi_x = (hi_x as f32 * x_scale) as i32;
        let s_hi_y = (hi_y as f32 * y_scale) as i32;

        let parent = self.peek_window();
        let (x, y) = match parent {
            Some(p) => {
                let (px, py) = self.screen_position_of(p);
                (s_lo_x.wrapping_sub(px), s_lo_y.wrapping_sub(py))
            }
            None => (s_lo_x, s_lo_y),
        };

        Ok((
            x,
            y,
            s_hi_x.wrapping_sub(s_lo_x),
            s_hi_y.wrapping_sub(s_lo_y),
        ))
    }

    /// `winGetScreenPosition` over the arena (the ancestor sum).
    fn screen_position_of(&self, index: usize) -> (i32, i32) {
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut idx = Some(index);
        while let Some(i) = idx {
            let w = &self.arena[i];
            x = x.wrapping_add(w.x);
            y = y.wrapping_add(w.y);
            idx = w.parent;
        }
        (x, y)
    }

    /// `parseImageOffset`.
    fn parse_image_offset(&mut self, inst: &mut InstData) -> R<()> {
        let __tok = self.strtok(Some(0), SPACE_SEPS);
        let a = self.require(__tok, "imageoffset-token")?;
        inst.image_offset.0 = atoi(self.tok(a));
        let __tok = self.strtok(None, SPACE_SEPS);
        let b = self.require(__tok, "imageoffset-token")?;
        inst.image_offset.1 = atoi(self.tok(b));
        Ok(())
    }

    /// `parseFont` — the label strtok, the quote scan through its NUL,
    /// the value after the quote, SIZE and BOLD.
    fn parse_font(&mut self, inst: &mut InstData) -> R<()> {
        let _ = self.strtok(Some(0), FONT_SEPS); // label
        let quote = self.scan_to_quote("font-quote")?;
        let __tok = self.strtok(Some(quote + 1), FONT_VALUE_SEPS);
        let name = self.require(__tok, "font-token")?;
        let font_name = self.tok(name).to_vec();

        let _ = self.strtok(None, FONT_SEPS); // "SIZE" label
        let __tok = self.strtok(None, FONT_SEPS);
        let sz = self.require(__tok, "font-value")?;
        let size = scan_d(self.tok(sz)).unwrap_or(0);

        let _ = self.strtok(None, FONT_SEPS); // "BOLD" label
        let __tok = self.strtok(None, FONT_SEPS);
        let bl = self.require(__tok, "font-value")?;
        let bold = scan_d(self.tok(bl)).unwrap_or(0);

        inst.font = Some(WndFont {
            name: font_name,
            size,
            bold,
        });
        Ok(())
    }

    /// `parseName` — the quoted value; the id is the interned key.
    fn parse_name(&mut self, inst: &mut InstData) -> R<()> {
        let quote = self.scan_to_quote("name-quote")?;
        let __tok = self.strtok(Some(quote + 1), b"\"");
        let val = self.require(__tok, "name-token")?;
        let name = self.tok(val).to_vec();
        inst.id = self.keys.name_to_key(&name).ordinal() as i32;
        inst.name = name;
        Ok(())
    }

    /// `parseStatus`.
    fn parse_status(&mut self, inst: &mut InstData) {
        inst.status = 0;
        self.parse_bit_string(&WINDOW_STATUS_NAMES, &mut inst.status);
    }

    /// `parseStyle`.
    fn parse_style(&mut self, inst: &mut InstData) {
        inst.style = 0;
        self.parse_bit_string(&WINDOW_STYLE_NAMES, &mut inst.style);
    }

    /// The callback parsers' shared shape (`parseSystemCallback` and
    /// friends).
    fn parse_callback_string(&mut self) -> R<Vec<u8>> {
        let quote = self.scan_to_quote("cb-quote")?;
        let __tok = self.strtok(Some(quote + 1), b"\"");
        let val = self.require(__tok, "cb-token")?;
        Ok(self.tok(val).to_vec())
    }

    /// `parseHeaderTemplate`.
    fn parse_header_template(&mut self, inst: &mut InstData) -> R<()> {
        inst.header_template = self.parse_callback_string()?;
        Ok(())
    }

    /// `parseListboxData` — the optional ScrollIfAtEnd gate, the column
    /// run.
    fn parse_listbox_data(&mut self, ld: &mut WndListboxData) -> R<()> {
        let __tok = self.strtok(Some(0), GADGET_SEPS);
        let _ = self.require(__tok, "listboxdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "listboxdata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            ld.list_length = n as i16;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "listboxdata-token")?; // label
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "listboxdata-token")?;
        ld.auto_scroll = scan_d(self.tok(v))
            .map(|n| n != 0)
            .unwrap_or(ld.auto_scroll);

        // "SCROLLIFATEND" (optional)
        let __tok = self.strtok(None, GADGET_SEPS);
        let label = self.require(__tok, "listboxdata-token")?;
        let label_b = self.tok(label).to_vec();
        if eq_ignore_case(&label_b, b"ScrollIfAtEnd") {
            let __tok = self.strtok(None, GADGET_SEPS);
            let v = self.require(__tok, "listboxdata-token")?;
            ld.scroll_if_at_end = scan_d(self.tok(v)).map(|n| n != 0).unwrap_or(false);
            let __tok = self.strtok(None, GADGET_SEPS);
            let _ = self.require(__tok, "listboxdata-token")?;
        } else {
            ld.scroll_if_at_end = false;
        }

        // "AUTOPURGE" (the label the gate skip already consumed)
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "listboxdata-token")?;
        ld.auto_purge = scan_d(self.tok(v)).map(|n| n != 0).unwrap_or(ld.auto_purge);

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "listboxdata-token")?; // label
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "listboxdata-token")?;
        ld.scroll_bar = scan_d(self.tok(v)).map(|n| n != 0).unwrap_or(ld.scroll_bar);

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "listboxdata-token")?; // label
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "listboxdata-token")?;
        ld.multi_select = scan_d(self.tok(v))
            .map(|n| n != 0)
            .unwrap_or(ld.multi_select);

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "listboxdata-token")?; // label
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "listboxdata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            ld.columns = n as i16;
        }
        if ld.columns > 1 {
            let mut pcts = Vec::new();
            for _ in 0..ld.columns {
                let __tok = self.strtok(None, GADGET_SEPS);
                let _ = self.require(__tok, "listboxdata-token")?; // label
                let __tok = self.strtok(None, GADGET_SEPS);
                let v = self.require(__tok, "listboxdata-token")?;
                pcts.push(scan_d(self.tok(v)).unwrap_or(0));
            }
            ld.column_width_percentage = Some(pcts);
        } else {
            ld.column_width_percentage = None;
        }
        // columnWidth = nullptr (never set by the parse)

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "listboxdata-token")?; // label
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "listboxdata-token")?;
        ld.force_select = scan_d(self.tok(v))
            .map(|n| n != 0)
            .unwrap_or(ld.force_select);

        Ok(())
    }

    /// `parseComboBoxData`.
    fn parse_combo_box_data(&mut self, cd: &mut WndComboBoxData) -> R<()> {
        let __tok = self.strtok(Some(0), GADGET_SEPS);
        let _ = self.require(__tok, "comboboxdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "comboboxdata-token")?;
        cd.is_editable = scan_d(self.tok(v))
            .map(|n| n != 0)
            .unwrap_or(cd.is_editable);

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "comboboxdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "comboboxdata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            cd.max_chars = n;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "comboboxdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "comboboxdata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            cd.max_display = n;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "comboboxdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "comboboxdata-token")?;
        cd.ascii_only = scan_d(self.tok(v)).map(|n| n != 0).unwrap_or(cd.ascii_only);

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "comboboxdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "comboboxdata-token")?;
        cd.letters_and_numbers_only = scan_d(self.tok(v))
            .map(|n| n != 0)
            .unwrap_or(cd.letters_and_numbers_only);

        Ok(())
    }

    /// `parseSliderData`.
    fn parse_slider_data(&mut self, sd: &mut WndSliderData) -> R<()> {
        let __tok = self.strtok(Some(0), GADGET_SEPS);
        let _ = self.require(__tok, "sliderdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "sliderdata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            sd.min_val = n;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "sliderdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "sliderdata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            sd.max_val = n;
        }
        Ok(())
    }

    /// `parseRadioButtonData`.
    fn parse_radio_button_data(&mut self, rd: &mut WndRadioButtonData) -> R<()> {
        let __tok = self.strtok(Some(0), GADGET_SEPS);
        let _ = self.require(__tok, "radiobuttondata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "radiobuttondata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            rd.group = n;
        }
        Ok(())
    }

    /// `parseTooltipText` — the empty-label early out, the label bound.
    fn parse_tooltip_text(&mut self, inst: &mut InstData) -> R<bool> {
        let quote = self.scan_to_quote("tooltiptext-quote")?;
        let after = quote + 1;
        // strlen(ptr) == 1 — the remaining C string is one byte (the
        // closing quote of an empty label)
        let rest_len = self.buffer[after..]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(WIN_BUFFER_LENGTH - after);
        if rest_len == 1 {
            return Ok(true);
        }
        let __tok = self.strtok(Some(after), QUOTE_SEPS);
        let val = self.require(__tok, "tooltiptext-token")?;
        let tok = self.tok(val).to_vec();
        if tok.len() >= MAX_TEXT_LABEL {
            // "TextTooltip label '%s' is too long" — FALSE fails the window
            return Ok(false);
        }
        inst.tooltip_label = tok;
        Ok(true)
    }

    /// `parseTooltipDelay`.
    fn parse_tooltip_delay(&mut self, inst: &mut InstData) -> R<()> {
        let __tok = self.strtok(Some(0), GADGET_SEPS);
        let v = self.require(__tok, "tooltipdelay-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            inst.tooltip_delay = n;
        }
        Ok(())
    }

    /// `parseText`.
    fn parse_text(&mut self, inst: &mut InstData) -> R<bool> {
        let quote = self.scan_to_quote("text-quote")?;
        let __tok = self.strtok(Some(quote + 1), QUOTE_SEPS);
        let val = self.require(__tok, "text-token")?;
        let tok = self.tok(val).to_vec();
        if tok.len() >= MAX_TEXT_LABEL {
            // "Text label '%s' is too long" — FALSE fails the window
            return Ok(false);
        }
        inst.text_label = tok;
        Ok(true)
    }

    /// `parseTextColor` — the positional three-state walk.
    fn parse_text_color(&mut self, inst: &mut InstData) -> R<()> {
        let mut first = true;
        for state in 0..3 {
            // color
            if first {
                let __tok = self.strtok(Some(0), GADGET_SEPS);
                let _ = self.require(__tok, "textcolor-token")?;
                first = false;
            } else {
                let __tok = self.strtok(None, GADGET_SEPS);
                let _ = self.require(__tok, "textcolor-token")?;
            }
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for v in [&mut r, &mut g, &mut b, &mut a] {
                let __tok = self.strtok(None, GADGET_SEPS);
                let t = self.require(__tok, "textcolor-token")?;
                if let Some(n) = scan_d(self.tok(t)) {
                    *v = n as u32;
                }
            }
            let color = game_make_color(r as u8, g as u8, b as u8, a as u8);

            // border color
            let __tok = self.strtok(None, GADGET_SEPS);
            let _ = self.require(__tok, "textcolor-token")?;
            let (mut br, mut bg, mut bb, mut ba) = (0u32, 0u32, 0u32, 0u32);
            for v in [&mut br, &mut bg, &mut bb, &mut ba] {
                let __tok = self.strtok(None, GADGET_SEPS);
                let t = self.require(__tok, "textcolor-token")?;
                if let Some(n) = scan_d(self.tok(t)) {
                    *v = n as u32;
                }
            }
            let border = game_make_color(br as u8, bg as u8, bb as u8, ba as u8);

            match state {
                0 => inst.text_colors.enabled = (color, border),
                1 => inst.text_colors.disabled = (color, border),
                _ => inst.text_colors.hilite = (color, border),
            }
        }
        Ok(())
    }

    /// `parseStaticTextData` — the forced defaults the editor never
    /// writes.
    fn parse_static_text_data(&mut self, td: &mut WndTextData) -> R<()> {
        let __tok = self.strtok(Some(0), GADGET_SEPS);
        let _ = self.require(__tok, "statictextdata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "statictextdata-token")?;
        td.centered = scan_d(self.tok(v)).map(|n| n != 0).unwrap_or(td.centered);

        td.centered_vertically = true;
        td.left_margin = 7;
        td.top_margin = 7;
        Ok(())
    }

    /// `parseTextEntryData`.
    fn parse_text_entry_data(&mut self, ed: &mut WndEntryData) -> R<()> {
        let __tok = self.strtok(Some(0), GADGET_SEPS);
        let _ = self.require(__tok, "textentrydata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "textentrydata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            ed.max_text_len = n as i16;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "textentrydata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "textentrydata-token")?;
        ed.secret_text = scan_d(self.tok(v))
            .map(|n| n != 0)
            .unwrap_or(ed.secret_text);

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "textentrydata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "textentrydata-token")?;
        ed.numerical_only = scan_d(self.tok(v))
            .map(|n| n != 0)
            .unwrap_or(ed.numerical_only);

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "textentrydata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "textentrydata-token")?;
        ed.alpha_numerical_only = scan_d(self.tok(v))
            .map(|n| n != 0)
            .unwrap_or(ed.alpha_numerical_only);

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "textentrydata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "textentrydata-token")?;
        ed.ascii_only = scan_d(self.tok(v)).map(|n| n != 0).unwrap_or(ed.ascii_only);
        Ok(())
    }

    /// `parseTabControlData` — the pane run (the out-of-bounds write is
    /// the oob verdict).
    fn parse_tab_control_data(&mut self, td: &mut WndTabControlData) -> R<()> {
        let __tok = self.strtok(Some(0), GADGET_SEPS);
        let _ = self.require(__tok, "tabcontroldata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "tabcontroldata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            td.tab_orientation = n;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "tabcontroldata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "tabcontroldata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            td.tab_edge = n;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "tabcontroldata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "tabcontroldata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            td.tab_width = n;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "tabcontroldata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "tabcontroldata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            td.tab_height = n;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "tabcontroldata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "tabcontroldata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            td.tab_count = n;
        }

        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "tabcontroldata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "tabcontroldata-token")?;
        if let Some(n) = scan_d(self.tok(v)) {
            td.pane_border = n;
        }

        // PANEDISABLED
        let __tok = self.strtok(None, GADGET_SEPS);
        let _ = self.require(__tok, "tabcontroldata-token")?;
        let __tok = self.strtok(None, GADGET_SEPS);
        let v = self.require(__tok, "tabcontroldata-token")?;
        let entry_count = scan_d(self.tok(v)).unwrap_or(0);

        for pane_index in 0..entry_count {
            let __tok = self.strtok(None, GADGET_SEPS);
            let t = self.require(__tok, "tabcontroldata-token")?;
            let v = scan_d(self.tok(t)).map(|n| n != 0).unwrap_or(false);
            if pane_index >= NUM_TAB_PANES as i32 {
                self.stop(WndVerdict::Oob("subpane-disabled-write"))?;
            }
            td.sub_pane_disabled[pane_index as usize] = v;
        }
        Ok(())
    }

    /// `parseDrawData` — the nine-element positional walk; the token
    /// selects the target array.
    fn parse_draw_data(&mut self, token: &[u8], inst: &mut InstData) -> R<()> {
        // which array family the token fills, and its state index
        #[derive(Clone, Copy)]
        enum Family {
            Window, // the inst data's own arrays (state picks which)
            Thumb,
            UpButton,
            DownButton,
            Slider,
            DropDown,
            EditBox,
            ListBox,
        }
        // the token's middle word carries the state (the suffix alone
        // does not: LISTBOX*ENABLED*UPBUTTONDRAWDATA ends in …BUTTONDRAWDATA)
        let (family, state) = match token {
            b"ENABLEDDRAWDATA" => (Family::Window, 0usize),
            b"DISABLEDDRAWDATA" => (Family::Window, 1),
            b"HILITEDRAWDATA" => (Family::Window, 2),
            b"LISTBOXENABLEDUPBUTTONDRAWDATA" => (Family::UpButton, 0),
            b"LISTBOXDISABLEDUPBUTTONDRAWDATA" => (Family::UpButton, 1),
            b"LISTBOXHILITEUPBUTTONDRAWDATA" => (Family::UpButton, 2),
            b"LISTBOXENABLEDDOWNBUTTONDRAWDATA" => (Family::DownButton, 0),
            b"LISTBOXDISABLEDDOWNBUTTONDRAWDATA" => (Family::DownButton, 1),
            b"LISTBOXHILITEDOWNBUTTONDRAWDATA" => (Family::DownButton, 2),
            b"LISTBOXENABLEDSLIDERDRAWDATA" => (Family::Slider, 0),
            b"LISTBOXDISABLEDSLIDERDRAWDATA" => (Family::Slider, 1),
            b"LISTBOXHILITESLIDERDRAWDATA" => (Family::Slider, 2),
            b"SLIDERTHUMBENABLEDDRAWDATA" => (Family::Thumb, 0),
            b"SLIDERTHUMBDISABLEDDRAWDATA" => (Family::Thumb, 1),
            b"SLIDERTHUMBHILITEDRAWDATA" => (Family::Thumb, 2),
            b"COMBOBOXDROPDOWNBUTTONENABLEDDRAWDATA" => (Family::DropDown, 0),
            b"COMBOBOXDROPDOWNBUTTONDISABLEDDRAWDATA" => (Family::DropDown, 1),
            b"COMBOBOXDROPDOWNBUTTONHILITEDRAWDATA" => (Family::DropDown, 2),
            b"COMBOBOXEDITBOXENABLEDDRAWDATA" => (Family::EditBox, 0),
            b"COMBOBOXEDITBOXDISABLEDDRAWDATA" => (Family::EditBox, 1),
            b"COMBOBOXEDITBOXHILITEDRAWDATA" => (Family::EditBox, 2),
            b"COMBOBOXLISTBOXENABLEDDRAWDATA" => (Family::ListBox, 0),
            b"COMBOBOXLISTBOXDISABLEDDRAWDATA" => (Family::ListBox, 1),
            b"COMBOBOXLISTBOXHILITEDRAWDATA" => (Family::ListBox, 2),
            // "ParseDrawData, undefined token" — unreachable: the field
            // table only routes these tokens here
            _ => (Family::Window, 0),
        };

        // a local copy of the array being filled (the parse walks the
        // buffer; the array lands back at the end)
        let mut work: [WndDrawData; MAX_DRAW_DATA] = match family {
            Family::Window if state == 0 => inst.enabled.clone(),
            Family::Window if state == 1 => inst.disabled.clone(),
            Family::Window => inst.hilite.clone(),
            Family::Thumb => self.arrays.thumb[state].clone(),
            Family::UpButton => self.arrays.up_button[state].clone(),
            Family::DownButton => self.arrays.down_button[state].clone(),
            Family::Slider => self.arrays.slider[state].clone(),
            Family::DropDown => self.arrays.drop_down[state].clone(),
            Family::EditBox => self.arrays.edit_box[state].clone(),
            Family::ListBox => self.arrays.list_box[state].clone(),
        };

        let mut first = true;
        for entry in &mut work {
            // IMAGE: X
            if first {
                let __tok = self.strtok(Some(0), GADGET_SEPS);
                let _ = self.require(__tok, "drawdata-image")?;
                first = false;
            } else {
                let __tok = self.strtok(None, GADGET_SEPS);
                let _ = self.require(__tok, "drawdata-image")?;
            }
            let __tok = self.strtok(None, GADGET_SEPS);
            let img = self.require(__tok, "drawdata-image")?;
            if self.tok(img) != b"NoImage" {
                entry.image = self.tok(img).to_vec();
            } else {
                entry.image.clear();
            }

            // COLOR: R G B A
            let __tok = self.strtok(None, GADGET_SEPS);
            let _ = self.require(__tok, "drawdata-color")?; // label
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for v in [&mut r, &mut g, &mut b, &mut a] {
                let __tok = self.strtok(None, GADGET_SEPS);
                let t = self.require(__tok, "drawdata-color")?;
                if let Some(n) = scan_d(self.tok(t)) {
                    *v = n as u32;
                }
            }
            entry.color = game_make_color(r as u8, g as u8, b as u8, a as u8);

            // BORDERCOLOR: R G B A
            let __tok = self.strtok(None, GADGET_SEPS);
            let _ = self.require(__tok, "drawdata-color")?; // label
            let (mut br, mut bg, mut bb, mut ba) = (0u32, 0u32, 0u32, 0u32);
            for v in [&mut br, &mut bg, &mut bb, &mut ba] {
                let __tok = self.strtok(None, GADGET_SEPS);
                let t = self.require(__tok, "drawdata-color")?;
                if let Some(n) = scan_d(self.tok(t)) {
                    *v = n as u32;
                }
            }
            entry.border_color = game_make_color(br as u8, bg as u8, bb as u8, ba as u8);
        }

        match family {
            Family::Window if state == 0 => inst.enabled = work,
            Family::Window if state == 1 => inst.disabled = work,
            Family::Window => inst.hilite = work,
            Family::Thumb => self.arrays.thumb[state] = work,
            Family::UpButton => self.arrays.up_button[state] = work,
            Family::DownButton => self.arrays.down_button[state] = work,
            Family::Slider => self.arrays.slider[state] = work,
            Family::DropDown => self.arrays.drop_down[state] = work,
            Family::EditBox => self.arrays.edit_box[state] = work,
            Family::ListBox => self.arrays.list_box[state] = work,
        }
        Ok(())
    }

    // --- the data templates (getDataTemplate / parseData) ---------------

    /// `getDataTemplate` — the zeroed template for the type, tracked as
    /// which static the `data` pointer aliases.
    fn take_data_template(&mut self, window_type: WndWindowType) -> DataPayload {
        match window_type {
            WndWindowType::VertSlider | WndWindowType::HorzSlider => {
                self.t_slider = WndSliderData::default();
                DataPayload::Slider(self.t_slider)
            }
            WndWindowType::ScrollListBox => {
                self.t_listbox = WndListboxData::default();
                DataPayload::Listbox(self.t_listbox.clone())
            }
            WndWindowType::TabControl => {
                self.t_tab = WndTabControlData::default();
                DataPayload::Tab(self.t_tab.clone())
            }
            WndWindowType::EntryField => {
                self.t_entry = WndEntryData::default();
                DataPayload::Entry(self.t_entry)
            }
            WndWindowType::StaticText => {
                self.t_text = WndTextData::default();
                DataPayload::Text(self.t_text)
            }
            WndWindowType::RadioButton => {
                self.t_radio = WndRadioButtonData::default();
                DataPayload::Radio(self.t_radio)
            }
            WndWindowType::ComboBox => {
                self.t_combo = WndComboBoxData::default();
                DataPayload::ComboBox(self.t_combo.clone())
            }
            _ => DataPayload::None,
        }
    }

    /// `parseData` — the obsolete flat DATA field: a fresh set of statics
    /// (statictext and radio are *not* re-zeroed in the original — their
    /// never-written members keep the load's zeros forever).
    fn parse_data(&mut self, window_type: WndWindowType) -> R<DataPayload> {
        let __tok = self.strtok(Some(0), DATA_SEPS);
        let a = self.require(__tok, "data-token")?;
        let first = atoi(self.tok(a));

        match window_type {
            WndWindowType::VertSlider | WndWindowType::HorzSlider => {
                let __tok = self.strtok(None, DATA_SEPS);
                let b = self.require(__tok, "data-token")?;
                let second = atoi(self.tok(b));
                self.d_slider = WndSliderData {
                    min_val: first,
                    max_val: second,
                };
                Ok(DataPayload::Slider(self.d_slider))
            }
            WndWindowType::ScrollListBox => {
                let mut vals = [first, 0, 0, 0, 0, 0];
                for v in &mut vals[1..] {
                    let __tok = self.strtok(None, DATA_SEPS);
                    let t = self.require(__tok, "data-token")?;
                    *v = atoi(self.tok(t));
                }
                // listLength, autoScroll, autoPurge, scrollBar, multiSelect,
                // forceSelect (scrollIfAtEnd and columns stay 0)
                self.d_listbox = WndListboxData {
                    list_length: vals[0] as i16,
                    auto_scroll: vals[1] != 0,
                    auto_purge: vals[2] != 0,
                    scroll_bar: vals[3] != 0,
                    multi_select: vals[4] != 0,
                    force_select: vals[5] != 0,
                    ..WndListboxData::default()
                };
                Ok(DataPayload::Listbox(self.d_listbox.clone()))
            }
            WndWindowType::EntryField => {
                // (the entryWidth read is commented out in the original —
                // one token is skipped)
                let _ = self.strtok(None, DATA_SEPS); // skipped, unchecked
                let mut secret = false;
                if let Some(t) = self.strtok(None, DATA_SEPS) {
                    secret = atoi(self.tok(t)) != 0;
                }
                let mut numerical = false;
                let mut alpha = false;
                let mut ascii = false;
                if let Some(t) = self.strtok(None, DATA_SEPS) {
                    let v = atoi(self.tok(t));
                    numerical = v == 1;
                    alpha = v == 2;
                    ascii = v == 3;
                }
                self.d_entry = WndEntryData {
                    max_text_len: first as i16,
                    secret_text: secret,
                    numerical_only: numerical,
                    alpha_numerical_only: alpha,
                    ascii_only: ascii,
                };
                Ok(DataPayload::Entry(self.d_entry))
            }
            WndWindowType::StaticText => {
                // no memset: centeredVertically/leftMargin/topMargin keep
                // the load's zeros (nothing ever writes them)
                self.d_text.centered = first != 0;
                let _ = self.strtok(None, DATA_SEPS); // the dead label read
                Ok(DataPayload::Text(self.d_text))
            }
            WndWindowType::RadioButton => {
                // no memset
                self.d_radio.group = first;
                Ok(DataPayload::Radio(self.d_radio))
            }
            _ => Ok(DataPayload::None),
        }
    }

    // --- the window stack -------------------------------------------------

    fn peek_window(&self) -> Option<usize> {
        if self.stack_ptr == 0 {
            None
        } else {
            self.window_stack[self.stack_ptr - 1]
        }
    }

    fn pop_window(&mut self) -> Option<usize> {
        if self.stack_ptr == 0 {
            return None;
        }
        self.stack_ptr -= 1;
        self.window_stack[self.stack_ptr]
    }

    fn push_window(&mut self, index: usize) {
        if self.stack_ptr == WIN_STACK_DEPTH - 1 {
            // "pushWindow: Warning, stack overflow" (DEBUG_LOG only) — the
            // push is silently refused
            return;
        }
        self.window_stack[self.stack_ptr] = Some(index);
        self.stack_ptr += 1;
    }

    /// `GadgetTabControlFixupSubPaneList` — the children reversed, capped
    /// at NUM_TAB_PANES (a longer list writes past the array in the
    /// original — the oob verdict).
    fn fixup_sub_pane_list(&mut self, index: usize) -> R<()> {
        let children = self.arena[index].children.clone();
        if children.is_empty() {
            // the empty walk leaves the slots as they were (the original
            // keeps its null-filled array)
            if let WndGadget::Tab(td) = &mut self.arena[index].gadget {
                td.sub_panes = None;
            }
            return Ok(());
        }
        let mut panes = Vec::new();
        for k in (0..children.len()).rev() {
            if panes.len() >= NUM_TAB_PANES {
                self.stop(WndVerdict::Oob("subpanes-write"))?;
            }
            panes.push(children[k]);
        }
        if let WndGadget::Tab(td) = &mut self.arena[index].gadget {
            td.sub_panes = Some(panes);
        }
        Ok(())
    }

    // --- creation (createGadget / createWindow) ---------------------------

    /// The style bit the type ORs in at creation.
    fn type_style_bit(window_type: WndWindowType) -> u32 {
        match window_type {
            WndWindowType::User => GWS_USER_WINDOW,
            WndWindowType::TabPane => GWS_TAB_PANE,
            WndWindowType::PushButton => GWS_PUSH_BUTTON,
            WndWindowType::RadioButton => GWS_RADIO_BUTTON,
            WndWindowType::CheckBox => GWS_CHECK_BOX,
            WndWindowType::TabControl => GWS_TAB_CONTROL,
            WndWindowType::VertSlider => GWS_VERT_SLIDER,
            WndWindowType::HorzSlider => GWS_HORZ_SLIDER,
            WndWindowType::ScrollListBox => GWS_SCROLL_LISTBOX,
            WndWindowType::ComboBox => GWS_COMBO_BOX,
            WndWindowType::EntryField => GWS_ENTRY_FIELD,
            WndWindowType::StaticText => GWS_STATIC_TEXT,
            WndWindowType::ProgressBar => GWS_PROGRESS_BAR,
        }
    }

    /// `createWindow` — USER/TABPANE take the generic creation; every
    /// other type routes to `createGadget`. Returns the new window's
    /// arena index, or `None` for an unknown gadget type.
    #[allow(clippy::too_many_arguments)]
    fn create_window(
        &mut self,
        window_type: WndWindowType,
        id: i32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        inst: &InstData,
        mut data: DataPayload,
        callbacks: &CallbackStrings,
    ) -> Option<usize> {
        let parent = self.peek_window();
        let index = self.arena.len();

        let mut win = WndWindow {
            window_type,
            id,
            parent,
            owner: None, // only createGadget sets the owner
            x,
            y,
            width,
            height,
            status: inst.status,
            style: inst.style | Self::type_style_bit(window_type),
            name: inst.name.clone(),
            system_callback: callbacks.system.clone(),
            input_callback: callbacks.input.clone(),
            tooltip_callback: callbacks.tooltip.clone(),
            draw_callback: callbacks.draw.clone(),
            font: inst.font.clone(),
            header_template: inst.header_template.clone(),
            text_label: inst.text_label.clone(),
            tooltip_label: inst.tooltip_label.clone(),
            tooltip_delay: inst.tooltip_delay,
            image_offset: inst.image_offset,
            text_colors: inst.text_colors.clone(),
            enabled_draw_data: inst.enabled.clone(),
            disabled_draw_data: inst.disabled.clone(),
            hilite_draw_data: inst.hilite.clone(),
            gadget: WndGadget::None,
            sub_draw: Vec::new(),
            children: Vec::new(),
        };

        // the gadget data the construction reads (the owner, the radio
        // screen id, the combo halves)
        match (&mut data, window_type) {
            (DataPayload::Radio(rd), WndWindowType::RadioButton) => {
                // the screen id is the name's file prefix, interned
                let mut filename = inst.name.clone();
                if let Some(colon) = filename.iter().position(|&b| b == b':') {
                    filename.truncate(colon);
                }
                // MAX_WINDOW_NAME_LEN strlcpy truncation
                filename.truncate(MAX_WINDOW_NAME_LEN - 1);
                rd.screen = self.keys.name_to_key(&filename).ordinal() as i32;
                win.owner = parent;
                win.gadget = WndGadget::Radio(*rd);
            }
            (DataPayload::Slider(sd), WndWindowType::VertSlider)
            | (DataPayload::Slider(sd), WndWindowType::HorzSlider) => {
                win.owner = parent;
                win.gadget = WndGadget::Slider(*sd);
            }
            (DataPayload::Listbox(ld), WndWindowType::ScrollListBox) => {
                win.owner = parent;
                win.gadget = WndGadget::Listbox(ld.clone());
            }
            (DataPayload::Tab(td), WndWindowType::TabControl) => {
                win.owner = parent;
                win.gadget = WndGadget::Tab(td.clone());
            }
            (DataPayload::Entry(ed), WndWindowType::EntryField) => {
                win.owner = parent;
                win.gadget = WndGadget::Entry(*ed);
            }
            (DataPayload::Text(td), WndWindowType::StaticText) => {
                win.owner = parent;
                win.gadget = WndGadget::StaticText(*td);
            }
            (DataPayload::ComboBox(cd), WndWindowType::ComboBox) => {
                // createGadget's constructed halves (verbatim): the NEW
                // EntryData / ListboxData with their fixed starts and the
                // copied gates, entryCount = 0
                cd.entry_count = 0;
                cd.entry_data = WndEntryData {
                    ascii_only: cd.ascii_only,
                    alpha_numerical_only: cd.letters_and_numbers_only,
                    max_text_len: cd.max_chars as i16,
                    ..WndEntryData::default()
                };
                cd.listbox_data = WndListboxData {
                    list_length: 10,
                    scroll_bar: true,
                    force_select: true,
                    columns: 1,
                    ..WndListboxData::default()
                };
                win.owner = parent;
                win.gadget = WndGadget::ComboBox(cd.clone());
            }
            _ => {
                // PUSHBUTTON / CHECKBOX / PROGRESSBAR carry no data; an
                // unknown type never reaches here (rejected earlier)
                if !matches!(
                    window_type,
                    WndWindowType::User
                        | WndWindowType::TabPane
                        | WndWindowType::PushButton
                        | WndWindowType::CheckBox
                        | WndWindowType::ProgressBar
                ) {
                    return None; // the null data the construction would hit
                }
            }
        }

        // the composite draw-data routing (what the construction copies
        // into the sub-windows it builds)
        win.sub_draw = match window_type {
            WndWindowType::VertSlider | WndWindowType::HorzSlider => {
                vec![self.sub_draw_slot(WndSubSlot::Thumb)]
            }
            WndWindowType::ScrollListBox => vec![
                self.sub_draw_slot(WndSubSlot::UpButton),
                self.sub_draw_slot(WndSubSlot::DownButton),
                self.sub_draw_slot(WndSubSlot::Slider),
                self.sub_draw_slot(WndSubSlot::Thumb),
            ],
            WndWindowType::ComboBox => vec![
                self.sub_draw_slot(WndSubSlot::DropDownButton),
                self.sub_draw_slot(WndSubSlot::EditBox),
                self.sub_draw_slot(WndSubSlot::ListBox),
                self.sub_draw_slot(WndSubSlot::UpButton),
                self.sub_draw_slot(WndSubSlot::DownButton),
                self.sub_draw_slot(WndSubSlot::Slider),
                self.sub_draw_slot(WndSubSlot::Thumb),
            ],
            _ => Vec::new(),
        };

        self.arena.push(win);
        if let Some(p) = parent {
            self.arena[p].children.push(index);
        }
        Some(index)
    }

    fn sub_draw_slot(&self, slot: WndSubSlot) -> WndSubDraw {
        let (enabled, disabled, hilite) = match slot {
            WndSubSlot::Thumb => (
                &self.arrays.thumb[0],
                &self.arrays.thumb[1],
                &self.arrays.thumb[2],
            ),
            WndSubSlot::UpButton => (
                &self.arrays.up_button[0],
                &self.arrays.up_button[1],
                &self.arrays.up_button[2],
            ),
            WndSubSlot::DownButton => (
                &self.arrays.down_button[0],
                &self.arrays.down_button[1],
                &self.arrays.down_button[2],
            ),
            WndSubSlot::Slider => (
                &self.arrays.slider[0],
                &self.arrays.slider[1],
                &self.arrays.slider[2],
            ),
            WndSubSlot::DropDownButton => (
                &self.arrays.drop_down[0],
                &self.arrays.drop_down[1],
                &self.arrays.drop_down[2],
            ),
            WndSubSlot::EditBox => (
                &self.arrays.edit_box[0],
                &self.arrays.edit_box[1],
                &self.arrays.edit_box[2],
            ),
            WndSubSlot::ListBox => (
                &self.arrays.list_box[0],
                &self.arrays.list_box[1],
                &self.arrays.list_box[2],
            ),
        };
        WndSubDraw {
            slot,
            enabled: enabled.clone(),
            disabled: disabled.clone(),
            hilite: hilite.clone(),
        }
    }

    // --- the walks ---------------------------------------------------------

    /// `parseChildWindows` — the child loop: ENDALLCHILDREN/END break, the
    /// color defaults re-parse, WINDOW recurses; unknown tokens drop
    /// whole (their value tokens become subsequent tokens).
    fn parse_child_windows(&mut self, index: usize) -> R<bool> {
        // (the tab control's default-children destroy has no observable
        // here: the model never auto-creates the sub-panes)
        self.push_window(index);

        loop {
            let (ok, token) = self.scan_string();
            if !ok {
                break;
            }

            if token == b"ENDALLCHILDREN" {
                break;
            }
            if token == b"END" {
                break;
            }

            if token == b"ENABLEDCOLOR" {
                self.defaults.enabled_color = self.parse_default_color()?;
            } else if token == b"DISABLEDCOLOR" {
                self.defaults.disabled_color = self.parse_default_color()?;
            } else if token == b"HILITECOLOR" {
                self.defaults.hilite_color = self.parse_default_color()?;
            } else if token == b"SELECTEDCOLOR" {
                self.defaults.selected_color = self.parse_default_color()?;
            } else if token == b"TEXTCOLOR" {
                self.defaults.text_color = self.parse_default_color()?;
            } else if token == b"WINDOW" && self.parse_window()?.is_none() {
                return Ok(false);
            }
            // (no else: unknown child tokens drop whole)
        }

        // Pop the current window off the stack
        let last = self.pop_window();
        if last != Some(index) {
            // "parseChildWindows: unmatched window on stack"
            return Ok(false);
        }

        if self.arena[index].style & GWS_TAB_CONTROL != 0 {
            self.fixup_sub_pane_list(index)?;
        }

        Ok(true)
    }

    /// `parseWindow` — the WINDOWTYPE/SCREENRECT head, the field loop, the
    /// DATA/END/CHILD routing.
    fn parse_window(&mut self) -> R<Option<usize>> {
        let mut window: Option<usize> = None;
        let mut inst = InstData::default();
        // the defTextColor the window's text colors start from (defFont is
        // always null — the default font parse discards its text)
        let def_text = self.defaults.text_color;
        inst.text_colors.enabled = (def_text, def_text);
        inst.text_colors.disabled = (def_text, def_text);
        inst.text_colors.hilite = (def_text, def_text);

        let mut callbacks = CallbackStrings::default();

        // window type
        self.read_until_semicolon()?;
        let _ = self.strtok(Some(0), FIELD_SEPS); // the WINDOWTYPE label
        let __tok = self.strtok(None, FIELD_SEPS);
        let type_tok = self.require(__tok, "windowtype-token")?;
        let type_word: Vec<u8> = {
            // strlcpy into char[64]: truncate at 63 bytes
            let mut w = self.tok(type_tok).to_vec();
            w.truncate(MAX_WINDOW_NAME_LEN - 1);
            w
        };
        let window_type = WndWindowType::from_word(&type_word);

        // based on the window type get the data template
        let mut data = match window_type {
            Some(t) => self.take_data_template(t),
            None => DataPayload::None,
        };

        // position
        self.read_until_semicolon()?;
        let _ = self.strtok(Some(0), FIELD_SEPS); // the SCREENRECT label
        let (x, y, width, height) = self.parse_screen_rect()?;

        // parse all the field definitions
        loop {
            // get token — an EOF here spins the original's loop forever
            let (ok, token) = self.scan_string();
            if !ok {
                self.stop(WndVerdict::Hang("window-field-eof"))?;
            }

            if let Some(field) = field_dispatch(token.as_slice()) {
                // eat '='
                let _ = self.scan_string();
                self.read_until_semicolon()?;

                // the data-consuming fields write through `data`: a
                // USER/TABPANE/unknown window hands them the null template
                if matches!(
                    field,
                    Field::ListboxData
                        | Field::ComboBoxData
                        | Field::SliderData
                        | Field::RadioButtonData
                        | Field::StaticTextData
                        | Field::TextEntryData
                        | Field::TabControlData
                ) && matches!(data, DataPayload::None)
                {
                    self.stop(WndVerdict::Crash("null-data-template"))?;
                }

                let ok = match field {
                    Field::Name => {
                        self.parse_name(&mut inst)?;
                        true
                    }
                    Field::Status => {
                        self.parse_status(&mut inst);
                        true
                    }
                    Field::Style => {
                        self.parse_style(&mut inst);
                        true
                    }
                    Field::SystemCallback => {
                        callbacks.system = self.parse_callback_string()?;
                        true
                    }
                    Field::InputCallback => {
                        callbacks.input = self.parse_callback_string()?;
                        true
                    }
                    Field::TooltipCallback => {
                        callbacks.tooltip = self.parse_callback_string()?;
                        true
                    }
                    Field::DrawCallback => {
                        callbacks.draw = self.parse_callback_string()?;
                        true
                    }
                    Field::Font => {
                        self.parse_font(&mut inst)?;
                        true
                    }
                    Field::HeaderTemplate => {
                        self.parse_header_template(&mut inst)?;
                        true
                    }
                    Field::ListboxData => {
                        let mut d = take_listbox(&mut data);
                        let r = self.parse_listbox_data(&mut d);
                        put_listbox(&mut data, d);
                        r?;
                        true
                    }
                    Field::ComboBoxData => {
                        let mut d = take_combo(&mut data);
                        let r = self.parse_combo_box_data(&mut d);
                        put_combo(&mut data, d);
                        r?;
                        true
                    }
                    Field::SliderData => {
                        let mut d = take_slider(&mut data);
                        let r = self.parse_slider_data(&mut d);
                        put_slider(&mut data, d);
                        r?;
                        true
                    }
                    Field::RadioButtonData => {
                        let mut d = take_radio(&mut data);
                        let r = self.parse_radio_button_data(&mut d);
                        put_radio(&mut data, d);
                        r?;
                        true
                    }
                    Field::TooltipText => self.parse_tooltip_text(&mut inst)?,
                    Field::TooltipDelay => {
                        self.parse_tooltip_delay(&mut inst)?;
                        true
                    }
                    Field::Text => self.parse_text(&mut inst)?,
                    Field::TextColor => {
                        self.parse_text_color(&mut inst)?;
                        true
                    }
                    Field::StaticTextData => {
                        let mut d = take_text(&mut data);
                        let r = self.parse_static_text_data(&mut d);
                        put_text(&mut data, d);
                        r?;
                        true
                    }
                    Field::TextEntryData => {
                        let mut d = take_entry(&mut data);
                        let r = self.parse_text_entry_data(&mut d);
                        put_entry(&mut data, d);
                        r?;
                        true
                    }
                    Field::TabControlData => {
                        let mut d = take_tab(&mut data);
                        let r = self.parse_tab_control_data(&mut d);
                        put_tab(&mut data, d);
                        r?;
                        true
                    }
                    Field::ImageOffset => {
                        self.parse_image_offset(&mut inst)?;
                        true
                    }
                    Field::Tooltip => true, // the fixed translation is no parse state
                    Field::DrawData => {
                        self.parse_draw_data(&token, &mut inst)?;
                        true
                    }
                };

                if !ok {
                    // "parseGameObject: Error parsing %s" — FALSE fails the
                    // window
                    break;
                }
            } else if token == b"DATA" {
                // eat '='
                let _ = self.scan_string();
                self.read_until_semicolon()?;

                let ty = match window_type {
                    Some(t) => t,
                    None => {
                        // an unknown type's DATA dereferences nothing here
                        // (the unknown type already fails the creation)
                        WndWindowType::User
                    }
                };
                data = self.parse_data(ty)?;

                // DATA on TABCONTROL/COMBOBOX hands the gadget a null data
                // pointer — the original crashes in the construction that
                // follows
                if matches!(data, DataPayload::None)
                    && matches!(ty, WndWindowType::TabControl | WndWindowType::ComboBox)
                {
                    self.stop(WndVerdict::Crash("gadget-null-data"))?;
                }
            } else if token == b"END" {
                // (the header-template font swap resolves against the
                // template manager — the name is the observable; the font
                // request stands)
                if window.is_none() {
                    window = self.finish_window(
                        window_type,
                        &inst,
                        &data,
                        &callbacks,
                        x,
                        y,
                        width,
                        height,
                    );
                }
                break;
            } else if token == b"CHILD" {
                // Create a window using the current description
                window =
                    self.finish_window(window_type, &inst, &data, &callbacks, x, y, width, height);
                let Some(win_idx) = window else {
                    break; // the unknown type: nothing to parent children to
                };

                // Parses the CHILD's window info.
                if !self.parse_child_windows(win_idx)? {
                    // winDestroy(window) — the subtree drops out of the walk
                    window = None;
                    break;
                }
            } else {
                // Else it is unrecognized so eat associated data
                self.read_until_semicolon()?;
            }
        }

        Ok(window)
    }

    /// The creation call shared by the END and CHILD branches (the
    /// original's `createWindow`): `None` for an unknown WINDOWTYPE.
    #[allow(clippy::too_many_arguments)]
    fn finish_window(
        &mut self,
        window_type: Option<WndWindowType>,
        inst: &InstData,
        data: &DataPayload,
        callbacks: &CallbackStrings,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Option<usize> {
        let ty = window_type?;
        // winSetWindowId(id): every created window takes the instance
        // data's id
        self.create_window(
            ty,
            inst.id,
            x,
            y,
            width,
            height,
            inst,
            data.clone(),
            callbacks,
        )
    }

    /// `parseLayoutBlock` — the callbacks; unknown tokens are skipped
    /// WITHOUT consuming their value.
    fn parse_layout_block(&mut self) -> R<bool> {
        let (ok, token) = self.scan_string();
        if !ok {
            return Ok(false);
        }

        // better be the layout block
        if token != b"STARTLAYOUTBLOCK" {
            return Ok(false);
        }

        loop {
            // get next token — an EOF here spins the original's loop forever
            let (ok, token) = self.scan_string();
            if !ok {
                self.stop(WndVerdict::Hang("layout-eof"))?;
            }

            // check for end
            if token == b"ENDLAYOUTBLOCK" {
                break;
            }

            if token == b"LAYOUTINIT" {
                self.read_until_semicolon()?;
                let __tok = self.strtok(Some(0), LAYOUT_WORD_SEPS);
                let c = self.require(__tok, "layout-callback-token")?;
                let __tok = self.strtok(Some(c.0), LAYOUT_NAME_SEPS);
                let name = self.require(__tok, "layout-callback-token")?;
                self.init_name = self.tok(name).to_vec();
            } else if token == b"LAYOUTUPDATE" {
                self.read_until_semicolon()?;
                let __tok = self.strtok(Some(0), LAYOUT_WORD_SEPS);
                let c = self.require(__tok, "layout-callback-token")?;
                let __tok = self.strtok(Some(c.0), LAYOUT_NAME_SEPS);
                let name = self.require(__tok, "layout-callback-token")?;
                self.update_name = self.tok(name).to_vec();
            } else if token == b"LAYOUTSHUTDOWN" {
                self.read_until_semicolon()?;
                let __tok = self.strtok(Some(0), LAYOUT_WORD_SEPS);
                let c = self.require(__tok, "layout-callback-token")?;
                let __tok = self.strtok(Some(c.0), LAYOUT_NAME_SEPS);
                let name = self.require(__tok, "layout-callback-token")?;
                self.shutdown_name = self.tok(name).to_vec();
            }
            // (unknown layout tokens drop whole — their value tokens
            // follow as subsequent tokens)
        }

        Ok(true)
    }

    /// `winCreateFromScript` — the top-level walk.
    fn run(&mut self) -> R<()> {
        // Reset the window stack and defaults
        self.stack_ptr = 0;
        self.window_stack = [None; WIN_STACK_DEPTH];
        self.defaults = WndDefaults::default();

        // read the file version — the scanInt return is unchecked and
        // `version` is uninitialized in the original on its failure; the
        // port reads it as 0 (the v1 shape)
        self.read_none(15); // strlen("FILE_VERSION = ")
        let (ok, v) = self.scan_int();
        if ok {
            self.version = v as u32;
        }
        self.next_line();

        // version 2+ have a special block called the layout block
        if self.version >= 2 {
            match self.parse_layout_block() {
                Ok(true) => {}
                // "Error parsing layout block" — the load fails before any
                // window is read; a stop verdict ends the walk outright
                _ => return Ok(()),
            }
        } else {
            // default none names
            self.init_name = b"[None]".to_vec();
            self.update_name = b"[None]".to_vec();
            self.shutdown_name = b"[None]".to_vec();
        }

        loop {
            let (ok, token) = self.scan_string();
            if !ok {
                break;
            }

            if token == b"END" {
                continue;
            }

            // the default color blocks: a stop verdict inside one ends
            // the walk (the transcription's throw unwinds past the loop)
            if matches!(
                token.as_slice(),
                b"ENABLEDCOLOR"
                    | b"DISABLEDCOLOR"
                    | b"HILITECOLOR"
                    | b"SELECTEDCOLOR"
                    | b"TEXTCOLOR"
                    | b"BACKGROUNDCOLOR"
            ) {
                let c = self.parse_default_color()?;
                match token.as_slice() {
                    b"ENABLEDCOLOR" => self.defaults.enabled_color = c,
                    b"DISABLEDCOLOR" => self.defaults.disabled_color = c,
                    b"HILITECOLOR" => self.defaults.hilite_color = c,
                    b"SELECTEDCOLOR" => self.defaults.selected_color = c,
                    b"TEXTCOLOR" => self.defaults.text_color = c,
                    _ => self.defaults.background_color = c,
                }
                continue;
            }
            // the default font block (parsed and discarded — the no-op)
            if token == b"FONT" {
                self.defaults.font_text = Some(self.parse_default_font()?);
                continue;
            }
            if token == b"WINDOW" {
                // Parse window descriptions until the last END is read
                // (the return is NOT checked: a failed root pushes a
                // nullptr entry and the loop continues)
                match self.parse_window()? {
                    Some(idx) => self.roots.push(WndRoot::Window(idx)),
                    None => self.roots.push(WndRoot::Failed),
                }
            }
            // (no else: unknown top-level tokens drop whole)
        }
        Ok(())
    }

    /// `read(nullptr, n)` — the discarded seek.
    fn read_none(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.data.len());
    }
}

/// The callback strings a window's parse collects (the original's
/// theSystemString & friends, reset per window).
#[derive(Debug, Clone, Default)]
struct CallbackStrings {
    system: Vec<u8>,
    input: Vec<u8>,
    tooltip: Vec<u8>,
    draw: Vec<u8>,
}

// ---------------------------------------------------------------------------
// the field table (gameWindowFieldList)
// ---------------------------------------------------------------------------

/// The field-table entries the dispatch resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Status,
    Style,
    SystemCallback,
    InputCallback,
    TooltipCallback,
    DrawCallback,
    Font,
    HeaderTemplate,
    ListboxData,
    ComboBoxData,
    SliderData,
    RadioButtonData,
    TooltipText,
    TooltipDelay,
    Text,
    TextColor,
    StaticTextData,
    TextEntryData,
    TabControlData,
    ImageOffset,
    Tooltip,
    DrawData,
}

/// `gameWindowFieldList` — the token → field routing (the original's
/// table, as one match).
fn field_dispatch(token: &[u8]) -> Option<Field> {
    Some(match token {
        b"NAME" => Field::Name,
        b"STATUS" => Field::Status,
        b"STYLE" => Field::Style,
        b"SYSTEMCALLBACK" => Field::SystemCallback,
        b"INPUTCALLBACK" => Field::InputCallback,
        b"TOOLTIPCALLBACK" => Field::TooltipCallback,
        b"DRAWCALLBACK" => Field::DrawCallback,
        b"FONT" => Field::Font,
        b"HEADERTEMPLATE" => Field::HeaderTemplate,
        b"LISTBOXDATA" => Field::ListboxData,
        b"COMBOBOXDATA" => Field::ComboBoxData,
        b"SLIDERDATA" => Field::SliderData,
        b"RADIOBUTTONDATA" => Field::RadioButtonData,
        b"TOOLTIPTEXT" => Field::TooltipText,
        b"TOOLTIPDELAY" => Field::TooltipDelay,
        b"TEXT" => Field::Text,
        b"TEXTCOLOR" => Field::TextColor,
        b"STATICTEXTDATA" => Field::StaticTextData,
        b"TEXTENTRYDATA" => Field::TextEntryData,
        b"TABCONTROLDATA" => Field::TabControlData,
        b"ENABLEDDRAWDATA"
        | b"DISABLEDDRAWDATA"
        | b"HILITEDRAWDATA"
        | b"LISTBOXENABLEDUPBUTTONDRAWDATA"
        | b"LISTBOXENABLEDDOWNBUTTONDRAWDATA"
        | b"LISTBOXENABLEDSLIDERDRAWDATA"
        | b"LISTBOXDISABLEDUPBUTTONDRAWDATA"
        | b"LISTBOXDISABLEDDOWNBUTTONDRAWDATA"
        | b"LISTBOXDISABLEDSLIDERDRAWDATA"
        | b"LISTBOXHILITEUPBUTTONDRAWDATA"
        | b"LISTBOXHILITEDOWNBUTTONDRAWDATA"
        | b"LISTBOXHILITESLIDERDRAWDATA"
        | b"SLIDERTHUMBENABLEDDRAWDATA"
        | b"SLIDERTHUMBDISABLEDDRAWDATA"
        | b"SLIDERTHUMBHILITEDRAWDATA"
        | b"COMBOBOXDROPDOWNBUTTONENABLEDDRAWDATA"
        | b"COMBOBOXDROPDOWNBUTTONDISABLEDDRAWDATA"
        | b"COMBOBOXDROPDOWNBUTTONHILITEDRAWDATA"
        | b"COMBOBOXEDITBOXENABLEDDRAWDATA"
        | b"COMBOBOXEDITBOXDISABLEDDRAWDATA"
        | b"COMBOBOXEDITBOXHILITEDRAWDATA"
        | b"COMBOBOXLISTBOXENABLEDDRAWDATA"
        | b"COMBOBOXLISTBOXDISABLEDDRAWDATA"
        | b"COMBOBOXLISTBOXHILITEDRAWDATA" => Field::DrawData,
        b"IMAGEOFFSET" => Field::ImageOffset,
        b"TOOLTIP" => Field::Tooltip,
        _ => return None,
    })
}

// the data-payload take/put helpers (the original's `data` pointer
// aliasing the shared statics)
fn take_listbox(d: &mut DataPayload) -> WndListboxData {
    match d {
        DataPayload::Listbox(x) => x.clone(),
        _ => WndListboxData::default(),
    }
}
fn put_listbox(d: &mut DataPayload, v: WndListboxData) {
    if matches!(d, DataPayload::Listbox(_)) {
        *d = DataPayload::Listbox(v);
    }
}
fn take_combo(d: &mut DataPayload) -> WndComboBoxData {
    match d {
        DataPayload::ComboBox(x) => x.clone(),
        _ => WndComboBoxData::default(),
    }
}
fn put_combo(d: &mut DataPayload, v: WndComboBoxData) {
    if matches!(d, DataPayload::ComboBox(_)) {
        *d = DataPayload::ComboBox(v);
    }
}
fn take_slider(d: &mut DataPayload) -> WndSliderData {
    match d {
        DataPayload::Slider(x) => *x,
        _ => WndSliderData::default(),
    }
}
fn put_slider(d: &mut DataPayload, v: WndSliderData) {
    if matches!(d, DataPayload::Slider(_)) {
        *d = DataPayload::Slider(v);
    }
}
fn take_radio(d: &mut DataPayload) -> WndRadioButtonData {
    match d {
        DataPayload::Radio(x) => *x,
        _ => WndRadioButtonData::default(),
    }
}
fn put_radio(d: &mut DataPayload, v: WndRadioButtonData) {
    if matches!(d, DataPayload::Radio(_)) {
        *d = DataPayload::Radio(v);
    }
}
fn take_text(d: &mut DataPayload) -> WndTextData {
    match d {
        DataPayload::Text(x) => *x,
        _ => WndTextData::default(),
    }
}
fn put_text(d: &mut DataPayload, v: WndTextData) {
    if matches!(d, DataPayload::Text(_)) {
        *d = DataPayload::Text(v);
    }
}
fn take_entry(d: &mut DataPayload) -> WndEntryData {
    match d {
        DataPayload::Entry(x) => *x,
        _ => WndEntryData::default(),
    }
}
fn put_entry(d: &mut DataPayload, v: WndEntryData) {
    if matches!(d, DataPayload::Entry(_)) {
        *d = DataPayload::Entry(v);
    }
}
fn take_tab(d: &mut DataPayload) -> WndTabControlData {
    match d {
        DataPayload::Tab(x) => x.clone(),
        _ => WndTabControlData::default(),
    }
}
fn put_tab(d: &mut DataPayload, v: WndTabControlData) {
    if matches!(d, DataPayload::Tab(_)) {
        *d = DataPayload::Tab(v);
    }
}

// ---------------------------------------------------------------------------
// the entry point
// ---------------------------------------------------------------------------

/// Parse a `.wnd` window layout (the `winCreateFromScript` load path) at
/// the given current display resolution.
///
/// The load never panics and never fails with an error: the result keeps
/// every partial state and a [`WndVerdict`] stating how the walk ended
/// (the hang/crash/oob words state where the original leaves defined
/// behaviour; see the module docs). [`WndLayout::loaded`] is
/// `WindowLayout::load`'s success test.
#[must_use]
pub fn parse_wnd(bytes: &[u8], display: WndDisplay) -> WndLayout {
    let mut p = Parser::new(bytes, display);
    let _ = p.run(); // a stop verdict is recorded, not an error
    let verdict = p.verdict.unwrap_or({
        if p.roots.iter().any(|r| matches!(r, WndRoot::Window(_))) {
            WndVerdict::Ok
        } else {
            WndVerdict::LoadFailed
        }
    });
    WndLayout {
        verdict,
        version: p.version,
        init_name: p.init_name,
        update_name: p.update_name,
        shutdown_name: p.shutdown_name,
        defaults: p.defaults,
        roots: p.roots,
        arena: p.arena,
        name_keys: p.keys,
        stream_pos: p.pos,
        stream_len: bytes.len(),
        display,
    }
}
