// SPDX-License-Identifier: GPL-3.0-or-later
//
// WND golden vector generator — a verbatim transcription of the Zero Hour
// window-layout (.wnd) load path at baseline 571a7c5:
//
//   Core/GameEngine/Source/GameClient/GUI/GameWindowManagerScript.cpp
//     (GameWindowManager::winCreateFromScript — the version read, the
//     version-2 layout block, the default color/font blocks and the
//     WINDOW loop; parseLayoutBlock with parseInit/parseUpdate/
//     parseShutdown; parseWindow with the WINDOWTYPE/SCREENRECT head, the
//     gameWindowFieldList walk and the DATA/END/CHILD routing;
//     parseChildWindows with the window stack; createWindow/createGadget
//     with their style-bit routing, the radio-button screen id and the
//     composite-gadget draw-data arrays; parseData — the obsolete DATA
//     field; every field parser: parseName, parseStatus, parseStyle, the
//     four callback parsers, parseFont, parseHeaderTemplate,
//     parseListboxData, parseComboBoxData, parseSliderData,
//     parseRadioButtonData, parseTooltipText, parseTooltipDelay, parseText,
//     parseTextColor, parseStaticTextData, parseTextEntryData,
//     parseTabControlData, parseDrawData, parseImageOffset, parseTooltip,
//     parseDefaultColor, parseDefaultFont, parseColor, parseBitString,
//     parseBitFlag, readUntilSemicolon, scanBool/scanShort/scanInt/
//     scanUnsignedInt)
//   Core/GameEngine/Source/Common/System/RAMFile.cpp
//     (RAMFile::read, RAMFile::scanInt, RAMFile::scanString,
//     RAMFile::nextLine — the tokenizing primitives the whole walk drives)
//   Core/GameEngine/Source/GameClient/GUI/WinInstanceData.cpp
//     (WinInstanceData::init — the per-window defaults the parse starts
//     from, including the WIN_COLOR_UNDEFINED sentinels)
//   Core/GameEngine/Source/GameClient/GUI/GameWindow.cpp
//     (GameWindow::winGetScreenPosition — the ancestor-summing walk
//     parseScreenRect resolves parent-relative coordinates through)
//   Core/GameEngine/Source/GameClient/GUI/GameWindowGlobal.cpp
//     (GameWindowManager::winMakeColor — the GameMakeColor pack)
//   Core/GameEngine/Include/GameClient/Color.h (GameMakeColor)
//   Core/GameEngine/Include/GameClient/GameWindow.h
//     (WIN_STATUS_* flag values, WIN_COLOR_UNDEFINED)
//   Core/GameEngine/Include/GameClient/Gadget.h
//     (GWS_* style values, SliderData, EntryData, TextData, ListboxData,
//     ComboBoxData, RadioButtonData, TabControlData, NUM_TAB_PANES)
//   Core/GameEngine/Include/GameClient/WinInstanceData.h
//     (WinDrawData, TextDrawData, MAX_DRAW_DATA = 9, MAX_TEXT_LABEL = 128,
//     MAX_WINDOW_NAME_LEN = 64)
//   Core/GameEngine/Include/GameClient/GameWindowManager.h
//     (WindowLayoutInfo, WindowLayoutCurrentVersion = 2)
//   Core/GameEngine/Source/Common/NameKeyGenerator.cpp
//     (nameToKey — the first-seen sequential interning the window ids and
//     the radio screen ids mint through)
//   GeneralsMD/Code/Tools/GUIEdit/Source/Save.cpp
//     (the writer side, consulted for the real file shapes: the
//     multi-line SCREENRECT and draw-data stanzas the collapsing reader
//     re-joins, the NAME "filename:control" decoration, the status/style
//     '+' lists, the TEXTCOLOR 3-state stanza)
//
// The transcription models the environment singletons the parse consults
// as explicit state (the same policy as the previous slices):
//
// - TheDisplay: the current resolution is a per-corpus parameter (the
//   SCREENRECT scaling divides by CREATIONRESOLUTION against it).
// - TheNameKeyGenerator: a fresh table per load. The original pre-seeds
//   the process-global generator with the whole WellKnownKeys table at
//   startup, so its window-name ids depend on load order across the whole
//   process; the fresh-table contract is the same documented divergence
//   the map slices already carry (map_chunk's NameKeys).
// - TheMappedImageCollection / TheFontLibrary / TheFunctionLexicon /
//   TheHeaderTemplateManager / TheGameText: the parse's only observables
//   through these are the names it hands them. The transcription records
//   the image name (or NoImage), the font request (name/size/bold) and
//   the callback/template/label strings; the resolutions are the later
//   client-layer slices (fonts are their own plan item).
// - The static read buffer (WIN_BUFFER_LENGTH = 2048) and the static
//   gadget-data templates: the original's are process globals that
//   survive across loads; the transcription (and the port) resets them
//   per load so the corpora stay independent — the cross-load
//   contamination the original exhibits is out of scope and documented.
//
// Where the original leaves defined behaviour (an unchecked EOF that
// spins forever, a strtok nullptr that strcmp/atoi/strlen would
// dereference, an out-of-bounds pane write), the transcription stops
// before the undefined act and records a verdict:
//
//   hang:<site>    the original's loop spins forever at EOF
//   crash:<site>   the original dereferences a null token / walks a
//                  quote scan off the buffer end
//   oob:<site>     the original writes past a fixed array
//
// The load otherwise never fails mid-walk in the original's release
// build: a field parser returning FALSE fails the window (and cascades
// through parseChildWindows), and a missing layout block fails the load
// before any window is read. Those are recorded as root failure entries
// and the load-failed verdict, not as aborts.
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
//
// Original: Copyright 2025 Electronic Arts Inc.
// Rust port Copyright 2026 the RustZeroHour contributors.
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#include <cctype>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <list>
#include <string>
#include <vector>

typedef bool Bool;
typedef signed char Int8;
typedef unsigned char UnsignedByte;
typedef unsigned char Byte;
typedef short Short;
typedef unsigned short UnsignedShort;
typedef int Int;
typedef unsigned int UnsignedInt;
typedef float Real;
typedef uint32_t Color;

// WinInstanceData.h: MAX_DRAW_DATA = 9, MAX_TEXT_LABEL = 128.
enum { WIN_BUFFER_LENGTH = 2048 };
enum { MAX_DRAW_DATA = 9 };
enum { MAX_TEXT_LABEL = 128 };
enum { MAX_WINDOW_NAME_LEN = 64 };
enum { WIN_STACK_DEPTH = 10 };
enum { NUM_TAB_PANES = 8 };

// GameWindow.h: WIN_COLOR_UNDEFINED = GAME_COLOR_UNDEFINED = 0x00FFFFFF.
enum { WIN_COLOR_UNDEFINED = 0x00FFFFFF };

// Color.h: GameMakeColor.
static inline Color GameMakeColor(UnsignedByte red, UnsignedByte green, UnsignedByte blue,
                                   UnsignedByte alpha)
{
        return (Color)(((uint32_t)alpha << 24) | ((uint32_t)red << 16) | ((uint32_t)green << 8) |
                       (uint32_t)blue);
}

// GameWindowGlobal.cpp: GameWindowManager::winMakeColor is this pack.
static inline Color winMakeColor(UnsignedByte red, UnsignedByte green, UnsignedByte blue,
                                  UnsignedByte alpha)
{
        return GameMakeColor(red, green, blue, alpha);
}

// ---------------------------------------------------------------------------
// the verdict recorder (the sites where the original leaves defined
// behaviour — see the file header)
// ---------------------------------------------------------------------------

struct Verdict {
        std::string word; // "", or "hang:...", "crash:...", "oob:..."
};

static Verdict g_verdict;
static bool g_layout_reached = false;   // the version read + (any) layout half finished
static bool g_defaults_reached = false; // the defaults line is dumpable

// The one "throw" of the transcription: stop the walk exactly where the
// original stops being defined.
struct WalkStop {};

static void stop(const std::string &word)
{
        if (g_verdict.word.empty())
                g_verdict.word = word;
        throw WalkStop();
}

// ---------------------------------------------------------------------------
// NameKeyGenerator (NameKeyGenerator.cpp, fresh per load — see header)
// ---------------------------------------------------------------------------

typedef UnsignedInt NameKeyType;

struct NameKeyGenerator {
        NameKeyGenerator() : m_nextID(1) {}

        NameKeyType nameToKey(const std::string &name)
        {
                for (size_t i = 0; i < m_names.size(); ++i)
                        if (name == m_names[i].first)
                                return m_names[i].second;
                NameKeyType key = m_nextID++;
                m_names.push_back(std::make_pair(name, key));
                return key;
        }

        std::vector<std::pair<std::string, NameKeyType>> m_names;
        NameKeyType m_nextID;
};

// ---------------------------------------------------------------------------
// RAMFile (RAMFile.cpp) — the read/scan primitives, verbatim
// ---------------------------------------------------------------------------

struct RamFile {
        const unsigned char *m_data;
        size_t m_size;
        size_t m_pos;

        RamFile(const unsigned char *data, size_t size) : m_data(data), m_size(size), m_pos(0) {}

        // RAMFile::read — a null destination just advances (the version
        // read's read(nullptr, 15) seek).
        Int read(void *buffer, Int bytes)
        {
                Int bytesLeft = (Int)(m_size - m_pos);
                if (bytes > bytesLeft)
                        bytes = bytesLeft;
                if (bytes > 0 && buffer != nullptr)
                        memcpy(buffer, &m_data[m_pos], bytes);
                m_pos += bytes;
                return bytes;
        }

        // RAMFile::scanInt — skip to a digit or '-', then eat the digit
        // run; atoi over the collected bytes.
        Bool scanInt(Int &newInt)
        {
                newInt = 0;
                std::string tempstr;

                while ((m_pos < m_size) && ((m_data[m_pos] < '0') || (m_data[m_pos] > '9')) &&
                       (m_data[m_pos] != '-')) {
                        ++m_pos;
                }

                if (m_pos >= m_size) {
                        m_pos = m_size;
                        return false;
                }

                do {
                        tempstr.push_back((char)m_data[m_pos]);
                        ++m_pos;
                } while ((m_pos < m_size) && ((m_data[m_pos] >= '0') && (m_data[m_pos] <= '9')));

                newInt = atoi(tempstr.c_str());
                return true;
        }

        // RAMFile::scanString — skip whitespace, then eat until
        // whitespace. The empty token at EOF is a real parse state (the
        // callers that ignore the return still compare it).
        Bool scanString(std::string &newString)
        {
                newString.clear();

                while ((m_pos < m_size) && isspace(m_data[m_pos])) {
                        ++m_pos;
                }

                if (m_pos >= m_size) {
                        m_pos = m_size;
                        return false;
                }

                do {
                        newString.push_back((char)m_data[m_pos]);
                        ++m_pos;
                } while ((m_pos < m_size) && (!isspace(m_data[m_pos])));

                return true;
        }

        // RAMFile::nextLine — seek past the next '\n' (the buffer copy
        // half is dead at the call site the version read drives).
        void nextLine()
        {
                while ((m_pos < m_size) && (m_data[m_pos] != '\n')) {
                        ++m_pos;
                }
                if (m_pos < m_size) {
                        ++m_pos;
                }
                if (m_pos >= m_size) {
                        m_pos = m_size;
                }
        }
};

// ---------------------------------------------------------------------------
// the window model (GameWindow + WinInstanceData, the parse-relevant
// halves)
// ---------------------------------------------------------------------------

// WinInstanceData.h: WinDrawData / TextDrawData. The image is the
// requested NAME (the original stores the resolved Image* — the
// collection is environment state; the name is the parse observable),
// owned here because the strtok token it comes from lives in the reused
// read buffer.
struct WinDrawData {
        std::string image; // empty == the null image (NoImage / unset)
        Color color;
        Color borderColor;
};

struct TextDrawData {
        Color color;
        Color borderColor;
};

// Gadget.h: the gadget data structs, the saved members only.
struct SliderData {
        Int minVal;
        Int maxVal;
};

struct EntryData {
        Short maxTextLen;
        Bool secretText;
        Bool numericalOnly;
        Bool alphaNumericalOnly;
        Bool aSCIIOnly;
};

struct TextData {
        Bool centered;
        Bool centeredVertically;
        Int leftMargin;
        Int topMargin;
};

struct ListboxData {
        Short listLength;
        Short columns;
        std::vector<Int> columnWidthPercentage; // empty == the null pointer
        Bool autoScroll;
        Bool autoPurge;
        Bool scrollBar;
        Bool multiSelect;
        Bool forceSelect;
        Bool scrollIfAtEnd;
};

struct ComboBoxData {
        Bool isEditable;
        Int maxDisplay;
        Int maxChars;
        Bool asciiOnly;
        Bool lettersAndNumbersOnly;
        // the constructed halves createGadget mints (the NEW EntryData /
        // ListboxData with their fixed starts and the copied gates)
        EntryData entryData;
        ListboxData listboxData;
        Int entryCount;
};

struct RadioButtonData {
        Int screen;
        Int group;
};

struct TabControlData {
        Int tabOrientation;
        Int tabEdge;
        Int tabWidth;
        Int tabHeight;
        Int tabCount;
        Int subPanes[NUM_TAB_PANES]; // window indices, -1 == the null pane
        Bool subPaneDisabled[NUM_TAB_PANES];
        Int paneBorder;
};

// WinInstanceData, the parse-relevant members.
struct WinInstanceData {
        WinDrawData m_enabledDrawData[MAX_DRAW_DATA];
        WinDrawData m_disabledDrawData[MAX_DRAW_DATA];
        WinDrawData m_hiliteDrawData[MAX_DRAW_DATA];
        TextDrawData m_enabledText;
        TextDrawData m_disabledText;
        TextDrawData m_hiliteText;
        Int m_id;
        UnsignedInt m_state;
        UnsignedInt m_style;
        UnsignedInt m_status;
        Int m_owner; // window index, -1 == null
        std::string m_textLabelString;
        std::string m_tooltipString;
        Int m_tooltipDelay;
        std::string m_decoratedNameString;
        Int m_imageOffsetX;
        Int m_imageOffsetY;
        // the font request (parseFont's getFont call resolves in the
        // original; the request is the parse observable)
        Bool m_hasFont;
        std::string m_fontName;
        Int m_fontSize;
        Int m_fontBold;
        std::string m_headerTemplateName;

        // WinInstanceData::init, verbatim (the display-string members are
        // not parse observables; the font pointer is the request above).
        void init()
        {
                for (Int i = 0; i < MAX_DRAW_DATA; i++) {
                        m_enabledDrawData[i].image.clear();
                        m_enabledDrawData[i].color = WIN_COLOR_UNDEFINED;
                        m_enabledDrawData[i].borderColor = WIN_COLOR_UNDEFINED;
                        m_disabledDrawData[i].image.clear();
                        m_disabledDrawData[i].color = WIN_COLOR_UNDEFINED;
                        m_disabledDrawData[i].borderColor = WIN_COLOR_UNDEFINED;
                        m_hiliteDrawData[i].image.clear();
                        m_hiliteDrawData[i].color = WIN_COLOR_UNDEFINED;
                        m_hiliteDrawData[i].borderColor = WIN_COLOR_UNDEFINED;
                }
                m_enabledText.color = WIN_COLOR_UNDEFINED;
                m_enabledText.borderColor = WIN_COLOR_UNDEFINED;
                m_disabledText.color = WIN_COLOR_UNDEFINED;
                m_disabledText.borderColor = WIN_COLOR_UNDEFINED;
                m_hiliteText.color = WIN_COLOR_UNDEFINED;
                m_hiliteText.borderColor = WIN_COLOR_UNDEFINED;
                m_id = 0;
                m_state = 0;
                m_style = 0;
                m_status = 0; // WIN_STATUS_NONE
                m_owner = -1;
                m_textLabelString.clear();
                m_tooltipString.clear();
                m_tooltipDelay = -1;
                m_decoratedNameString.clear();
                m_imageOffsetX = 0;
                m_imageOffsetY = 0;
                m_hasFont = false;
                m_fontName.clear();
                m_fontSize = 0;
                m_fontBold = 0;
                m_headerTemplateName.clear();
        }
};

// The gadget data a window carries after creation: which template it
// snapped (a copy — the original hands the gadget the shared static, and
// the next window's getDataTemplate memset would wipe it; the copy is the
// value the created gadget reads, documented in the port).
enum GadgetType {
        GADGET_NONE,
        GADGET_SLIDER,
        GADGET_LISTBOX,
        GADGET_COMBOBOX,
        GADGET_ENTRY,
        GADGET_STATICTEXT,
        GADGET_RADIO,
        GADGET_TABCONTROL,
};

struct Gadget {
        GadgetType kind;
        SliderData slider;
        ListboxData listbox;
        ComboBoxData combobox;
        EntryData entry;
        TextData text;
        RadioButtonData radio;
        TabControlData tab;
};

// the composite-gadget draw data a window's construction routes in
// (createGadget copies the static arrays into the sub-windows it builds;
// the snapshot is the parse observable — see the port docs)
struct RoutedDraw {
        const char *slot;
        WinDrawData enabled[MAX_DRAW_DATA];
        WinDrawData disabled[MAX_DRAW_DATA];
        WinDrawData hilite[MAX_DRAW_DATA];
};

struct GameWindow {
        std::string type; // the WINDOWTYPE word
        Int x, y, width, height; // region lo + size, parent-relative
        Int index; // the arena id (dump identity)
        Int parent; // arena index, -1 == none
        std::vector<Int> children; // arena indices in creation order
        WinInstanceData inst;
        Gadget gadget;
        std::vector<RoutedDraw> routed;
        // the callback strings the editor data saves (the original keeps
        // the function pointers too; the strings are the parse state)
        std::string systemCallbackString;
        std::string inputCallbackString;
        std::string tooltipCallbackString;
        std::string drawCallbackString;

        GameWindow() : x(0), y(0), width(0), height(0), index(-1), parent(-1) {}
};

// GameWindow::winGetScreenPosition — the ancestor summing walk
// (transcribed over the arena).
static void winGetScreenPosition(const std::vector<GameWindow *> &arena, const GameWindow *w,
                                  Int *x, Int *y)
{
        *x = w->x;
        *y = w->y;
        Int p = w->parent;
        while (p != -1) {
                const GameWindow *parent = arena[p];
                *x += parent->x;
                *y += parent->y;
                p = parent->parent;
        }
}

// ---------------------------------------------------------------------------
// the load state (the file-scope statics of GameWindowManagerScript.cpp,
// fresh per load — see the file header)
// ---------------------------------------------------------------------------

struct LoadState {
        RamFile *inFile;
        std::vector<GameWindow *> arena; // every created window
        GameWindow *windowStack[WIN_STACK_DEPTH];
        GameWindow **stackPtr;

        // default visual properties (resetWindowDefaults)
        Color defEnabledColor;
        Color defDisabledColor;
        Color defBackgroundColor;
        Color defHiliteColor;
        Color defSelectedColor;
        Color defTextColor;
        std::string defFontText; // the discarded default-font stanza text

        // the static gadget templates (getDataTemplate's statics — each
        // handed out memset-zeroed)
        SliderData tSlider;
        ListboxData tListbox;
        EntryData tEntry;
        TextData tText;
        RadioButtonData tRadio;
        TabControlData tTab;
        ComboBoxData tCombo;

        // parseData's own statics (the DATA field path — statictext and
        // radio are NOT memset there in the original)
        SliderData dSlider;
        ListboxData dListbox;
        EntryData dEntry;
        TextData dText;
        RadioButtonData dRadio;

        // the composite-gadget draw data arrays (the file-scope WinDrawData
        // arrays the LISTBOX*/COMBOBOX*/SLIDERTHUMB* fields fill and
        // createGadget routes into the constructed sub-windows; the parse
        // observable is the array itself)
        WinDrawData enabledDropDownButtonDrawData[MAX_DRAW_DATA];
        WinDrawData disabledDropDownButtonDrawData[MAX_DRAW_DATA];
        WinDrawData hiliteDropDownButtonDrawData[MAX_DRAW_DATA];
        WinDrawData enabledEditBoxDrawData[MAX_DRAW_DATA];
        WinDrawData disabledEditBoxDrawData[MAX_DRAW_DATA];
        WinDrawData hiliteEditBoxDrawData[MAX_DRAW_DATA];
        WinDrawData enabledListBoxDrawData[MAX_DRAW_DATA];
        WinDrawData disabledListBoxDrawData[MAX_DRAW_DATA];
        WinDrawData hiliteListBoxDrawData[MAX_DRAW_DATA];
        WinDrawData enabledUpButtonDrawData[MAX_DRAW_DATA];
        WinDrawData disabledUpButtonDrawData[MAX_DRAW_DATA];
        WinDrawData hiliteUpButtonDrawData[MAX_DRAW_DATA];
        WinDrawData enabledDownButtonDrawData[MAX_DRAW_DATA];
        WinDrawData disabledDownButtonDrawData[MAX_DRAW_DATA];
        WinDrawData hiliteDownButtonDrawData[MAX_DRAW_DATA];
        WinDrawData enabledSliderDrawData[MAX_DRAW_DATA];
        WinDrawData disabledSliderDrawData[MAX_DRAW_DATA];
        WinDrawData hiliteSliderDrawData[MAX_DRAW_DATA];
        WinDrawData enabledSliderThumbDrawData[MAX_DRAW_DATA];
        WinDrawData disabledSliderThumbDrawData[MAX_DRAW_DATA];
        WinDrawData hiliteSliderThumbDrawData[MAX_DRAW_DATA];

        // the persistent read buffer (readUntilSemicolon's window onto the
        // file — its stale bytes at EOF are load state)
        unsigned char buffer[WIN_BUFFER_LENGTH];

        // the display (TheDisplay)
        Int displayWidth;
        Int displayHeight;

        NameKeyGenerator keys;

        // WindowLayoutInfo
        UnsignedInt version;
        std::string initNameString;
        std::string updateNameString;
        std::string shutdownNameString;
        std::vector<int> roots; // arena indices; -1 == the nullptr push of
                                // a failed root parse

        LoadState(RamFile *f, Int dw, Int dh)
            : inFile(f), stackPtr(windowStack), defEnabledColor(0), defDisabledColor(0),
              defBackgroundColor(0), defHiliteColor(0), defSelectedColor(0), defTextColor(0),
              displayWidth(dw), displayHeight(dh), version(0)
        {
                memset(windowStack, 0, sizeof(windowStack));
                memset(buffer, 0, sizeof(buffer));
                zeroStatics();
        }

        void zeroStatics()
        {
                memset(&tSlider, 0, sizeof(tSlider));
                tListbox = {};
                memset(&tEntry, 0, sizeof(tEntry));
                memset(&tText, 0, sizeof(tText));
                memset(&tRadio, 0, sizeof(tRadio));
                memset(&tTab, 0, sizeof(tTab));
                tCombo = {};
                // parseData's statics start zero too (file-scope statics
                // are zero-initialized; only their never-written members
                // stay stale across uses within one load)
                memset(&dSlider, 0, sizeof(dSlider));
                dListbox = {};
                memset(&dEntry, 0, sizeof(dEntry));
                memset(&dText, 0, sizeof(dText));
                memset(&dRadio, 0, sizeof(dRadio));
                for (int z = 0; z < MAX_DRAW_DATA; z++) enabledDropDownButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) disabledDropDownButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) hiliteDropDownButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) enabledEditBoxDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) disabledEditBoxDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) hiliteEditBoxDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) enabledListBoxDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) disabledListBoxDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) hiliteListBoxDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) enabledUpButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) disabledUpButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) hiliteUpButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) enabledDownButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) disabledDownButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) hiliteDownButtonDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) enabledSliderDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) disabledSliderDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) hiliteSliderDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) enabledSliderThumbDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) disabledSliderThumbDrawData[z] = {};
                for (int z = 0; z < MAX_DRAW_DATA; z++) hiliteSliderThumbDrawData[z] = {};
        }
};

static LoadState *g_state = nullptr;

// ---------------------------------------------------------------------------
// the small scan/parse helpers (GameWindowManagerScript.cpp, verbatim)
// ---------------------------------------------------------------------------

// strlcpy (the codebase's truncating copy), renamed to keep glibc's own declaration out of the way.
static size_t rzh_strlcpy(char *dst, const char *src, size_t size)
{
        size_t len = strlen(src);
        if (size > 0) {
                size_t n = len < size - 1 ? len : size - 1;
                memcpy(dst, src, n);
                dst[n] = '\0';
        }
        return len;
}

static Int stricmp(const char *a, const char *b) { return strcasecmp(a, b); }

static Int scanBool(const char *source, Bool &val)
{
        Int temp = 0;
        Int ret = sscanf(source, "%d", &temp);
        val = (Bool)temp;
        return ret;
}

static Int scanShort(const char *source, Short &val)
{
        Int temp = 0;
        Int ret = sscanf(source, "%d", &temp);
        val = (Short)temp;
        return ret;
}

static Int scanInt(const char *source, Int &val) { return sscanf(source, "%d", &val); }

static Int scanUnsignedInt(const char *source, UnsignedInt &val)
{
        // the original sscanf's a %d into an UnsignedInt& — the same
        // reinterpreting read glibc performs
        Int temp = 0;
        Int ret = sscanf(source, "%d", &temp);
        memcpy(&val, &temp, sizeof(val));
        return ret;
}

// readUntilSemicolon — verbatim, including the persistent-buffer walk at
// EOF (a read of 0 bytes leaves buffer[i] as the stale byte) and the
// leading-whitespace skip. One EOF shape spins in the original: when only
// whitespace has been seen (start) and the stale byte is whitespace, the
// loop never advances i and never re-reads a fresh byte — the
// transcription stops with the hang verdict there (every other stale byte
// advances i, so those walks stay bounded and read the stale bytes the
// original reads).
static void readUntilSemicolon(RamFile *fp, char *buf, int maxBufLen)
{
        int i = 0;
        Bool start = true;

        while (i < maxBufLen) {
                Int got = fp->read(buf + i, 1);

                if (isspace((unsigned char)buf[i])) {
                        if (start == false) {
                                buf[i++] = ' ';
                        } else if (got == 0) {
                                // the stale leading whitespace at EOF: the
                                // original spins forever
                                stop("hang:readuntil-eof");
                        }
                } else {
                        start = false;
                        if (buf[i] == ';') {
                                buf[i] = '\000';
                                return;
                        }
                        i++;
                }
        }

        // "Read buffer overflow - input truncated" (DEBUG_LOG only)
        buf[maxBufLen - 1] = '\000';
}

// the token-walk guard: the original dereferences the null strtok result
// at every site below; the transcription stops with the crash verdict
// instead. Every call keeps the original's call shape.
static const char *tokOrNull(const char *c, const char *site)
{
        if (c == nullptr)
                stop(std::string("crash:") + site);
        return c;
}

// the quote-scan guard: the original's `while (*ptr != '"') ptr++` walks
// past the buffer's end when no quote exists; the transcription stops at
// the array end with the crash verdict.
static char *scanToQuote(char *buffer, const char *site)
{
        char *ptr = buffer;
        while (*ptr != '"') {
                ptr++;
                if (ptr >= buffer + WIN_BUFFER_LENGTH)
                        stop(std::string("crash:") + site);
        }
        return ptr;
}

// parseBitFlag — stricmp over the flag list, setting the bit.
static Bool parseBitFlag(const char *flagString, UnsignedInt *bits, const char *const *flagList)
{
        for (int i = 0; const char *c = flagList[i]; i++) {
                if (stricmp(c, flagString) == 0) {
                        *bits |= (1u << i);
                        return true;
                }
        }
        return false;
}

// parseBitString — 'A+B+C' over a 256-byte copy; the NULL check only
// compares the first four bytes.
static void parseBitString(const char *inBuffer, UnsignedInt *bits, const char *const *flagList)
{
        char buffer[256];
        char *tok;

        rzh_strlcpy(buffer, inBuffer, sizeof(buffer));

        if (strncmp(buffer, "NULL", 4) != 0) {
                for (tok = strtok(buffer, "+"); tok; tok = strtok(nullptr, "+")) {
                        if (!parseBitFlag(tok, bits, flagList)) {
                                // "ParseBitString: Invalid flag '%s'." (DEBUG_LOG only)
                        }
                }
        }
}

// parseColor — atoi into Bytes (the truncation is real), packed opaque.
static Bool parseColor(Color *color, char *buffer)
{
        char *c;

        c = strtok(buffer, " \t\n\r");
        Byte red = (Byte)atoi(tokOrNull(c, "color-token"));

        c = strtok(nullptr, " \t\n\r");
        Byte green = (Byte)atoi(tokOrNull(c, "color-token"));

        c = strtok(nullptr, " \t\n\r");
        Byte blue = (Byte)atoi(tokOrNull(c, "color-token"));

        *color = winMakeColor(red, green, blue, 255);

        return true;
}

// parseDefaultColor — the '=' eat is a whole scanString token.
static Bool parseDefaultColor(Color *color, RamFile *inFile, char *buffer)
{
        std::string str;
        inFile->scanString(str);

        readUntilSemicolon(inFile, buffer, WIN_BUFFER_LENGTH);

        if (strcmp(buffer, "TRANSPARENT") == 0) {
                *color = WIN_COLOR_UNDEFINED;
        } else {
                parseColor(color, buffer);
        }

        return true;
}

// parseDefaultFont — the parsed font is DISCARDED in the original (the
// @todo); defFont stays null. The stanza text is kept as the observable.
static Bool parseDefaultFont(RamFile *inFile, char *buffer, std::string &text)
{
        std::string str;
        inFile->scanString(str);

        readUntilSemicolon(inFile, buffer, WIN_BUFFER_LENGTH);
        text = buffer;

        return true;
}

// parseTooltip — sets the fixed "Need tooltip translation" text; no
// parse-state change (the label field is untouched).
static Bool parseTooltip()
{
        return true;
}

static GameWindow *peekWindow(LoadState &S);

// parseScreenRect — the strtok continuation with its own separator set,
// the resolution scaling, and the parent-relative adjustment.
static Bool parseScreenRect(char *buffer, Int *x, Int *y, Int *width, Int *height)
{
        (void)buffer; // the original takes the token but only continues the strtok chain
        LoadState &S = *g_state;
        const char *seps = " ,:=\n\r\t";
        char *c;

        Int loX = 0, loY = 0, hiX = 0, hiY = 0, createX = 0, createY = 0;

        c = strtok(nullptr, seps); // UPPERLEFT token
        c = strtok(nullptr, seps); // x position
        scanInt(tokOrNull(c, "screenrect-token"), loX);
        c = strtok(nullptr, seps); // y position
        scanInt(tokOrNull(c, "screenrect-token"), loY);

        c = strtok(nullptr, seps); // BOTTOMRIGHT token
        c = strtok(nullptr, seps); // x position
        scanInt(tokOrNull(c, "screenrect-token"), hiX);
        c = strtok(nullptr, seps); // y position
        scanInt(tokOrNull(c, "screenrect-token"), hiY);

        c = strtok(nullptr, seps); // CREATIONRESOLUTION token
        c = strtok(nullptr, seps); // x creation resolution
        scanInt(tokOrNull(c, "screenrect-token"), createX);
        c = strtok(nullptr, seps); // y creation resolution
        scanInt(tokOrNull(c, "screenrect-token"), createY);

        // shrink or expand by the current/creation resolution ratio
        Real xScale = (Real)S.displayWidth / (Real)createX;
        Real yScale = (Real)S.displayHeight / (Real)createY;
        Int sloX = (Int)((Real)loX * xScale);
        Int sloY = (Int)((Real)loY * yScale);
        Int shiX = (Int)((Real)hiX * xScale);
        Int shiY = (Int)((Real)hiY * yScale);

        GameWindow *parent = peekWindow(S);

        if (parent) {
                Int px, py;
                winGetScreenPosition(S.arena, parent, &px, &py);
                *x = sloX - px;
                *y = sloY - py;
        } else {
                *x = sloX;
                *y = sloY;
        }

        *width = shiX - sloX;
        *height = shiY - sloY;

        return true;
}

// parseImageOffset
static Bool parseImageOffset(char *buffer, WinInstanceData *instData)
{
        char *c;

        c = strtok(buffer, " \t\n\r");
        instData->m_imageOffsetX = atoi(tokOrNull(c, "imageoffset-token"));

        c = strtok(nullptr, " \t\n\r");
        instData->m_imageOffsetY = atoi(tokOrNull(c, "imageoffset-token"));

        return true;
}

// parseFont — the NAME label is consumed by a first strtok (its NUL the
// quote scan then walks through, verbatim), the value by the
// ':'-separated token after the quote.
static Bool parseFont(char *buffer, WinInstanceData *instData)
{
        char *c, *ptr;
        const char *seps = " ,\n\r\t";
        const char *stringSeps = ":,\n\r\t\"";
        char fontName[256];
        Int fontSize = 0;
        Int fontBold = 0;

        c = strtok(buffer, seps); // label
        // scan to the first " mark
        ptr = scanToQuote(buffer, "font-quote");
        ptr++; // skip the "
        c = strtok(ptr, stringSeps); // value
        rzh_strlcpy(fontName, tokOrNull(c, "font-token"), sizeof(fontName));

        // "SIZE"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "font-value"), fontSize);

        // "BOLD"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "font-value"), fontBold);

        // TheFontLibrary->getFont(name, size, bold): the request is the
        // observable (the resolution is the fonts slice)
        instData->m_hasFont = true;
        instData->m_fontName = fontName;
        instData->m_fontSize = fontSize;
        instData->m_fontBold = fontBold;

        return true;
}

// parseName — the quoted value; the window id is the interned name key.
static Bool parseName(char *buffer, WinInstanceData *instData)
{
        char *c, *ptr;
        const char *stringSeps = "\"";

        ptr = scanToQuote(buffer, "name-quote");
        ptr++; // skip the first "
        c = strtok(ptr, stringSeps); // name value
        instData->m_decoratedNameString = tokOrNull(c, "name-token");

        // given the name assign a window ID from the name key generator
        instData->m_id = (Int)g_state->keys.nameToKey(instData->m_decoratedNameString);

        return true;
}

// parseStatus
static Bool parseStatus(char *buffer, WinInstanceData *instData)
{
        extern const char *const WindowStatusNames[];
        instData->m_status = 0;
        parseBitString(buffer, &instData->m_status, WindowStatusNames);
        return true;
}

// parseStyle
static Bool parseStyle(char *buffer, WinInstanceData *instData)
{
        extern const char *const WindowStyleNames[];
        instData->m_style = 0;
        parseBitString(buffer, &instData->m_style, WindowStyleNames);
        return true;
}

// the four callback parsers (the strings are the observable; the
// FunctionLexicon resolution is the client layer)
static Bool parseCallbackString(char *buffer, std::string &out, const char *quoteSite,
                                 const char *tokenSite)
{
        char *c, *ptr;
        const char *stringSeps = "\"";

        ptr = scanToQuote(buffer, quoteSite);
        ptr++; // skip the first "
        c = strtok(ptr, stringSeps);
        out = tokOrNull(c, tokenSite);
        return true;
}

// parseHeaderTemplate
static Bool parseHeaderTemplate(char *buffer, WinInstanceData *instData)
{
        return parseCallbackString(buffer, instData->m_headerTemplateName,
                                   "headertemplate-quote", "headertemplate-token");
}

// parseListboxData — the optional ScrollIfAtEnd gate, the column run.
static Bool parseListboxData(char *buffer, ListboxData *listData)
{
        char *c;
        const char *seps = " :,\n\r\t";

        // "LENGTH"
        c = strtok(buffer, seps); // label
        c = strtok(nullptr, seps);
        scanShort(tokOrNull(c, "listboxdata-token"), listData->listLength);

        // "AUTOSCROLL"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "listboxdata-token"), listData->autoScroll);

        // "SCROLLIFATEND" (optional)
        c = strtok(nullptr, seps); // label
        if (stricmp(tokOrNull(c, "listboxdata-token"), "ScrollIfAtEnd") == 0) {
                c = strtok(nullptr, seps); // value
                scanBool(tokOrNull(c, "listboxdata-token"), listData->scrollIfAtEnd);
                c = strtok(nullptr, seps); // label
        } else {
                listData->scrollIfAtEnd = false;
        }

        // "AUTOPURGE"
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "listboxdata-token"), listData->autoPurge);

        // "SCROLLBAR"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "listboxdata-token"), listData->scrollBar);

        // "MULTISELECT"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "listboxdata-token"), listData->multiSelect);

        // "COLUMNS"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanShort(tokOrNull(c, "listboxdata-token"), listData->columns);
        if (listData->columns > 1) {
                listData->columnWidthPercentage.clear();
                for (Int i = 0; i < listData->columns; i++) {
                        c = strtok(nullptr, seps); // label
                        c = strtok(nullptr, seps); // value
                        Int pct = 0;
                        scanInt(tokOrNull(c, "listboxdata-token"), pct);
                        listData->columnWidthPercentage.push_back(pct);
                }
        } else {
                // columnWidthPercentage = nullptr
        }
        // columnWidth = nullptr (never set by the parse)
        // "FORCESELECT"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "listboxdata-token"), listData->forceSelect);

        return true;
}

// parseComboBoxData
static Bool parseComboBoxData(char *buffer, ComboBoxData *comboData)
{
        char *c;
        const char *seps = " :,\n\r\t";

        c = strtok(buffer, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "comboboxdata-token"), comboData->isEditable);

        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "comboboxdata-token"), comboData->maxChars);

        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "comboboxdata-token"), comboData->maxDisplay);

        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "comboboxdata-token"), comboData->asciiOnly);

        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "comboboxdata-token"), comboData->lettersAndNumbersOnly);

        return true;
}

// parseSliderData
static Bool parseSliderData(char *buffer, SliderData *sliderData)
{
        char *c;
        const char *seps = " :,\n\r\t";

        c = strtok(buffer, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "sliderdata-token"), sliderData->minVal);

        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "sliderdata-token"), sliderData->maxVal);

        return true;
}

// parseRadioButtonData
static Bool parseRadioButtonData(char *buffer, RadioButtonData *radioData)
{
        char *c;
        const char *seps = " :,\n\r\t";

        c = strtok(buffer, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "radiobuttondata-token"), radioData->group);

        return true;
}

// parseTooltipText — the strlen(ptr) == 1 empty-label early out, the
// MAX_TEXT_LABEL bound.
static Bool parseTooltipText(char *buffer, WinInstanceData *instData)
{
        char *ptr = buffer;
        char *c;
        const char *stringSeps = "\n\r\t\"";

        ptr = scanToQuote(buffer, "tooltiptext-quote");
        ptr++; // skip the "
        if (strlen(ptr) == 1)
                return true;
        c = strtok(ptr, stringSeps); // value
        const char *tok = tokOrNull(c, "tooltiptext-token");
        if (strlen(tok) >= MAX_TEXT_LABEL) {
                // "TextTooltip label '%s' is too long" — the release build
                // still returns FALSE here
                return false;
        }
        instData->m_tooltipString = tok;

        return true;
}

// parseTooltipDelay
static Bool parseTooltipDelay(char *buffer, WinInstanceData *instData)
{
        char *c;
        const char *seps = " :,\n\r\t";

        c = strtok(buffer, seps); // value
        scanInt(tokOrNull(c, "tooltipdelay-token"), instData->m_tooltipDelay);

        return true;
}

// parseText
static Bool parseText(char *buffer, WinInstanceData *instData)
{
        char *ptr = buffer;
        char *c;
        const char *stringSeps = "\n\r\t\"";

        ptr = scanToQuote(buffer, "text-quote");
        ptr++; // skip the "
        c = strtok(ptr, stringSeps); // value
        const char *tok = tokOrNull(c, "text-token");
        if (strlen(tok) >= MAX_TEXT_LABEL) {
                // "Text label '%s' is too long"
                return false;
        }
        instData->m_textLabelString = tok;

        return true;
}

// parseTextColor — the 3-state walk, positional (the labels are read and
// discarded), the first-token bootstrap.
static Bool parseTextColor(char *buffer, WinInstanceData *instData)
{
        char *c;
        const char *seps = " :,\n\r\t";
        // zero-initialized: the original's are uninitialized stack locals
        // whose values a failed sscanf leaves undefined (the port reads 0)
        UnsignedInt r = 0, g = 0, b = 0, a = 0;
        TextDrawData *textData;
        Bool first = true;

        for (Int i = 0; i < 3; i++) {
                if (i == 0)
                        textData = &instData->m_enabledText;
                else if (i == 1)
                        textData = &instData->m_disabledText;
                else
                        textData = &instData->m_hiliteText;

                // color
                if (first == true)
                        c = strtok(buffer, seps); // label
                else
                        c = strtok(nullptr, seps); // label
                first = false;
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "textcolor-token"), r);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "textcolor-token"), g);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "textcolor-token"), b);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "textcolor-token"), a);
                textData->color = GameMakeColor((Byte)r, (Byte)g, (Byte)b, (Byte)a);

                // border color
                c = strtok(nullptr, seps); // label
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "textcolor-token"), r);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "textcolor-token"), g);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "textcolor-token"), b);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "textcolor-token"), a);
                textData->borderColor = GameMakeColor((Byte)r, (Byte)g, (Byte)b, (Byte)a);
        }

        return true;
}

// parseStaticTextData — the forced defaults the editor never writes.
static Bool parseStaticTextData(char *buffer, TextData *textData)
{
        char *c;
        const char *seps = " :,\n\r\t";

        // "CENTERED"
        c = strtok(buffer, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "statictextdata-token"), textData->centered);

        // these are initialized here because no TextData constructor runs
        textData->centeredVertically = true;
        textData->leftMargin = 7;
        textData->topMargin = 7;

        return true;
}

// parseTextEntryData
static Bool parseTextEntryData(char *buffer, EntryData *entryData)
{
        char *c;
        const char *seps = " :,\n\r\t";

        // "MAXLEN"
        c = strtok(buffer, seps); // label
        c = strtok(nullptr, seps); // value
        scanShort(tokOrNull(c, "textentrydata-token"), entryData->maxTextLen);

        // "SECRETTEXT"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "textentrydata-token"), entryData->secretText);

        // "NUMERICALONLY"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "textentrydata-token"), entryData->numericalOnly);

        // "ALPHANUMERICALONLY"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "textentrydata-token"), entryData->alphaNumericalOnly);

        // "ASCIIONLY"
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanBool(tokOrNull(c, "textentrydata-token"), entryData->aSCIIOnly);

        return true;
}

// parseTabControlData — the pane run writes subPaneDisabled[paneIndex]
// with no NUM_TAB_PANES bound in the original; the transcription stops
// with the oob verdict at the first out-of-bounds index.
static Bool parseTabControlData(char *buffer, TabControlData *tabControlData)
{
        char *c;
        const char *seps = " :,\n\r\t";

        // TABORIENTATION
        c = strtok(buffer, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "tabcontroldata-token"), tabControlData->tabOrientation);

        // TABEDGE
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "tabcontroldata-token"), tabControlData->tabEdge);

        // TABWIDTH
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "tabcontroldata-token"), tabControlData->tabWidth);

        // TABHEIGHT
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "tabcontroldata-token"), tabControlData->tabHeight);

        // TABCOUNT
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "tabcontroldata-token"), tabControlData->tabCount);

        // PANEBORDER
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "tabcontroldata-token"), tabControlData->paneBorder);

        // PANEDISABLED
        Int entryCount = 0;
        c = strtok(nullptr, seps); // label
        c = strtok(nullptr, seps); // value
        scanInt(tokOrNull(c, "tabcontroldata-token"), entryCount);

        for (Int paneIndex = 0; paneIndex < entryCount; paneIndex++) {
                c = strtok(nullptr, seps); // value
                Bool v = false;
                scanBool(tokOrNull(c, "tabcontroldata-token"), v);
                if (paneIndex >= NUM_TAB_PANES)
                        stop("oob:subpane-disabled-write");
                tabControlData->subPaneDisabled[paneIndex] = v;
        }

        return true;
}

// parseDrawData — the 9-element loop over one buffer: the file's stanza
// (nine IMAGE/COLOR/BORDERCOLOR triplets, re-joined by the collapsing
// read) is consumed 12 tokens at a time; the token selects the array.
static Bool parseDrawData(const char *token, char *buffer, WinInstanceData *instData)
{
        LoadState &S = *g_state;
        // zero-initialized: the original's are uninitialized stack locals
        // whose values a failed sscanf leaves undefined (the port reads 0)
        UnsignedInt r = 0, g = 0, b = 0, a = 0;
        WinDrawData *drawData;
        Bool first = true;
        char *c;
        const char *seps = " :,\n\r\t";

        for (Int i = 0; i < MAX_DRAW_DATA; i++) {
                if (strcmp(token, "ENABLEDDRAWDATA") == 0)
                        drawData = &instData->m_enabledDrawData[i];
                else if (strcmp(token, "DISABLEDDRAWDATA") == 0)
                        drawData = &instData->m_disabledDrawData[i];
                else if (strcmp(token, "HILITEDRAWDATA") == 0)
                        drawData = &instData->m_hiliteDrawData[i];
                else if (strcmp(token, "LISTBOXENABLEDUPBUTTONDRAWDATA") == 0)
                        drawData = &S.enabledUpButtonDrawData[i];
                else if (strcmp(token, "LISTBOXDISABLEDUPBUTTONDRAWDATA") == 0)
                        drawData = &S.disabledUpButtonDrawData[i];
                else if (strcmp(token, "LISTBOXHILITEUPBUTTONDRAWDATA") == 0)
                        drawData = &S.hiliteUpButtonDrawData[i];
                else if (strcmp(token, "LISTBOXENABLEDDOWNBUTTONDRAWDATA") == 0)
                        drawData = &S.enabledDownButtonDrawData[i];
                else if (strcmp(token, "LISTBOXDISABLEDDOWNBUTTONDRAWDATA") == 0)
                        drawData = &S.disabledDownButtonDrawData[i];
                else if (strcmp(token, "LISTBOXHILITEDOWNBUTTONDRAWDATA") == 0)
                        drawData = &S.hiliteDownButtonDrawData[i];
                else if (strcmp(token, "LISTBOXENABLEDSLIDERDRAWDATA") == 0)
                        drawData = &S.enabledSliderDrawData[i];
                else if (strcmp(token, "LISTBOXDISABLEDSLIDERDRAWDATA") == 0)
                        drawData = &S.disabledSliderDrawData[i];
                else if (strcmp(token, "LISTBOXHILITESLIDERDRAWDATA") == 0)
                        drawData = &S.hiliteSliderDrawData[i];
                else if (strcmp(token, "SLIDERTHUMBENABLEDDRAWDATA") == 0)
                        drawData = &S.enabledSliderThumbDrawData[i];
                else if (strcmp(token, "SLIDERTHUMBDISABLEDDRAWDATA") == 0)
                        drawData = &S.disabledSliderThumbDrawData[i];
                else if (strcmp(token, "SLIDERTHUMBHILITEDRAWDATA") == 0)
                        drawData = &S.hiliteSliderThumbDrawData[i];
                else if (strcmp(token, "COMBOBOXDROPDOWNBUTTONENABLEDDRAWDATA") == 0)
                        drawData = &S.enabledDropDownButtonDrawData[i];
                else if (strcmp(token, "COMBOBOXDROPDOWNBUTTONDISABLEDDRAWDATA") == 0)
                        drawData = &S.disabledDropDownButtonDrawData[i];
                else if (strcmp(token, "COMBOBOXDROPDOWNBUTTONHILITEDRAWDATA") == 0)
                        drawData = &S.hiliteDropDownButtonDrawData[i];
                else if (strcmp(token, "COMBOBOXEDITBOXENABLEDDRAWDATA") == 0)
                        drawData = &S.enabledEditBoxDrawData[i];
                else if (strcmp(token, "COMBOBOXEDITBOXDISABLEDDRAWDATA") == 0)
                        drawData = &S.disabledEditBoxDrawData[i];
                else if (strcmp(token, "COMBOBOXEDITBOXHILITEDRAWDATA") == 0)
                        drawData = &S.hiliteEditBoxDrawData[i];
                else if (strcmp(token, "COMBOBOXLISTBOXENABLEDDRAWDATA") == 0)
                        drawData = &S.enabledListBoxDrawData[i];
                else if (strcmp(token, "COMBOBOXLISTBOXDISABLEDDRAWDATA") == 0)
                        drawData = &S.disabledListBoxDrawData[i];
                else if (strcmp(token, "COMBOBOXLISTBOXHILITEDRAWDATA") == 0)
                        drawData = &S.hiliteListBoxDrawData[i];
                else {
                        // "ParseDrawData, undefined token" — unreachable:
                        // the field table only routes these tokens here
                        return false;
                }

                // IMAGE: X
                if (first == true)
                        c = strtok(buffer, seps); // label
                else
                        c = strtok(nullptr, seps); // label
                first = false;

                c = strtok(nullptr, seps); // value
                if (strcmp(tokOrNull(c, "drawdata-image"), "NoImage") != 0) {
                        // findImageByName: the requested name is the
                        // observable (a miss would store null — the
                        // collection is environment state)
                        drawData->image = c;
                } else {
                        drawData->image.clear();
                }
                // COLOR: R G B A
                c = strtok(nullptr, seps); // label
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "drawdata-color"), r);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "drawdata-color"), g);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "drawdata-color"), b);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "drawdata-color"), a);
                drawData->color = GameMakeColor((Byte)r, (Byte)g, (Byte)b, (Byte)a);

                // BORDERCOLOR: R G B A
                c = strtok(nullptr, seps); // label
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "drawdata-color"), r);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "drawdata-color"), g);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "drawdata-color"), b);
                c = strtok(nullptr, seps); // value
                scanUnsignedInt(tokOrNull(c, "drawdata-color"), a);
                drawData->borderColor = GameMakeColor((Byte)r, (Byte)g, (Byte)b, (Byte)a);
        }

        return true;
}

// ---------------------------------------------------------------------------
// the flag name tables (GameWindowManagerScript.cpp, verbatim)
// ---------------------------------------------------------------------------

const char *const WindowStatusNames[] = {
        "ACTIVE",           "TOGGLE",       "DRAGABLE",      "ENABLED",     "HIDDEN",
        "ABOVE",            "BELOW",        "IMAGE",         "TABSTOP",     "NOINPUT",
        "NOFOCUS",          "DESTROYED",    "BORDER",        "SMOOTH_TEXT", "ONE_LINE",
        "NO_FLUSH",         "SEE_THRU",     "RIGHT_CLICK",   "WRAP_CENTERED",
        "CHECK_LIKE",       "HOTKEY_TEXT",  "USE_OVERLAY_STATES", "NOT_READY",
        "FLASHING",         "ALWAYS_COLOR", "ON_MOUSE_DOWN", nullptr,
};

const char *const WindowStyleNames[] = {
        "PUSHBUTTON", "RADIOBUTTON",  "CHECKBOX",      "VERTSLIDER",  "HORZSLIDER",
        "SCROLLLISTBOX", "ENTRYFIELD", "STATICTEXT",    "PROGRESSBAR", "USER",
        "MOUSETRACK", "ANIMATED",     "TABSTOP",       "TABCONTROL",  "TABPANE",
        "COMBOBOX",   nullptr,
};

// GWS_* (Gadget.h) — the style bits createWindow/createGadget OR in.
enum {
        GWS_PUSH_BUTTON = 0x00000001,
        GWS_RADIO_BUTTON = 0x00000002,
        GWS_CHECK_BOX = 0x00000004,
        GWS_VERT_SLIDER = 0x00000008,
        GWS_HORZ_SLIDER = 0x00000010,
        GWS_SCROLL_LISTBOX = 0x00000020,
        GWS_ENTRY_FIELD = 0x00000040,
        GWS_STATIC_TEXT = 0x00000080,
        GWS_PROGRESS_BAR = 0x00000100,
        GWS_USER_WINDOW = 0x00000200,
        GWS_TAB_CONTROL = 0x00002000,
        GWS_TAB_PANE = 0x00004000,
        GWS_COMBO_BOX = 0x00008000,
};

// ---------------------------------------------------------------------------
// getDataTemplate / parseData (the obsolete DATA field path)
// ---------------------------------------------------------------------------

// the data template pointer handed to the field parsers; -1 == null
struct DataPtr {
        int which; // 0 == null, 1..7 == the template statics, 8..12 == the
                   // parseData statics
};

static void *getDataTemplate(LoadState &S, const char *type)
{
        if (strcmp(type, "VERTSLIDER") == 0 || strcmp(type, "HORZSLIDER") == 0) {
                memset(&S.tSlider, 0, sizeof(S.tSlider));
                return &S.tSlider;
        } else if (strcmp(type, "SCROLLLISTBOX") == 0) {
                S.tListbox = {};
                return &S.tListbox;
        } else if (strcmp(type, "TABCONTROL") == 0) {
                memset(&S.tTab, 0, sizeof(S.tTab));
                return &S.tTab;
        } else if (strcmp(type, "ENTRYFIELD") == 0) {
                memset(&S.tEntry, 0, sizeof(S.tEntry));
                return &S.tEntry;
        } else if (strcmp(type, "STATICTEXT") == 0) {
                memset(&S.tText, 0, sizeof(S.tText));
                return &S.tText;
        } else if (strcmp(type, "RADIOBUTTON") == 0) {
                memset(&S.tRadio, 0, sizeof(S.tRadio));
                return &S.tRadio;
        } else if (strcmp(type, "COMBOBOX") == 0) {
                S.tCombo = {};
                return &S.tCombo;
        }
        return nullptr;
}

// parseData — the obsolete flat DATA field. statictext and radiobutton are
// NOT memset in the original (their never-written members keep whatever an
// earlier DATA read left — zero for the whole load, since nothing else
// writes those statics).
static void *parseData(LoadState &S, void *data, const char *type, char *buffer)
{
        char *c;

        if (strcmp(type, "VERTSLIDER") == 0 || strcmp(type, "HORZSLIDER") == 0) {
                memset(&S.dSlider, 0, sizeof(S.dSlider));

                c = strtok(buffer, " \t\n\r");
                S.dSlider.minVal = atoi(tokOrNull(c, "data-token"));

                c = strtok(nullptr, " \t\n\r");
                S.dSlider.maxVal = atoi(tokOrNull(c, "data-token"));

                data = &S.dSlider;
        } else if (strcmp(type, "SCROLLLISTBOX") == 0) {
                S.dListbox = {};

                c = strtok(buffer, " \t\n\r");
                S.dListbox.listLength = atoi(tokOrNull(c, "data-token"));

                c = strtok(nullptr, " \t\n\r");
                S.dListbox.autoScroll = atoi(tokOrNull(c, "data-token")) != 0;

                c = strtok(nullptr, " \t\n\r");
                S.dListbox.autoPurge = atoi(tokOrNull(c, "data-token")) != 0;

                c = strtok(nullptr, " \t\n\r");
                S.dListbox.scrollBar = atoi(tokOrNull(c, "data-token")) != 0;

                c = strtok(nullptr, " \t\n\r");
                S.dListbox.multiSelect = atoi(tokOrNull(c, "data-token")) != 0;

                c = strtok(nullptr, " \t\n\r");
                S.dListbox.forceSelect = atoi(tokOrNull(c, "data-token")) != 0;

                data = &S.dListbox;
        } else if (strcmp(type, "ENTRYFIELD") == 0) {
                memset(&S.dEntry, 0, sizeof(S.dEntry));

                c = strtok(buffer, " \t\n\r");
                S.dEntry.maxTextLen = (Short)atoi(tokOrNull(c, "data-token"));

                c = strtok(nullptr, " \t\n\r");
                // (the entryWidth read is commented out in the original)

                c = strtok(nullptr, " \t\n\r");
                if (c) {
                        S.dEntry.secretText = atoi(c) != 0 ? true : false;
                } else {
                        S.dEntry.secretText = false;
                }

                c = strtok(nullptr, " \t\n\r");
                if (c) {
                        S.dEntry.numericalOnly = (atoi(c) == 1);
                        S.dEntry.alphaNumericalOnly = (atoi(c) == 2);
                        S.dEntry.aSCIIOnly = (atoi(c) == 3);
                } else {
                        S.dEntry.numericalOnly = false;
                        S.dEntry.alphaNumericalOnly = false;
                        S.dEntry.aSCIIOnly = false;
                }
                data = &S.dEntry;
        } else if (strcmp(type, "STATICTEXT") == 0) {
                // no memset: centeredVertically/leftMargin/topMargin keep
                // the load's earlier values (zero — nothing writes them)
                c = strtok(buffer, " \t\n\r");
                S.dText.centered = atoi(tokOrNull(c, "data-token")) != 0 ? true : false;

                c = strtok(nullptr, " \t\n\r");
                // (the label read is dead in the original)

                data = &S.dText;
        } else if (strcmp(type, "RADIOBUTTON") == 0) {
                // no memset
                c = strtok(buffer, " \t\n\r");
                S.dRadio.group = atoi(tokOrNull(c, "data-token"));

                data = &S.dRadio;
        } else {
                data = nullptr;
        }

        return data;
}

// ---------------------------------------------------------------------------
// createGadget / createWindow
// ---------------------------------------------------------------------------

// snapshot the gadget data the window carries (the original hands the
// gadget the shared static; the copy is the created value — see the port's
// docs)
static void snapshotGadget(LoadState &S, GameWindow *w, const char *type, void *data)
{
        (void)S;
        Gadget &g = w->gadget;
        if (strcmp(type, "PUSHBUTTON") == 0 || strcmp(type, "CHECKBOX") == 0 ||
            strcmp(type, "PROGRESSBAR") == 0) {
                g.kind = GADGET_NONE;
        } else if (strcmp(type, "RADIOBUTTON") == 0) {
                g.kind = GADGET_RADIO;
                g.radio = *(RadioButtonData *)data;
        } else if (strcmp(type, "TABCONTROL") == 0) {
                g.kind = GADGET_TABCONTROL;
                g.tab = *(TabControlData *)data;
        } else if (strcmp(type, "VERTSLIDER") == 0 || strcmp(type, "HORZSLIDER") == 0) {
                g.kind = GADGET_SLIDER;
                g.slider = *(SliderData *)data;
        } else if (strcmp(type, "SCROLLLISTBOX") == 0) {
                g.kind = GADGET_LISTBOX;
                g.listbox = *(ListboxData *)data;
        } else if (strcmp(type, "COMBOBOX") == 0) {
                g.kind = GADGET_COMBOBOX;
                ComboBoxData *cData = (ComboBoxData *)data;
                // createGadget's constructed halves (verbatim): the NEW
                // EntryData / ListboxData with their fixed starts and the
                // copied gates, and entryCount = 0
                cData->entryCount = 0;
                cData->entryData = {};
                cData->listboxData = {};
                cData->entryData.aSCIIOnly = cData->asciiOnly;
                cData->entryData.alphaNumericalOnly = cData->lettersAndNumbersOnly;
                cData->entryData.maxTextLen = (Short)cData->maxChars;
                cData->listboxData.listLength = 10;
                cData->listboxData.autoScroll = false;
                cData->listboxData.scrollIfAtEnd = false;
                cData->listboxData.autoPurge = false;
                cData->listboxData.scrollBar = true;
                cData->listboxData.multiSelect = false;
                cData->listboxData.forceSelect = true;
                cData->listboxData.columns = 1;
                g.combobox = *cData;
        } else if (strcmp(type, "ENTRYFIELD") == 0) {
                g.kind = GADGET_ENTRY;
                g.entry = *(EntryData *)data;
        } else if (strcmp(type, "STATICTEXT") == 0) {
                g.kind = GADGET_STATICTEXT;
                g.text = *(TextData *)data;
        } else {
                g.kind = GADGET_NONE;
        }
}

// snapshotRouted — the composite draw data the gadget's construction
// routes in from the load's static arrays, in the original's routing
// order (slider: the thumb; listbox: up, down, slider, the slider's
// thumb; combo box: dropdown, editbox, listbox, then the listbox's up,
// down, slider and thumb)
static void snapshotRouted(LoadState &S, GameWindow *w, const char *type)
{
        auto grab = [&](const char *slot, const WinDrawData *en, const WinDrawData *dis,
                        const WinDrawData *hil) {
                RoutedDraw r;
                r.slot = slot;
                for (int z = 0; z < MAX_DRAW_DATA; z++) {
                        r.enabled[z] = en[z];
                        r.disabled[z] = dis[z];
                        r.hilite[z] = hil[z];
                }
                w->routed.push_back(std::move(r));
        };
        if (strcmp(type, "VERTSLIDER") == 0 || strcmp(type, "HORZSLIDER") == 0) {
                grab("thumb", S.enabledSliderThumbDrawData, S.disabledSliderThumbDrawData,
                     S.hiliteSliderThumbDrawData);
        } else if (strcmp(type, "SCROLLLISTBOX") == 0) {
                grab("upbtn", S.enabledUpButtonDrawData, S.disabledUpButtonDrawData,
                     S.hiliteUpButtonDrawData);
                grab("downbtn", S.enabledDownButtonDrawData, S.disabledDownButtonDrawData,
                     S.hiliteDownButtonDrawData);
                grab("slider", S.enabledSliderDrawData, S.disabledSliderDrawData,
                     S.hiliteSliderDrawData);
                grab("thumb", S.enabledSliderThumbDrawData, S.disabledSliderThumbDrawData,
                     S.hiliteSliderThumbDrawData);
        } else if (strcmp(type, "COMBOBOX") == 0) {
                grab("dropdown", S.enabledDropDownButtonDrawData, S.disabledDropDownButtonDrawData,
                     S.hiliteDropDownButtonDrawData);
                grab("editbox", S.enabledEditBoxDrawData, S.disabledEditBoxDrawData,
                     S.hiliteEditBoxDrawData);
                grab("cblistbox", S.enabledListBoxDrawData, S.disabledListBoxDrawData,
                     S.hiliteListBoxDrawData);
                grab("upbtn", S.enabledUpButtonDrawData, S.disabledUpButtonDrawData,
                     S.hiliteUpButtonDrawData);
                grab("downbtn", S.enabledDownButtonDrawData, S.disabledDownButtonDrawData,
                     S.hiliteDownButtonDrawData);
                grab("slider", S.enabledSliderDrawData, S.disabledSliderDrawData,
                     S.hiliteSliderDrawData);
                grab("thumb", S.enabledSliderThumbDrawData, S.disabledSliderThumbDrawData,
                     S.hiliteSliderThumbDrawData);
        }
}

// createGadget — the style-bit routing, the radio screen id, the composite
// draw-data routing (the arrays stay parse state on the load; the original
// copies them into the sub-windows it constructs — the construction is the
// client layer, the arrays are the observable).
static GameWindow *createGadget(LoadState &S, const char *type, GameWindow *parent, Int status,
                                Int x, Int y, Int width, Int height, WinInstanceData *instData,
                                void *data)
{
        (void)status; // the original passes it through to the gadget constructors
        GameWindow *window = nullptr;

        instData->m_owner = parent ? parent->index : -1;

        if (strcmp(type, "PUSHBUTTON") == 0) {
                instData->m_style |= GWS_PUSH_BUTTON;
        } else if (strcmp(type, "RADIOBUTTON") == 0) {
                RadioButtonData *rData = (RadioButtonData *)data;
                char filename[MAX_WINDOW_NAME_LEN];
                char *c;

                rzh_strlcpy(filename, instData->m_decoratedNameString.c_str(), sizeof(filename));
                c = strchr(filename, ':');
                if (c)
                        *c = 0; // terminate after filename (format is filename:gadgetname)
                rData->screen = (Int)(S.keys.nameToKey(filename));

                instData->m_style |= GWS_RADIO_BUTTON;
        } else if (strcmp(type, "CHECKBOX") == 0) {
                instData->m_style |= GWS_CHECK_BOX;
        } else if (strcmp(type, "TABCONTROL") == 0) {
                instData->m_style |= GWS_TAB_CONTROL;
        } else if (strcmp(type, "VERTSLIDER") == 0) {
                instData->m_style |= GWS_VERT_SLIDER;
        } else if (strcmp(type, "HORZSLIDER") == 0) {
                instData->m_style |= GWS_HORZ_SLIDER;
        } else if (strcmp(type, "SCROLLLISTBOX") == 0) {
                instData->m_style |= GWS_SCROLL_LISTBOX;
        } else if (strcmp(type, "COMBOBOX") == 0) {
                ComboBoxData *cData = (ComboBoxData *)data;
                // the NEW'd halves are snapshotted in snapshotGadget; the
                // mutation happens on the shared static there (verbatim
                // order: createGadget mutates, then the gadget reads)
                cData->entryCount = 0;
                instData->m_style |= GWS_COMBO_BOX;
        } else if (strcmp(type, "ENTRYFIELD") == 0) {
                instData->m_style |= GWS_ENTRY_FIELD;
        } else if (strcmp(type, "STATICTEXT") == 0) {
                instData->m_style |= GWS_STATIC_TEXT;
        } else if (strcmp(type, "PROGRESSBAR") == 0) {
                instData->m_style |= GWS_PROGRESS_BAR;
        } else {
                // unknown gadget type: gogoGadget* never runs, the window
                // stays null (createWindow's caller sees the null)
                return nullptr;
        }

        // the gogoGadget* creation (the sub-window construction is the
        // client layer; the draw data it routes in from the static arrays
        // is snapshotted here, in the original's routing order)
        window = new GameWindow();
        window->index = (Int)S.arena.size();
        S.arena.push_back(window);
        window->type = type;
        window->parent = parent ? parent->index : -1;
        if (parent)
                parent->children.push_back(window->index);
        window->x = x;
        window->y = y;
        window->width = width;
        window->height = height;
        window->inst = *instData;
        snapshotGadget(S, window, type, data);
        snapshotRouted(S, window, type);

        return window;
}

// createWindow — USER/TABPANE get the generic creation; everything else
// routes to createGadget.
static GameWindow *createWindow(LoadState &S, const char *type, Int id, Int status, Int x, Int y,
                                Int width, Int height, WinInstanceData *instData, void *data)
{
        GameWindow *window;
        GameWindow *parent = peekWindow(S);

        if (strcmp(type, "USER") == 0 || strcmp(type, "TABPANE") == 0) {
                window = new GameWindow();
                window->index = (Int)S.arena.size();
                S.arena.push_back(window);
                window->type = type;
                window->parent = parent ? parent->index : -1;
                if (parent)
                        parent->children.push_back(window->index);
                window->x = x;
                window->y = y;
                window->width = width;
                window->height = height;
                window->inst = *instData;
                window->inst.m_style |=
                        (strcmp(type, "USER") == 0) ? GWS_USER_WINDOW : GWS_TAB_PANE;
                window->gadget.kind = GADGET_NONE;
        } else {
                window = createGadget(S, type, parent, status, x, y, width, height, instData, data);
                if (window == nullptr)
                        return nullptr;
        }

        // winSetWindowId(id) — every created window takes the instance
        // data's id (NAME's interned key, or 0 unnamed)
        window->inst.m_id = id;

        return window;
}

// ---------------------------------------------------------------------------
// the window stack
// ---------------------------------------------------------------------------

static void resetWindowStack(LoadState &S)
{
        memset(S.windowStack, 0, sizeof(S.windowStack));
        S.stackPtr = S.windowStack;
}

static GameWindow *peekWindow(LoadState &S)
{
        if (S.stackPtr == S.windowStack)
                return nullptr;
        return *(S.stackPtr - 1);
}

static GameWindow *popWindow(LoadState &S)
{
        if (S.stackPtr == S.windowStack)
                return nullptr;
        S.stackPtr--;
        return *S.stackPtr;
}

static void pushWindow(LoadState &S, GameWindow *window)
{
        if (S.stackPtr == &S.windowStack[WIN_STACK_DEPTH - 1]) {
                // "pushWindow: Warning, stack overflow" (DEBUG_LOG only)
                return;
        }
        *S.stackPtr++ = window;
}

// ---------------------------------------------------------------------------
// GadgetTabControlFixupSubPaneList — the reverse child walk (the sub-pane
// slots cap at NUM_TAB_PANES; a longer child list writes past them in the
// original — the transcription stops at the bound)
// ---------------------------------------------------------------------------

static void fixupSubPaneList(LoadState &S, GameWindow *tabControl)
{
        (void)S;
        Int childIndex = 0;
        TabControlData *tabData = &tabControl->gadget.tab;
        if (!tabControl->children.empty()) {
                // the children walk from the last to the first
                for (size_t k = tabControl->children.size(); k-- > 0;) {
                        if (childIndex >= NUM_TAB_PANES)
                                stop("oob:subpanes-write");
                        tabData->subPanes[childIndex] = tabControl->children[k];
                        childIndex++;
                }
        }
}

// ---------------------------------------------------------------------------
// parseChildWindows / parseWindow
// ---------------------------------------------------------------------------

static GameWindow *parseWindow(LoadState &S);

// parseChildWindows — the child loop: ENDALLCHILDREN/END break, the color
// defaults re-parse, WINDOW recurses. Unknown tokens are dropped WITHOUT
// consuming their value (no else branch — the value tokens become
// subsequent tokens).
static Bool parseChildWindows(LoadState &S, GameWindow *window)
{
        std::string asciibuf;

        // (the tab control's default-children destroy has no observable
        // here: the model never auto-creates the sub-panes — see the port
        // docs)

        pushWindow(S, window);

        while (true) {
                if (S.inFile->scanString(asciibuf) == false) {
                        break;
                }

                if (asciibuf == "ENDALLCHILDREN") {
                        break;
                }

                if (asciibuf == "END") {
                        break;
                }

                if (asciibuf == "ENABLEDCOLOR") {
                        parseDefaultColor(&S.defEnabledColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "DISABLEDCOLOR") {
                        parseDefaultColor(&S.defDisabledColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "HILITECOLOR") {
                        parseDefaultColor(&S.defHiliteColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "SELECTEDCOLOR") {
                        parseDefaultColor(&S.defSelectedColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "TEXTCOLOR") {
                        parseDefaultColor(&S.defTextColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "WINDOW") {
                        if (parseWindow(S) == nullptr) {
                                return false;
                        }
                }
                // (no else: unknown child tokens drop whole)
        }

        // Pop the current window off the stack
        GameWindow *lastWindow = popWindow(S);

        if (lastWindow != window) {
                // "parseChildWindows: unmatched window on stack"
                return false;
        }

        if (window->inst.m_style & GWS_TAB_CONTROL)
                fixupSubPaneList(S, window);

        return true;
}

// the field table walk state
struct FieldParse {
        const char *name;
};

static const FieldParse gameWindowFieldList[] = {
        {"NAME"},         {"STATUS"},         {"STYLE"},           {"SYSTEMCALLBACK"},
        {"INPUTCALLBACK"}, {"TOOLTIPCALLBACK"}, {"DRAWCALLBACK"},   {"FONT"},
        {"HEADERTEMPLATE"}, {"LISTBOXDATA"},   {"COMBOBOXDATA"},    {"SLIDERDATA"},
        {"RADIOBUTTONDATA"}, {"TOOLTIPTEXT"},  {"TOOLTIPDELAY"},    {"TEXT"},
        {"TEXTCOLOR"},    {"STATICTEXTDATA"}, {"TEXTENTRYDATA"},   {"TABCONTROLDATA"},
        {"ENABLEDDRAWDATA"}, {"DISABLEDDRAWDATA"}, {"HILITEDRAWDATA"},
        {"LISTBOXENABLEDUPBUTTONDRAWDATA"}, {"LISTBOXENABLEDDOWNBUTTONDRAWDATA"},
        {"LISTBOXENABLEDSLIDERDRAWDATA"}, {"LISTBOXDISABLEDUPBUTTONDRAWDATA"},
        {"LISTBOXDISABLEDDOWNBUTTONDRAWDATA"}, {"LISTBOXDISABLEDSLIDERDRAWDATA"},
        {"LISTBOXHILITEUPBUTTONDRAWDATA"}, {"LISTBOXHILITEDOWNBUTTONDRAWDATA"},
        {"LISTBOXHILITESLIDERDRAWDATA"}, {"SLIDERTHUMBENABLEDDRAWDATA"},
        {"SLIDERTHUMBDISABLEDDRAWDATA"}, {"SLIDERTHUMBHILITEDRAWDATA"},
        {"COMBOBOXDROPDOWNBUTTONENABLEDDRAWDATA"}, {"COMBOBOXDROPDOWNBUTTONDISABLEDDRAWDATA"},
        {"COMBOBOXDROPDOWNBUTTONHILITEDRAWDATA"}, {"COMBOBOXEDITBOXENABLEDDRAWDATA"},
        {"COMBOBOXEDITBOXDISABLEDDRAWDATA"}, {"COMBOBOXEDITBOXHILITEDRAWDATA"},
        {"COMBOBOXLISTBOXENABLEDDRAWDATA"}, {"COMBOBOXLISTBOXDISABLEDDRAWDATA"},
        {"COMBOBOXLISTBOXHILITEDRAWDATA"}, {"IMAGEOFFSET"}, {"TOOLTIP"}, {nullptr},
};

// parseWindow — the WINDOWTYPE/SCREENRECT head, the field loop, the
// DATA/END/CHILD routing.
static GameWindow *parseWindow(LoadState &S)
{
        GameWindow *window = nullptr;
        // (the original computes `parent` here for a parentSize it never
        // uses — dead code, not transcribed)
        WinInstanceData instData;
        char type[MAX_WINDOW_NAME_LEN];
        char token[256];
        char *c;
        Int x = 0, y = 0, width = 0, height = 0;
        void *data = nullptr;
        std::string asciibuf;
        const char *seps = " =;\n\r\t";

        // reset the callback strings (the original resets the function
        // pointers and their strings)
        std::string theSystemString, theInputString, theTooltipString, theDrawString;

        // get the size of the parent, or the screen (dead in the original —
        // parentSize is never used)

        // Initialize the instance data to the defaults
        instData.init();
        instData.m_enabledText.color = S.defTextColor;
        instData.m_enabledText.borderColor = S.defTextColor;
        instData.m_disabledText.color = S.defTextColor;
        instData.m_disabledText.borderColor = S.defTextColor;
        instData.m_hiliteText.color = S.defTextColor;
        instData.m_hiliteText.borderColor = S.defTextColor;

        instData.m_hasFont = false; // defFont is always null (the default
                                    // font parse discards its text)

        // window type
        readUntilSemicolon(S.inFile, (char *)S.buffer, WIN_BUFFER_LENGTH);
        c = strtok((char *)S.buffer, seps);
        // (the WINDOWTYPE assert is debug-only; a missing token crashes
        // the release build at the strlcpy)
        c = strtok(nullptr, seps);
        rzh_strlcpy(type, tokOrNull(c, "windowtype-token"), sizeof(type));

        // based on the window type get the data template
        data = getDataTemplate(S, type);

        // position
        readUntilSemicolon(S.inFile, (char *)S.buffer, WIN_BUFFER_LENGTH);
        c = strtok((char *)S.buffer, seps);
        // (the SCREENRECT assert is debug-only)
        if (parseScreenRect((char *)S.buffer, &x, &y, &width, &height) == false)
                goto cleanupAndExit;

        // parse all the field definitions
        while (true) {
                // get token — an EOF here spins the original's loop
                // forever (the empty token matches nothing, the
                // unrecognized branch re-reads the stale buffer)
                if (S.inFile->scanString(asciibuf) == false) {
                        stop("hang:window-field-eof");
                }

                // parse field
                size_t fi;
                for (fi = 0; gameWindowFieldList[fi].name; fi++) {
                        if (asciibuf == gameWindowFieldList[fi].name) {
                                rzh_strlcpy(token, asciibuf.c_str(), sizeof(token));

                                // eat '='
                                S.inFile->scanString(asciibuf);

                                readUntilSemicolon(S.inFile, (char *)S.buffer, WIN_BUFFER_LENGTH);

                                Bool ok = true;
                                if (strcmp(token, "LISTBOXDATA") == 0 ||
                                    strcmp(token, "COMBOBOXDATA") == 0 ||
                                    strcmp(token, "SLIDERDATA") == 0 ||
                                    strcmp(token, "RADIOBUTTONDATA") == 0 ||
                                    strcmp(token, "STATICTEXTDATA") == 0 ||
                                    strcmp(token, "TEXTENTRYDATA") == 0 ||
                                    strcmp(token, "TABCONTROLDATA") == 0) {
                                        if (data == nullptr)
                                                stop("crash:null-data-template");
                                }
                                if (strcmp(token, "NAME") == 0) {
                                        ok = parseName((char *)S.buffer, &instData);
                                } else if (strcmp(token, "STATUS") == 0) {
                                        ok = parseStatus((char *)S.buffer, &instData);
                                } else if (strcmp(token, "STYLE") == 0) {
                                        ok = parseStyle((char *)S.buffer, &instData);
                                } else if (strcmp(token, "SYSTEMCALLBACK") == 0) {
                                        ok = parseCallbackString((char *)S.buffer,
                                                                 theSystemString, "cb-quote",
                                                                 "cb-token");
                                } else if (strcmp(token, "INPUTCALLBACK") == 0) {
                                        ok = parseCallbackString((char *)S.buffer,
                                                                 theInputString, "cb-quote",
                                                                 "cb-token");
                                } else if (strcmp(token, "TOOLTIPCALLBACK") == 0) {
                                        ok = parseCallbackString((char *)S.buffer,
                                                                 theTooltipString, "cb-quote",
                                                                 "cb-token");
                                } else if (strcmp(token, "DRAWCALLBACK") == 0) {
                                        ok = parseCallbackString((char *)S.buffer,
                                                                 theDrawString, "cb-quote",
                                                                 "cb-token");
                                } else if (strcmp(token, "FONT") == 0) {
                                        ok = parseFont((char *)S.buffer, &instData);
                                } else if (strcmp(token, "HEADERTEMPLATE") == 0) {
                                        ok = parseHeaderTemplate((char *)S.buffer, &instData);
                                } else if (strcmp(token, "LISTBOXDATA") == 0) {
                                        ok = parseListboxData((char *)S.buffer,
                                                              (ListboxData *)data);
                                } else if (strcmp(token, "COMBOBOXDATA") == 0) {
                                        ok = parseComboBoxData((char *)S.buffer,
                                                               (ComboBoxData *)data);
                                } else if (strcmp(token, "SLIDERDATA") == 0) {
                                        ok = parseSliderData((char *)S.buffer,
                                                             (SliderData *)data);
                                } else if (strcmp(token, "RADIOBUTTONDATA") == 0) {
                                        ok = parseRadioButtonData((char *)S.buffer,
                                                                  (RadioButtonData *)data);
                                } else if (strcmp(token, "TOOLTIPTEXT") == 0) {
                                        ok = parseTooltipText((char *)S.buffer, &instData);
                                } else if (strcmp(token, "TOOLTIPDELAY") == 0) {
                                        ok = parseTooltipDelay((char *)S.buffer, &instData);
                                } else if (strcmp(token, "TEXT") == 0) {
                                        ok = parseText((char *)S.buffer, &instData);
                                } else if (strcmp(token, "TEXTCOLOR") == 0) {
                                        ok = parseTextColor((char *)S.buffer, &instData);
                                } else if (strcmp(token, "STATICTEXTDATA") == 0) {
                                        ok = parseStaticTextData((char *)S.buffer,
                                                                 (TextData *)data);
                                } else if (strcmp(token, "TEXTENTRYDATA") == 0) {
                                        ok = parseTextEntryData((char *)S.buffer,
                                                                (EntryData *)data);
                                } else if (strcmp(token, "TABCONTROLDATA") == 0) {
                                        ok = parseTabControlData((char *)S.buffer,
                                                                 (TabControlData *)data);
                                } else if (strcmp(token, "IMAGEOFFSET") == 0) {
                                        ok = parseImageOffset((char *)S.buffer, &instData);
                                } else if (strcmp(token, "TOOLTIP") == 0) {
                                        ok = parseTooltip();
                                } else {
                                        // the draw-data family (the token
                                        // selects the array)
                                        ok = parseDrawData(token, (char *)S.buffer, &instData);
                                }

                                if (ok == false) {
                                        // "parseGameObject: Error parsing %s"
                                        goto cleanupAndExit;
                                }

                                break;
                        }
                }

                if (gameWindowFieldList[fi].name == nullptr) {
                        // If it's the END keyword
                        if (asciibuf == "DATA") {
                                // eat '='
                                S.inFile->scanString(asciibuf);

                                readUntilSemicolon(S.inFile, (char *)S.buffer, WIN_BUFFER_LENGTH);

                                data = parseData(S, data, type, (char *)S.buffer);

                                // DATA on TABCONTROL/COMBOBOX hands the
                                // gadget a null data pointer — the original
                                // crashes in the construction that follows
                                if (data == nullptr &&
                                    (strcmp(type, "TABCONTROL") == 0 ||
                                     strcmp(type, "COMBOBOX") == 0)) {
                                        stop("crash:gadget-null-data");
                                }
                        } else if (asciibuf == "END") {
                                // (the header-template font swap resolves
                                // against the template manager — the name
                                // is the observable; the font request
                                // stands)

                                // Create a window using the current description
                                if (window == nullptr)
                                        window = createWindow(S, type, instData.m_id,
                                                              (Int)instData.m_status, x, y, width,
                                                              height, &instData, data);

                                goto cleanupAndExit;
                        } else if (asciibuf == "CHILD") {
                                // Create a window using the current description
                                window = createWindow(S, type, instData.m_id,
                                                      (Int)instData.m_status, x, y, width, height,
                                                      &instData, data);

                                if (window == nullptr)
                                        goto cleanupAndExit;

                                // Parses the CHILD's window info.
                                if (parseChildWindows(S, window) == false) {
                                        // winDestroy(window) — the subtree
                                        // drops out of the walk
                                        window = nullptr;
                                        goto cleanupAndExit;
                                }
                        } else {
                                // Else it is unrecognized so eat associated data
                                readUntilSemicolon(S.inFile, (char *)S.buffer, WIN_BUFFER_LENGTH);
                        }
                }
        }

cleanupAndExit:
        // the created window's callback strings (the editor data saves)
        if (window) {
                window->systemCallbackString = theSystemString;
                window->inputCallbackString = theInputString;
                window->tooltipCallbackString = theTooltipString;
                window->drawCallbackString = theDrawString;
        }

        return window;
}

// ---------------------------------------------------------------------------
// the layout block (parseLayoutBlock, parseInit/parseUpdate/parseShutdown)
// ---------------------------------------------------------------------------

static Bool parseInit(const char *buffer, std::string &out)
{
        char *c;
        const char *seps = " \n\r\t";

        c = strtok((char *)buffer, seps);
        out = tokOrNull(c, "layout-callback-token");

        return true;
}

static Bool parseUpdate(const char *buffer, std::string &out)
{
        return parseInit(buffer, out);
}

static Bool parseShutdown(const char *buffer, std::string &out)
{
        return parseInit(buffer, out);
}

static Bool parseLayoutBlock(LoadState &S)
{
        std::string asciitoken;
        if (S.inFile->scanString(asciitoken) == false) {
                return false;
        }

        // better be the layout block
        if (asciitoken != "STARTLAYOUTBLOCK") {
                return false;
        }

        while (true) {
                // get next token — an EOF here spins the original's loop
                // forever (the empty token matches neither the end marker
                // nor any table row)
                if (S.inFile->scanString(asciitoken) == false) {
                        stop("hang:layout-eof");
                }

                // check for end
                if (asciitoken == "ENDLAYOUTBLOCK") {
                        break;
                }

                // search for token in the table (LAYOUTINIT/LAYOUTUPDATE/
                // LAYOUTSHUTDOWN); unknown tokens are skipped WITHOUT
                // consuming their value — the value's tokens follow
                if (asciitoken == "LAYOUTINIT") {
                        readUntilSemicolon(S.inFile, (char *)S.buffer, WIN_BUFFER_LENGTH);
                        char *c = strtok((char *)S.buffer, " =");
                        parseInit(tokOrNull(c, "layout-callback-token"), S.initNameString);
                } else if (asciitoken == "LAYOUTUPDATE") {
                        readUntilSemicolon(S.inFile, (char *)S.buffer, WIN_BUFFER_LENGTH);
                        char *c = strtok((char *)S.buffer, " =");
                        parseUpdate(tokOrNull(c, "layout-callback-token"), S.updateNameString);
                } else if (asciitoken == "LAYOUTSHUTDOWN") {
                        readUntilSemicolon(S.inFile, (char *)S.buffer, WIN_BUFFER_LENGTH);
                        char *c = strtok((char *)S.buffer, " =");
                        parseShutdown(tokOrNull(c, "layout-callback-token"), S.shutdownNameString);
                }
        }

        return true;
}

// ---------------------------------------------------------------------------
// winCreateFromScript (the top-level walk)
// ---------------------------------------------------------------------------

static void winCreateFromScript(LoadState &S)
{
        std::string asciibuf;

        // Reset the window stack and defaults
        resetWindowStack(S);
        // resetWindowDefaults
        S.defEnabledColor = 0;
        S.defDisabledColor = 0;
        S.defBackgroundColor = 0;
        S.defHiliteColor = 0;
        S.defSelectedColor = 0;
        S.defTextColor = 0;
        S.defFontText.clear();

        // read the file version — the return of scanInt is unchecked and
        // `version` is uninitialized in the original on its failure; the
        // transcription (and the port) read it as 0 (the v1 shape)
        S.inFile->read(nullptr, (Int)strlen("FILE_VERSION = "));
        S.inFile->scanInt((Int &)S.version);
        S.inFile->nextLine();

        // version 2+ have a special block called the layout block
        if (S.version >= 2) {
                if (parseLayoutBlock(S) == false) {
                        // "Error parsing layout block" — the load fails
                        // before any window is read
                        g_layout_reached = true;
                        return;
                }
        } else {
                // default none names
                S.initNameString = "[None]";
                S.updateNameString = "[None]";
                S.shutdownNameString = "[None]";
        }
        g_layout_reached = true;

        while (true) {
                if (S.inFile->scanString(asciibuf) == false) {
                        break;
                }

                if (asciibuf == "END") {
                        continue;
                }

                if (asciibuf == "ENABLEDCOLOR") {
                        parseDefaultColor(&S.defEnabledColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "DISABLEDCOLOR") {
                        parseDefaultColor(&S.defDisabledColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "HILITECOLOR") {
                        parseDefaultColor(&S.defHiliteColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "SELECTEDCOLOR") {
                        parseDefaultColor(&S.defSelectedColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "TEXTCOLOR") {
                        parseDefaultColor(&S.defTextColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "BACKGROUNDCOLOR") {
                        parseDefaultColor(&S.defBackgroundColor, S.inFile, (char *)S.buffer);
                } else if (asciibuf == "FONT") {
                        parseDefaultFont(S.inFile, (char *)S.buffer, S.defFontText);
                } else if (asciibuf == "WINDOW") {
                        // Parse window descriptions until the last END is read
                        GameWindow *window = parseWindow(S);
                        // (the return is NOT checked: a failed root pushes
                        // a nullptr entry and the loop continues)
                        S.roots.push_back(window ? window->index : -1);
                }
                // (no else: unknown top-level tokens drop whole — their
                // value tokens become subsequent tokens)
        }

        g_defaults_reached = true;
}

// ---------------------------------------------------------------------------
// the dump (the golden script grammar: file/disp/verdict/ver/layout/
// defcol/deffont/firstwin/root/win/rect/stat/name/cb/font/ht/label/imgoff/
// tcol/ddi/gad/chld-mark-implicit/nk/stream — every line carries the
// corpus name)
// ---------------------------------------------------------------------------

static std::string escapeBytes(const std::string &s)
{
        std::string out;
        for (unsigned char c : s) {
                switch (c) {
                case '\n': out += "\\n"; break;
                case '\r': out += "\\r"; break;
                case '\t': out += "\\t"; break;
                case '"': out += "\\\""; break;
                case '\\': out += "\\\\"; break;
                default:
                        if (c >= 0x20 && c <= 0x7e) {
                                out += (char)c;
                        } else {
                                char b[8];
                                snprintf(b, sizeof(b), "\\x%02x", c);
                                out += b;
                        }
                        break;
                }
        }
        return out;
}

static std::string esc(const std::string &s) { return s.empty() ? "-" : escapeBytes(s); }

static std::string hexBytes(const std::string &s)
{
        static const char *digits = "0123456789abcdef";
        std::string out;
        out.reserve(s.size() * 2);
        for (unsigned char c : s) {
                out.push_back(digits[c >> 4]);
                out.push_back(digits[c & 15]);
        }
        if (out.empty())
                out = "-";
        return out;
}

static const char *imgName(const WinDrawData &d)
{
        return d.image.empty() ? "NoImage" : d.image.c_str();
}

static void dumpDD(const char *corpus, Int idx, const char *field, const WinDrawData *arr)
{
        for (Int i = 0; i < MAX_DRAW_DATA; i++) {
                printf("ddi %s %d %s %d %s %08x %08x\n", corpus, idx, field, i, imgName(arr[i]),
                       arr[i].color, arr[i].borderColor);
        }
}

static void dumpGadget(const char *corpus, Int idx, const Gadget &g)
{
        switch (g.kind) {
        case GADGET_NONE:
                printf("gad %s %d none\n", corpus, idx);
                break;
        case GADGET_RADIO:
                printf("gad %s %d radio %d %d\n", corpus, idx, g.radio.group, g.radio.screen);
                break;
        case GADGET_SLIDER:
                printf("gad %s %d slider %d %d\n", corpus, idx, g.slider.minVal, g.slider.maxVal);
                break;
        case GADGET_LISTBOX: {
                const ListboxData &l = g.listbox;
                std::string colw;
                if (!l.columnWidthPercentage.empty()) {
                        for (size_t i = 0; i < l.columnWidthPercentage.size(); i++) {
                                char b[32];
                                snprintf(b, sizeof(b), "%s%d", i ? "," : "",
                                         l.columnWidthPercentage[i]);
                                colw += b;
                        }
                } else {
                        colw = "-";
                }
                printf("gad %s %d listbox %d %d %d %d %d %d %d %s %d\n", corpus, idx, l.listLength,
                       l.autoScroll ? 1 : 0, l.scrollIfAtEnd ? 1 : 0, l.autoPurge ? 1 : 0,
                       l.scrollBar ? 1 : 0, l.multiSelect ? 1 : 0, l.columns, colw.c_str(),
                       l.forceSelect ? 1 : 0);
                break;
        }
        case GADGET_COMBOBOX: {
                const ComboBoxData &c = g.combobox;
                printf("gad %s %d combo %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d\n", corpus, idx,
                       c.isEditable ? 1 : 0, c.maxChars, c.maxDisplay, c.asciiOnly ? 1 : 0,
                       c.lettersAndNumbersOnly ? 1 : 0, c.entryData.maxTextLen,
                       c.entryData.secretText ? 1 : 0, c.entryData.numericalOnly ? 1 : 0,
                       c.entryData.alphaNumericalOnly ? 1 : 0, c.entryData.aSCIIOnly ? 1 : 0,
                       c.listboxData.listLength, c.listboxData.autoScroll ? 1 : 0,
                       c.listboxData.scrollBar ? 1 : 0, c.listboxData.forceSelect ? 1 : 0,
                       c.listboxData.columns, c.entryCount);
                break;
        }
        case GADGET_ENTRY:
                printf("gad %s %d entry %d %d %d %d %d\n", corpus, idx, g.entry.maxTextLen,
                       g.entry.secretText ? 1 : 0, g.entry.numericalOnly ? 1 : 0,
                       g.entry.alphaNumericalOnly ? 1 : 0, g.entry.aSCIIOnly ? 1 : 0);
                break;
        case GADGET_STATICTEXT:
                printf("gad %s %d static %d %d %d %d\n", corpus, idx, g.text.centered ? 1 : 0,
                       g.text.centeredVertically ? 1 : 0, g.text.leftMargin, g.text.topMargin);
                break;
        case GADGET_TABCONTROL: {
                const TabControlData &t = g.tab;
                std::string panes, subpanes;
                for (Int i = 0; i < NUM_TAB_PANES; i++) {
                        char b[32];
                        snprintf(b, sizeof(b), "%s%d", i ? "," : "", t.subPaneDisabled[i] ? 1 : 0);
                        panes += b;
                        if (t.subPanes[i] != -1) {
                                snprintf(b, sizeof(b), "%s%d", subpanes.empty() ? "" : ",",
                                         t.subPanes[i]);
                                subpanes += b;
                        }
                }
                if (subpanes.empty())
                        subpanes = "-";
                printf("gad %s %d tab %d %d %d %d %d %d %s %s\n", corpus, idx, t.tabOrientation,
                       t.tabEdge, t.tabWidth, t.tabHeight, t.tabCount, t.paneBorder,
                       panes.c_str(), subpanes.c_str());
                break;
        }
        }
}

static void dumpWindowTree(LoadState &S, const char *corpus, Int idx, Int depth)
{
        const GameWindow *w = S.arena[idx];
        Int sx, sy;
        winGetScreenPosition(S.arena, w, &sx, &sy);
        printf("win %s %d %d %d %s %d %d %d\n", corpus, idx, w->parent, depth, w->type.c_str(),
               w->inst.m_id, w->inst.m_owner, (Int)w->children.size());
        printf("rect %s %d %d %d %d %d %d %d\n", corpus, idx, w->x, w->y, w->width, w->height, sx,
               sy);
        printf("stat %s %d %08x %08x\n", corpus, idx, w->inst.m_status, w->inst.m_style);
        printf("name %s %d %s\n", corpus, idx, esc(w->inst.m_decoratedNameString).c_str());
        printf("cb %s %d %s %s %s %s\n", corpus, idx, esc(w->systemCallbackString).c_str(),
               esc(w->inputCallbackString).c_str(), esc(w->tooltipCallbackString).c_str(),
               esc(w->drawCallbackString).c_str());
        if (w->inst.m_hasFont) {
                printf("font %s %d %s %d %d\n", corpus, idx,
                       escapeBytes(w->inst.m_fontName).c_str(), w->inst.m_fontSize,
                       w->inst.m_fontBold);
        } else {
                printf("font %s %d -\n", corpus, idx);
        }
        printf("ht %s %d %s\n", corpus, idx, esc(w->inst.m_headerTemplateName).c_str());
        printf("label %s %d %s %s %d\n", corpus, idx, esc(w->inst.m_textLabelString).c_str(),
               esc(w->inst.m_tooltipString).c_str(), w->inst.m_tooltipDelay);
        printf("imgoff %s %d %d %d\n", corpus, idx, w->inst.m_imageOffsetX,
               w->inst.m_imageOffsetY);
        printf("tcol %s %d %08x %08x %08x %08x %08x %08x\n", corpus, idx,
               w->inst.m_enabledText.color, w->inst.m_enabledText.borderColor,
               w->inst.m_disabledText.color, w->inst.m_disabledText.borderColor,
               w->inst.m_hiliteText.color, w->inst.m_hiliteText.borderColor);
        dumpDD(corpus, idx, "ENABLEDDRAWDATA", w->inst.m_enabledDrawData);
        dumpDD(corpus, idx, "DISABLEDDRAWDATA", w->inst.m_disabledDrawData);
        dumpDD(corpus, idx, "HILITEDRAWDATA", w->inst.m_hiliteDrawData);
        dumpGadget(corpus, idx, w->gadget);

        // the routed composite arrays, snapshotted at creation
        static const struct { const char *slot; const char *en; const char *dis; const char *hil; }
        tokens[] = {
                {"thumb", "SLIDERTHUMBENABLEDDRAWDATA", "SLIDERTHUMBDISABLEDDRAWDATA",
                 "SLIDERTHUMBHILITEDRAWDATA"},
                {"upbtn", "LISTBOXENABLEDUPBUTTONDRAWDATA", "LISTBOXDISABLEDUPBUTTONDRAWDATA",
                 "LISTBOXHILITEUPBUTTONDRAWDATA"},
                {"downbtn", "LISTBOXENABLEDDOWNBUTTONDRAWDATA",
                 "LISTBOXDISABLEDDOWNBUTTONDRAWDATA", "LISTBOXHILITEDOWNBUTTONDRAWDATA"},
                {"slider", "LISTBOXENABLEDSLIDERDRAWDATA", "LISTBOXDISABLEDSLIDERDRAWDATA",
                 "LISTBOXHILITESLIDERDRAWDATA"},
                {"dropdown", "COMBOBOXDROPDOWNBUTTONENABLEDDRAWDATA",
                 "COMBOBOXDROPDOWNBUTTONDISABLEDDRAWDATA",
                 "COMBOBOXDROPDOWNBUTTONHILITEDRAWDATA"},
                {"editbox", "COMBOBOXEDITBOXENABLEDDRAWDATA",
                 "COMBOBOXEDITBOXDISABLEDDRAWDATA", "COMBOBOXEDITBOXHILITEDRAWDATA"},
                {"cblistbox", "COMBOBOXLISTBOXENABLEDDRAWDATA",
                 "COMBOBOXLISTBOXDISABLEDDRAWDATA", "COMBOBOXLISTBOXHILITEDRAWDATA"},
        };
        for (size_t r = 0; r < w->routed.size(); r++) {
                printf("subdd %s %d %s\n", corpus, idx, w->routed[r].slot);
                const char *en = nullptr, *dis = nullptr, *hil = nullptr;
                for (const auto &t : tokens) {
                        if (strcmp(t.slot, w->routed[r].slot) == 0) {
                                en = t.en;
                                dis = t.dis;
                                hil = t.hil;
                                break;
                        }
                }
                dumpDD(corpus, idx, en, w->routed[r].enabled);
                dumpDD(corpus, idx, dis, w->routed[r].disabled);
                dumpDD(corpus, idx, hil, w->routed[r].hilite);
        }

        for (size_t k = 0; k < w->children.size(); k++) {
                dumpWindowTree(S, corpus, w->children[k], depth + 1);
        }
}

static void dumpCorpus(LoadState &S, const char *corpus, const std::string &text, Int dw, Int dh)
{
        printf("file %s %s\n", corpus, hexBytes(text).c_str());
        printf("disp %s %d %d\n", corpus, dw, dh);

        // the verdict word: the stop verdict if any, else the layout load
        // outcome (WindowLayout::load fails when no root window parsed)
        Int firstWin = -1;
        for (size_t i = 0; i < S.roots.size(); i++) {
                if (S.roots[i] != -1) {
                        firstWin = (Int)i;
                        break;
                }
        }
        std::string verdict = g_verdict.word;
        if (verdict.empty()) {
                verdict = (firstWin != -1) ? "ok" : "load-failed";
        }
        printf("verdict %s %s\n", corpus, verdict.c_str());
        printf("ver %s %u\n", corpus, S.version);
        printf("layout %s %s %s %s\n", corpus, esc(S.initNameString).c_str(),
               esc(S.updateNameString).c_str(), esc(S.shutdownNameString).c_str());
        printf("defcol %s %08x %08x %08x %08x %08x %08x\n", corpus, S.defEnabledColor,
               S.defDisabledColor, S.defBackgroundColor, S.defHiliteColor, S.defSelectedColor,
               S.defTextColor);
        printf("deffont %s %s\n", corpus,
               S.defFontText.empty() ? "none" : escapeBytes(S.defFontText).c_str());
        printf("firstwin %s %d\n", corpus, firstWin);
        printf("roots %s %d\n", corpus, (Int)S.roots.size());
        for (size_t i = 0; i < S.roots.size(); i++) {
                printf("root %s %d %s\n", corpus, (Int)i, S.roots[i] != -1 ? "ok" : "failed");
        }

        // the reachable trees
        for (size_t i = 0; i < S.roots.size(); i++) {
                if (S.roots[i] != -1) {
                        dumpWindowTree(S, corpus, S.roots[i], 0);
                }
        }

        // the orphans (the CHILD-twice shape: the earlier window of a
        // double-CHILD stays in the manager's list but outside the roots)
        std::vector<Int> reachable;
        std::vector<Int> stack;
        for (size_t i = 0; i < S.roots.size(); i++) {
                if (S.roots[i] != -1)
                        stack.push_back(S.roots[i]);
        }
        while (!stack.empty()) {
                Int idx = stack.back();
                stack.pop_back();
                reachable.push_back(idx);
                const GameWindow *w = S.arena[idx];
                for (size_t k = w->children.size(); k-- > 0;)
                        stack.push_back(w->children[k]);
        }
        std::vector<Int> orphans;
        for (size_t i = 0; i < S.arena.size(); i++) {
                bool found = false;
                for (size_t k = 0; k < reachable.size(); k++) {
                        if (reachable[k] == (Int)i) {
                                found = true;
                                break;
                        }
                }
                if (!found)
                        orphans.push_back((Int)i);
        }
        if (!orphans.empty()) {
                printf("orphans %s %d\n", corpus, (Int)orphans.size());
                for (size_t i = 0; i < orphans.size(); i++) {
                        // only the subtree roots: an orphan whose parent is
                        // itself orphaned is covered by that parent's dump
                        Int parent = S.arena[orphans[i]]->parent;
                        bool parentOrphaned = false;
                        for (size_t k = 0; k < orphans.size(); k++) {
                                if (orphans[k] == parent) {
                                        parentOrphaned = true;
                                        break;
                                }
                        }
                        if (!parentOrphaned)
                                dumpWindowTree(S, corpus, orphans[i], 0);
                }
        }

        printf("namekeys %s %d\n", corpus, (Int)S.keys.m_names.size());
        for (size_t i = 0; i < S.keys.m_names.size(); i++) {
                printf("nk %s %u %s\n", corpus, S.keys.m_names[i].second,
                       escapeBytes(S.keys.m_names[i].first).c_str());
        }
        printf("stream %s %d %d\n", corpus, (Int)S.inFile->m_pos, (Int)S.inFile->m_size);
}

// ---------------------------------------------------------------------------
// the corpora
// ---------------------------------------------------------------------------

// a nine-line draw-data stanza the way GUIEdit's Save.cpp writes one
// (entry 0 carries the token; the last line ends the field with ';')
static std::string ddStanza(const char *token, const char *img, int r, int g, int b, int a, int br,
                            int bg, int bb, int ba)
{
        std::string s;
        char line[512];
        std::string pad(strlen(token), ' ');
        for (int i = 0; i < 9; i++) {
                int rr = (r + i * 13) % 300; // crosses 255 — the Byte truncation
                int gg = (g + i * 7) % 300;
                int bbv = (b + i * 29) % 300;
                char iname[64];
                snprintf(iname, sizeof(iname), "%s%d", img, i);
                const char *useName = (i % 3 == 0) ? "NoImage" : iname;
                const char *tok = (i == 0) ? token : pad.c_str();
                const char *tail = (i == 8) ? ";" : ",";
                snprintf(line, sizeof(line), "%s%s%s IMAGE: %s, COLOR: %d %d %d %d, "
                                             "BORDERCOLOR: %d %d %d %d%s\n",
                         (i == 0) ? "" : "             ", tok, (i == 0) ? " = " : "   ", useName,
                         rr, gg, bbv, (a + i) % 256, (br + i * 3) % 256, (bg + i * 5) % 256,
                         (bb + i * 11) % 256, (ba + i * 2) % 256, tail);
                s += line;
        }
        return s;
}

static std::string deepNesting(int levels)
{
        // levels nested USER windows, each with a CHILD marker before the
        // next; the innermost closes with END, then every level closes
        // with ENDALLCHILDREN + END
        std::string s = "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n";
        for (int i = 0; i < levels; i++) {
                s += "WINDOW\nWINDOWTYPE = USER;\n";
                s += "SCREENRECT = UPPERLEFT: 10 20,\n             BOTTOMRIGHT: 110 220,\n";
                s += "             CREATIONRESOLUTION: 800 600;\n";
                s += "NAME = \"Deep.wnd:Level" + std::to_string(i) + "\";\n";
                if (i + 1 < levels)
                        s += "CHILD\n";
        }
        s += "END\n";
        for (int i = 1; i < levels; i++)
                s += "ENDALLCHILDREN\nEND\n";
        return s;
}

struct Corpus {
        const char *name;
        Int displayW;
        Int displayH;
        std::string text;
};

int main(int argc, char **argv)
{
        if (argc != 2) {
                fprintf(stderr, "usage: %s <golden-out>\n", argv[0]);
                return 1;
        }
        FILE *out = freopen(argv[1], "w", stdout);
        if (!out) {
                fprintf(stderr, "error: cannot open %s\n", argv[1]);
                return 1;
        }

        printf("# gen_wnd_golden.cpp — GameWindowManagerScript.cpp winCreateFromScript "
               "transcription (baseline 571a7c5).\n");
        printf("# Golden script for rzh-formats::wnd; see tools/differential/README.md.\n");
        printf("# Grammar: file/disp/verdict/ver/layout/defcol/deffont/firstwin/roots/root/"
               "win/rect/stat/name/cb/font/ht/label/imgoff/tcol/ddi/gad/subdd/orphans/nk/stream.\n");

        // --- corpus assembly --------------------------------------------------------
        std::vector<Corpus> corpora;

        auto add = [&](const char *name, Int w, Int h, const std::string &text) {
                corpora.push_back(Corpus{name, w, h, text});
        };

        add("empty", 800, 600, "");

        add("version_v1", 800, 600, "FILE_VERSION = 1;\nEND\n");

        add("v2_layout", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\nEND\n");

        add("layout_callbacks", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\n  LAYOUTINIT = GameMenuInit;\n  LAYOUTUPDATE "
            "= GameMenuUpdate;\n  LAYOUTSHUTDOWN = GameMenuShutdown;\nENDLAYOUTBLOCK\nEND\n");

        add("layout_missing", 800, 600, "FILE_VERSION = 2;\nWINDOW\nEND\n");

        add("layout_badstart", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUT\nENDLAYOUTBLOCK\nEND\n");

        add("layout_eof", 800, 600, "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\n  LAYOUTINIT = x;\n");

        add("layout_unknown", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\n  WHOOPS = 1;\n  LAYOUTINIT = InitFn;\n  "
            "JUNK 42;\n  LAYOUTUPDATE = UpdateFn;\nENDLAYOUTBLOCK\nEND\n");

        add("layout_quoted", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\n  LAYOUTINIT = \"QuotedInit\";\n  LAYOUTUPDATE "
            "= Two Words;\nENDLAYOUTBLOCK\nEND\n");

        add("v1_window", 800, 600,
            "FILE_VERSION = 1;\nWINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 4 5,\n        "
            "     BOTTOMRIGHT: 104 105,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Old.wnd:Root\";\nSTATUS = ENABLED;\nEND\n");

        // the kitchen sink: every scalar field on one USER window
        {
                std::string t = "FILE_VERSION = 3;\nSTARTLAYOUTBLOCK\n  LAYOUTINIT = "
                                "MainMenuInit;\n  LAYOUTSHUTDOWN = MainMenuShutdown;\n"
                                "ENDLAYOUTBLOCK\n";
                t += "WINDOW\nWINDOWTYPE = USER;\n";
                t += "SCREENRECT = UPPERLEFT: 12 34,\n             BOTTOMRIGHT: 412 334,\n       "
                     "      CREATIONRESOLUTION: 800 600;\n";
                t += "NAME = \"Menus.wnd:MainWindow\";\n";
                t += "STATUS = ENABLED+DRAGABLE+SEETHRU;\n";
                t += "STYLE = MOUSETRACK+ANIMATED;\n";
                t += "SYSTEMCALLBACK = \"SystemCB\";\nINPUTCALLBACK = \"InputCB\";\n";
                t += "TOOLTIPCALLBACK = \"TooltipCB\";\nDRAWCALLBACK = \"DrawCB\";\n";
                t += "FONT = NAME: \"Airborne\", SIZE: 14, BOLD: 1;\n";
                t += "HEADERTEMPLATE = \"CampaignHeader\";\n";
                t += "TOOLTIPTEXT = \"Tooltip:Main\";\nTOOLTIPDELAY = 20;\n";
                t += "TEXT = \"GUI:MainWindow\";\n";
                t += "IMAGEOFFSET = 3 7;\nTOOLTIP = ;\n";
                t += "TEXTCOLOR = ENABLED:  255 255 255 255, ENABLEDBORDER:  0 0 0 255,\n";
                t += "            DISABLED: 128 128 128 255, DISABLEDBORDER: 0 0 0 255,\n";
                t += "            HILITE:   200 220 255 255, HILITEBORDER:   64 64 64 255;\n";
                t += ddStanza("ENABLEDDRAWDATA", "ImgE", 10, 20, 30, 255, 1, 2, 3, 255);
                t += ddStanza("DISABLEDDRAWDATA", "ImgD", 100, 110, 120, 254, 5, 6, 7, 253);
                t += ddStanza("HILITEDRAWDATA", "ImgH", 200, 210, 220, 252, 9, 10, 11, 251);
                t += "END\n";
                add("user_full", 800, 600, t);
        }

        // parent with two children, one grandchild, one TABPANE child
        add("user_children", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 100 100,\n             "
            "BOTTOMRIGHT: 500 400,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Menus.wnd:Parent\";\nSTATUS = ENABLED;\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 130 130,\n             "
            "BOTTOMRIGHT: 230 230,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Menus.wnd:ChildA\";\nEND\nENDALLCHILDREN\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 130 260,\n             "
            "BOTTOMRIGHT: 430 360,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Menus.wnd:ChildB\";\nSTATUS = ENABLED+HIDDEN;\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = TABPANE;\nSCREENRECT = UPPERLEFT: 140 270,\n             "
            "BOTTOMRIGHT: 240 320,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Menus.wnd:GrandChild\";\nEND\nENDALLCHILDREN\n"
            "END\nENDALLCHILDREN\nEND\n");

        add("deep_9", 800, 600, deepNesting(9));
        add("deep_10", 800, 600, deepNesting(10));
        add("deep_11", 800, 600, deepNesting(11));

        // the scaling corpora (creation 400x300 on display 800x600 → x2)
        add("scale_x2", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 10 20,\n             "
            "BOTTOMRIGHT: 210 170,\n             CREATIONRESOLUTION: 400 300;\nNAME = "
            "\"S.wnd:W\";\nEND\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: -5 -10,\n             "
            "BOTTOMRIGHT: 55 40,\n             CREATIONRESOLUTION: 400 300;\nEND\n");

        add("scale_odd", 1024, 768,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 100 50,\n             "
            "BOTTOMRIGHT: 350 250,\n             CREATIONRESOLUTION: 300 200;\nNAME = "
            "\"S2.wnd:W\";\nEND\n");

        add("scale_child", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 100 100,\n             "
            "BOTTOMRIGHT: 300 300,\n             CREATIONRESOLUTION: 400 300;\nNAME = "
            "\"SC.wnd:Parent\";\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 150 150,\n             "
            "BOTTOMRIGHT: 250 250,\n             CREATIONRESOLUTION: 400 300;\nNAME = "
            "\"SC.wnd:Child\";\nEND\nENDALLCHILDREN\nEND\n");

        add("rect_inverted", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 400 300,\n             "
            "BOTTOMRIGHT: 100 50,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"R.wnd:Negative\";\nEND\n");

        add("status_flags", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"F.wnd:NullStatus\";\nSTATUS = NULL;\nEND\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"F.wnd:NoneStatus\";\nSTATUS = NONE;\nEND\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"F.wnd:LowerStatus\";\nSTATUS = enabled+dragable+nope;\nEND\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"F.wnd:AllStatus\";\nSTATUS = "
            "ACTIVE+TOGGLE+DRAGABLE+ENABLED+HIDDEN+ABOVE+BELOW+IMAGE+TABSTOP+NOINPUT+NOFOCUS+"
            "DESTROYED+BORDER+SMOOTH_TEXT+ONE_LINE+NO_FLUSH+SEE_THRU+RIGHT_CLICK+WRAP_CENTERED+"
            "CHECK_LIKE+HOTKEY_TEXT+USE_OVERLAY_STATES+NOT_READY+FLASHING+ALWAYS_COLOR+"
            "ON_MOUSE_DOWN;\nEND\n");

        add("style_flags", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"F.wnd:StyleAll\";\nSTYLE = "
            "PUSHBUTTON+RADIOBUTTON+CHECKBOX+VERTSLIDER+HORZSLIDER+SCROLLLISTBOX+ENTRYFIELD+"
            "STATICTEXT+PROGRESSBAR+MOUSETRACK+ANIMATED+TABSTOP+TABCONTROL+TABPANE+COMBOBOX+"
            "BOGUS;\nEND\n");

        add("name_id", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nEND\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Twice.wnd:A\";\nNAME = \"Twice.wnd:B\";\nEND\n");

        // the positional TEXTCOLOR walk with mismatched labels
        add("textcolor_positional", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"TC.wnd:W\";\n"
            "TEXTCOLOR = FOO:  1 2 3 4, BAR: 5 6 7 8,\n            BAZ:  9 10 11 12, QUX: 13 "
            "14 15 16,\n            QUUX: 17 18 19 20, QUUUX: 21 22 23 24;\n"
            "END\n");

        // a short draw-data stanza: the 9-element loop runs off the token
        // supply and the original dereferences the null strtok result
        add("drawdata_short", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"D.wnd:Short\";\n"
            "ENABLEDDRAWDATA = IMAGE: NoImage, COLOR: 1 2 3 4, BORDERCOLOR: 5 6 7 8;\nEND\n");

        // the top-level default colors and the discarded default font
        add("defaults_top", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "ENABLEDCOLOR = 1 2 3;\nDISABLEDCOLOR = TRANSPARENT;\nHILITECOLOR = 300 4 5;\n"
            "SELECTEDCOLOR = 10 20 30;\nTEXTCOLOR = 100 150 200;\nBACKGROUNDCOLOR = 0 0 0;\n"
            "FONT = NAME: \"DefaultFace\", SIZE: 12, BOLD: 0;\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"DC.wnd:After\";\nEND\n");

        // default colors set mid-file and inside a child block: only the
        // later windows see them
        add("defaults_midfile", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"DM.wnd:Before\";\nEND\n"
            "TEXTCOLOR = 255 0 0;\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"DM.wnd:After\";\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 1 1,\n             "
            "BOTTOMRIGHT: 2 2,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"DM.wnd:Child\";\nEND\nENABLEDCOLOR = 9 9 9;\nENDALLCHILDREN\n"
            "END\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"DM.wnd:Last\";\nEND\n");

        // the plain gadget style bits
        add("gadget_zoo", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = PUSHBUTTON;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 60 20,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Z.wnd:Btn\";\nEND\n"
            "WINDOW\nWINDOWTYPE = CHECKBOX;\nSCREENRECT = UPPERLEFT: 0 30,\n             "
            "BOTTOMRIGHT: 60 50,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Z.wnd:Check\";\nEND\n"
            "WINDOW\nWINDOWTYPE = PROGRESSBAR;\nSCREENRECT = UPPERLEFT: 0 60,\n             "
            "BOTTOMRIGHT: 60 70,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Z.wnd:Prog\";\nEND\n"
            "WINDOW\nWINDOWTYPE = TABPANE;\nSCREENRECT = UPPERLEFT: 0 80,\n             "
            "BOTTOMRIGHT: 60 90,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Z.wnd:Pane\";\nEND\n");

        // the radio button: the group parse and the screen id from the
        // name's filename prefix
        add("gadget_radio", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = RADIOBUTTON;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 60 20,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Radio.wnd:ChoiceA\";\nRADIOBUTTONDATA = GROUP: 3;\nEND\n"
            "WINDOW\nWINDOWTYPE = RADIOBUTTON;\nSCREENRECT = UPPERLEFT: 0 30,\n             "
            "BOTTOMRIGHT: 60 50,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Radio.wnd:ChoiceB\";\nRADIOBUTTONDATA = GROUP: 3;\nEND\n"
            "WINDOW\nWINDOWTYPE = RADIOBUTTON;\nSCREENRECT = UPPERLEFT: 0 60,\n             "
            "BOTTOMRIGHT: 60 80,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"NoColonName\";\nRADIOBUTTONDATA = GROUP: 7;\nEND\n");

        // the slider: SLIDERDATA and the thumb draw data routing
        {
                std::string t = "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
                                "WINDOW\nWINDOWTYPE = VERTSLIDER;\nSCREENRECT = UPPERLEFT: 10 "
                                "10,\n             BOTTOMRIGHT: 30 210,\n             "
                                "CREATIONRESOLUTION: 800 600;\nNAME = \"Sl.wnd:V\";\n"
                                "SLIDERDATA = MINVALUE: 0,\n             MAXVALUE: 100;\n";
                t += ddStanza("SLIDERTHUMBENABLEDDRAWDATA", "Thumb", 50, 60, 70, 255, 1, 1, 1, 255);
                t += ddStanza("SLIDERTHUMBDISABLEDDRAWDATA", "ThumbD", 60, 70, 80, 255, 2, 2, 2,
                              255);
                t += ddStanza("SLIDERTHUMBHILITEDRAWDATA", "ThumbH", 70, 80, 90, 255, 3, 3, 3, 255);
                t += "END\n";
                add("gadget_slider", 800, 600, t);
        }

        // the listbox: LISTBOXDATA with columns, the composite draw data
        // fields, and a second listbox WITHOUT them (the stale routing)
        {
                std::string t = "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
                                "WINDOW\nWINDOWTYPE = SCROLLLISTBOX;\nSCREENRECT = UPPERLEFT: "
                                "10 10,\n             BOTTOMRIGHT: 210 310,\n             "
                                "CREATIONRESOLUTION: 800 600;\nNAME = \"Lb.wnd:Main\";\n"
                                "LISTBOXDATA = LENGTH: 12,\n              AUTOSCROLL: 1,\n     "
                                "         SCROLLIFATEND: 1,\n              AUTOPURGE: 0,\n        "
                                "      SCROLLBAR: 1,\n              MULTISELECT: 1,\n            "
                                "  COLUMNS: 3,\n              COLUMNSWIDTH%: 50,\n             "
                                " COLUMNSWIDTH%: 30,\n              COLUMNSWIDTH%: 20,\n        "
                                "      FORCESELECT: 0;\n";
                t += ddStanza("LISTBOXENABLEDUPBUTTONDRAWDATA", "UpBtn", 40, 50, 60, 255, 1, 2, 3,
                              255);
                t += ddStanza("LISTBOXDISABLEDUPBUTTONDRAWDATA", "UpBtnD", 41, 51, 61, 254, 4, 5,
                              6, 254);
                t += ddStanza("LISTBOXHILITEUPBUTTONDRAWDATA", "UpBtnH", 42, 52, 62, 253, 7, 8, 9,
                              253);
                t += ddStanza("LISTBOXENABLEDDOWNBUTTONDRAWDATA", "DnBtn", 43, 53, 63, 252, 10,
                              11, 12, 252);
                t += ddStanza("LISTBOXDISABLEDDOWNBUTTONDRAWDATA", "DnBtnD", 44, 54, 64, 251, 13,
                              14, 15, 251);
                t += ddStanza("LISTBOXHILITEDOWNBUTTONDRAWDATA", "DnBtnH", 45, 55, 65, 250, 16,
                              17, 18, 250);
                t += ddStanza("LISTBOXENABLEDSLIDERDRAWDATA", "LbSlider", 46, 56, 66, 249, 19, 20,
                              21, 249);
                t += ddStanza("LISTBOXDISABLEDSLIDERDRAWDATA", "LbSliderD", 47, 57, 67, 248, 22,
                              23, 24, 248);
                t += ddStanza("LISTBOXHILITESLIDERDRAWDATA", "LbSliderH", 48, 58, 68, 247, 25,
                              26, 27, 247);
                t += ddStanza("SLIDERTHUMBENABLEDDRAWDATA", "LbThumb", 49, 59, 69, 246, 28, 29,
                              30, 246);
                t += ddStanza("SLIDERTHUMBDISABLEDDRAWDATA", "LbThumbD", 50, 60, 70, 245, 31, 32,
                              33, 245);
                t += ddStanza("SLIDERTHUMBHILITEDRAWDATA", "LbThumbH", 51, 61, 71, 244, 34, 35,
                              36, 244);
                t += "END\n";
                // the stale-routing follow-up: no composite fields of its own
                t += "WINDOW\nWINDOWTYPE = SCROLLLISTBOX;\nSCREENRECT = UPPERLEFT: 10 320,\n     "
                     "        BOTTOMRIGHT: 210 420,\n             CREATIONRESOLUTION: 800 600;\n"
                     "NAME = \"Lb.wnd:Bare\";\nEND\n";
                add("gadget_listbox", 800, 600, t);
        }

        // a one-column listbox (the columnWidthPercentage stays null)
        add("gadget_listbox_nocols", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = SCROLLLISTBOX;\nSCREENRECT = UPPERLEFT: 5 5,\n             "
            "BOTTOMRIGHT: 105 55,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"L1.wnd:One\";\n"
            "LISTBOXDATA = LENGTH: 5,\n              AUTOSCROLL: 0,\n              AUTOPURGE: "
            "1,\n              SCROLLBAR: 0,\n              MULTISELECT: 0,\n              "
            "COLUMNS: 1,\n              FORCESELECT: 1;\nEND\n");

        // the combo box: COMBOBOXDATA and the whole composite family
        {
                std::string t = "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
                                "WINDOW\nWINDOWTYPE = COMBOBOX;\nSCREENRECT = UPPERLEFT: 20 "
                                "20,\n             BOTTOMRIGHT: 220 60,\n             "
                                "CREATIONRESOLUTION: 800 600;\nNAME = \"Cb.wnd:Pick\";\n"
                                "COMBOBOXDATA = ISEDITABLE: 1,\n               MAXCHARS: 32,\n "
                                "              MAXDISPLAY: 6,\n               ASCIIONLY: 1,\n  "
                                "             LETTERSANDNUMBERS: 0;\n";
                t += ddStanza("COMBOBOXDROPDOWNBUTTONENABLEDDRAWDATA", "Drop", 60, 70, 80, 255, 1,
                              2, 3, 255);
                t += ddStanza("COMBOBOXDROPDOWNBUTTONDISABLEDDRAWDATA", "DropD", 61, 71, 81, 254,
                              4, 5, 6, 254);
                t += ddStanza("COMBOBOXDROPDOWNBUTTONHILITEDRAWDATA", "DropH", 62, 72, 82, 253, 7,
                              8, 9, 253);
                t += ddStanza("COMBOBOXEDITBOXENABLEDDRAWDATA", "Edit", 63, 73, 83, 252, 10, 11,
                              12, 252);
                t += ddStanza("COMBOBOXEDITBOXDISABLEDDRAWDATA", "EditD", 64, 74, 84, 251, 13, 14,
                              15, 251);
                t += ddStanza("COMBOBOXEDITBOXHILITEDRAWDATA", "EditH", 65, 75, 85, 250, 16, 17,
                              18, 250);
                t += ddStanza("COMBOBOXLISTBOXENABLEDDRAWDATA", "CbList", 66, 76, 86, 249, 19, 20,
                              21, 249);
                t += ddStanza("COMBOBOXLISTBOXDISABLEDDRAWDATA", "CbListD", 67, 77, 87, 248, 22,
                              23, 24, 248);
                t += ddStanza("COMBOBOXLISTBOXHILITEDRAWDATA", "CbListH", 68, 78, 88, 247, 25, 26,
                              27, 247);
                t += "END\n";
                add("gadget_combo", 800, 600, t);
        }

        // the tab control: TABCONTROLDATA and the reverse sub-pane fixup
        add("gadget_tab", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = TABCONTROL;\nSCREENRECT = UPPERLEFT: 30 30,\n             "
            "BOTTOMRIGHT: 430 330,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Tb.wnd:Tabs\";\n"
            "TABCONTROLDATA = TABORIENTATION: 1,\n                 TABEDGE: 2,\n              "
            "   TABWIDTH: 60,\n                 TABHEIGHT: 18,\n                 TABCOUNT: 3,\n "
            "                PANEBORDER: 2,\n                 PANEDISABLED: 3 1 0 0 0 0 0 0;\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = TABPANE;\nSCREENRECT = UPPERLEFT: 40 60,\n             "
            "BOTTOMRIGHT: 140 160,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Tb.wnd:Pane0\";\nEND\nENDALLCHILDREN\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = TABPANE;\nSCREENRECT = UPPERLEFT: 150 60,\n             "
            "BOTTOMRIGHT: 250 160,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Tb.wnd:Pane1\";\nEND\nENDALLCHILDREN\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = TABPANE;\nSCREENRECT = UPPERLEFT: 260 60,\n             "
            "BOTTOMRIGHT: 360 160,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"Tb.wnd:Pane2\";\nEND\nENDALLCHILDREN\n"
            "END\n");

        // PANEDISABLED with nine entries: the original writes past
        // subPaneDisabled[8]
        add("gadget_tab_oob", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = TABCONTROL;\nSCREENRECT = UPPERLEFT: 30 30,\n             "
            "BOTTOMRIGHT: 430 330,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"T9.wnd:Tabs\";\n"
            "TABCONTROLDATA = TABORIENTATION: 0,\n                 TABEDGE: 0,\n              "
            "   TABWIDTH: 0,\n                 TABHEIGHT: 0,\n                 TABCOUNT: 0,\n  "
            "               PANEBORDER: 0,\n                 PANEDISABLED: 9 0 0 0 0 0 0 0 0 "
            "1;\nEND\n");

        // the entry field and the static text
        add("gadget_entry_static", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = ENTRYFIELD;\nSCREENRECT = UPPERLEFT: 10 10,\n             "
            "BOTTOMRIGHT: 210 30,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"E.wnd:Field\";\n"
            "TEXTENTRYDATA = MAXLEN: 31,\n                SECRETTEXT: 1,\n                "
            "NUMERICALONLY: 0,\n                ALPHANUMERICALONLY: 0,\n                "
            "ASCIIONLY: 0;\nEND\n"
            "WINDOW\nWINDOWTYPE = STATICTEXT;\nSCREENRECT = UPPERLEFT: 10 40,\n             "
            "BOTTOMRIGHT: 210 60,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"E.wnd:Text\";\n"
            "STATICTEXTDATA = CENTERED: 1;\nEND\n");

        // the obsolete DATA field, every handled type
        add("data_field", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = VERTSLIDER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 20 200,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"D.wnd:Slider\";\nDATA = 0 100;\nEND\n"
            "WINDOW\nWINDOWTYPE = SCROLLLISTBOX;\nSCREENRECT = UPPERLEFT: 0 210,\n             "
            "BOTTOMRIGHT: 200 410,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"D.wnd:List\";\nDATA = 9 1 0 1 0 1;\nEND\n"
            "WINDOW\nWINDOWTYPE = ENTRYFIELD;\nSCREENRECT = UPPERLEFT: 0 420,\n             "
            "BOTTOMRIGHT: 200 440,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"D.wnd:Entry\";\nDATA = 16 0 3;\nEND\n"
            "WINDOW\nWINDOWTYPE = STATICTEXT;\nSCREENRECT = UPPERLEFT: 0 450,\n             "
            "BOTTOMRIGHT: 200 470,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"D.wnd:Text\";\nDATA = 1 labelignored;\nEND\n"
            "WINDOW\nWINDOWTYPE = RADIOBUTTON;\nSCREENRECT = UPPERLEFT: 0 480,\n             "
            "BOTTOMRIGHT: 200 500,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"D.wnd:Radio\";\nDATA = 4;\nEND\n");

        // DATA after LISTBOXDATA replaces the parsed data whole
        add("data_after_listbox", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = SCROLLLISTBOX;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 200 200,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"DL.wnd:Both\";\n"
            "LISTBOXDATA = LENGTH: 11,\n              AUTOSCROLL: 1,\n              "
            "SCROLLIFATEND: 1,\n              AUTOPURGE: 1,\n              SCROLLBAR: 1,\n       "
            "       MULTISELECT: 1,\n              COLUMNS: 2,\n              COLUMNSWIDTH%: "
            "60,\n              COLUMNSWIDTH%: 40,\n              FORCESELECT: 1;\n"
            "DATA = 7 0 1 0 1 0;\nEND\n");

        // DATA with no tokens: atoi(nullptr)
        add("data_short", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = VERTSLIDER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 20 200,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"DS.wnd:S\";\nDATA = ;\nEND\n");

        // DATA on a TABCONTROL: the null data pointer reaches the gadget
        // construction
        add("data_tabcontrol", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = TABCONTROL;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 200 200,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"DT.wnd:T\";\nDATA = 1;\nEND\n");

        // a gadget-data field on a USER window: the null template
        add("data_null_template", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 100 100,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"NU.wnd:U\";\nSLIDERDATA = MINVALUE: 1,\n             MAXVALUE: 2;\nEND\n");

        // the bare TOOLTIP: its unchecked eat-'=' swallows the next field
        // token whole (TEXT here) and the field after it loses its value
        add("tooltip_bare", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"TB.wnd:W\";\n"
            "TOOLTIP;\nTEXT = \"GUI:Swallowed\";\nTOOLTIPDELAY = 9;\nEND\n");

        // the too-long labels (the MAX_TEXT_LABEL refusal cascades)
        {
                std::string longText(128, 'x');
                std::string longTip(130, 'y');
                std::string t = "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n";
                t += "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
                     "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
                     "\"TL.wnd:First\";\nEND\n";
                t += "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
                     "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
                     "\"TL.wnd:Text\";\nTEXT = \"" +
                     longText + "\";\nEND\n";
                t += "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
                     "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
                     "\"TL.wnd:Tip\";\nTOOLTIPTEXT = \"" +
                     longTip + "\";\nEND\n";
                t += "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
                     "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
                     "\"TL.wnd:Never\";\nEND\n";
                add("label_too_long", 800, 600, t);
        }

        // a failed parse deep in a child tree kills the whole root
        {
                std::string longLabel(140, 'z');
                std::string t = "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n";
                t += "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
                     "BOTTOMRIGHT: 100 100,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
                     "\"CF.wnd:Root\";\n";
                t += "CHILD\n";
                t += "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 10 10,\n             "
                     "BOTTOMRIGHT: 50 50,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
                     "\"CF.wnd:Kid\";\n";
                t += "CHILD\n";
                t += "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 12 12,\n             "
                     "BOTTOMRIGHT: 30 30,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
                     "\"CF.wnd:Grand\";\nTEXT = \"";
                t += longLabel;
                t += "\";\nEND\nENDALLCHILDREN\nEND\nENDALLCHILDREN\nEND\n";
                add("child_fail_cascade", 800, 600, t);
        }
        // the grandchild never closes: EOF inside its field loop
        add("eof_in_window", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 100 100,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"EOF.wnd:W\";\nSTATUS = ENABLED;\n");

        // the double CHILD: the first window is orphaned
        add("child_twice", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 200 200,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"C2.wnd:Orphan\";\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 5 5,\n             "
            "BOTTOMRIGHT: 25 25,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"C2.wnd:OrphanKid\";\nEND\nENDALLCHILDREN\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 10 10,\n             "
            "BOTTOMRIGHT: 30 30,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"C2.wnd:Kept\";\nEND\nENDALLCHILDREN\n"
            "END\n");

        // unknown tokens at the three levels (dropped whole at the top and
        // in children; eaten to the ';' inside a window's field list)
        add("unknown_fields", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "TOPLEVELJUNK 1 2 3\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"U.wnd:W\";\nSOMEUNKNOWNFIELD = whatever values here;\nSTATUS = ENABLED;\nEND\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"U.wnd:Parent\";\n"
            "CHILD\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 1 1,\n             "
            "BOTTOMRIGHT: 2 2,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"U.wnd:Kid\";\nEND\nCHILDJUNK 9 9\nENDALLCHILDREN\nEND\n");

        // ENDALLCHILDREN and stray END tokens at the top level
        add("stray_ends", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "ENDALLCHILDREN\nEND\nEND\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"SE.wnd:W\";\nEND\n");

        // an unknown WINDOWTYPE: createWindow hands back null and the root
        // fails (the nullptr push)
        add("windowtype_unknown", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = HOLOGRAM;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"WT.wnd:X\";\nEND\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = "
            "\"WT.wnd:Ok\";\nEND\n");

        // the WINDOWTYPE read at EOF: the null strtok token
        add("windowtype_eof", 800, 600, "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\nWINDOW\n");

        // an empty quoted NAME: the null strtok token
        add("name_empty", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME = \"\";\nEND\n");

        // a missing '=': the "eaten" token is the value and the field
        // content shifts
        add("no_equals", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 0 0,\n             "
            "BOTTOMRIGHT: 10 10,\n             CREATIONRESOLUTION: 800 600;\nNAME \"Menus.wnd:"
            "Shifted\";\nEND\n");

        // the version family
        add("version_zero", 800, 600, "FILE_VERSION = 0;\nEND\n");
        add("version_big", 800, 600, "FILE_VERSION = 99;\nEND\n");
        add("version_neg", 800, 600, "FILE_VERSION = -1;\nEND\n");
        add("version_garbage", 800, 600, "FILE_VERSION = abc;\nEND\n");
        add("version_nospace", 800, 600, "FILE_VERSION=2;\nEND\n");

        // CRLF and tab separators
        add("crlf_tabs", 800, 600,
            "FILE_VERSION\t=\t2;\r\nSTARTLAYOUTBLOCK\r\n\tLAYOUTINIT = XInit;\r\n"
            "ENDLAYOUTBLOCK\r\nWINDOW\r\nWINDOWTYPE\t=\tUSER;\r\nSCREENRECT = UPPERLEFT: 1 "
            "2,\r\n             BOTTOMRIGHT: 41 42,\r\n             CREATIONRESOLUTION: 800 "
            "600;\r\nNAME = \"CRLF.wnd:W\";\r\nSTATUS = ENABLED;\r\nEND\r\n");

        // the missing SCREENRECT value words
        add("screenrect_short", 800, 600,
            "FILE_VERSION = 2;\nSTARTLAYOUTBLOCK\nENDLAYOUTBLOCK\n"
            "WINDOW\nWINDOWTYPE = USER;\nSCREENRECT = UPPERLEFT: 1 2;\nEND\n");

        // --- run every corpus -------------------------------------------------------

        for (const Corpus &c : corpora) {
                if (getenv("WND_TRACE"))
                        fprintf(stderr, "corpus %s\n", c.name);
                RamFile file((const unsigned char *)c.text.data(), c.text.size());
                LoadState S(&file, c.displayW, c.displayH);
                g_state = &S;
                g_verdict.word.clear();
                try {
                        winCreateFromScript(S);
                } catch (WalkStop &) {
                        // the verdict word is already recorded
                }
                g_state = nullptr;
                dumpCorpus(S, c.name, c.text, c.displayW, c.displayH);

                for (size_t i = 0; i < S.arena.size(); i++)
                        delete S.arena[i];
        }

        fflush(out);
        return 0;
}
