// SPDX-License-Identifier: GPL-3.0-or-later
//
// Golden-vector driver for `rzh-formats::csf` (parity level P0).
//
// Two verbatim transcriptions from the baseline tree feed one golden
// script that the Rust test replays byte-for-byte:
//
//   writer — Core/Tools/Babylon/expimp.cpp: `generate_csf`,
//     `writeCSFLabel`, `writeCSFString` and the LANGINFO table from
//     TransDB.cpp (the trailing language letter on wave chunks).
//     The Babylon *tool* transforms (`StripSpaces`, `ConvertMetaChars`)
//     are omitted deliberately: they are editor-side content
//     normalizations, not format semantics, and every corpus below is
//     chosen so they would be no-ops — where the two agree
//     byte-for-byte. `writeCSFString`'s on-disk inversion and the
//     length bookkeeping (wave_len = strlen + 1) are kept verbatim.
//
//   reader — Core/GameEngine/Source/GameClient/GameText.cpp:
//     `parseCSF` (the invert-until-zero decode, the first-string-only
//     rule, the C-string views of label/text/speech, the trailing-byte
//     acceptance) and `stripSpaces`. The fixed `m_buffer`/`m_tbuffer`
//     member buffers are transcribed as real arrays; corpora never
//     enter the shapes where the C++ would read out of bounds or reuse
//     stale buffer content (the Rust port refuses those with errors —
//     unit-pinned in `crates/rzh-formats/tests/properties.rs`).
//
// Output grammar (one command per line, hex lowercase, '-' = empty):
//
//   file <id> <hex>                          the corpus bytes
//   gen  <id> <hex>                          the transcribed writer's bytes
//                                            (writer corpora only; must
//                                            equal `file`)
//   ok   <id> / err <id>                     the transcribed reader's verdict
//   count <id> <n>                           labels the reader accepted
//   view <id> <i> <label> <text> <speech>    the game-visible strings
//                                            (C-string views: label/speech
//                                            bytes up to NUL, text as
//                                            UTF-16LE units up to 0x0000,
//                                            after the quirk decode +
//                                            stripSpaces)
//   info <id> <langid|->                     `getCSFInfo`: the language the
//                                            runtime selects ('-' when the
//                                            file is not a CSF at all)
//
// Usage: gen_csf_golden <output-file>
//
// Temporary differential-testing infrastructure: removed at cut-over (M5).
//
// Rust port Copyright 2026 the RustZeroHour contributors.
//
// This program is free software; you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at your
// option) any later version.

#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

// --- the format constants (expimp.h / GameText.cpp) --------------------------

static const int CSF_ID = ('C' << 24) | ('S' << 16) | ('F' << 8) | (' ');
static const int CSF_LABEL = ('L' << 24) | ('B' << 16) | ('L' << 8) | (' ');
static const int CSF_STRING = ('S' << 24) | ('T' << 16) | ('R' << 8) | (' ');
static const int CSF_STRINGWITHWAVE = ('S' << 24) | ('T' << 16) | ('R' << 8) | ('W');
static const int CSF_VERSION = 3;

// WideChar is 16-bit on the game's platform; pin it explicitly (Linux
// wchar_t is 32-bit and the transcription must not depend on it).
typedef uint16_t WideChar;

static const int MAX_UITEXT_LENGTH = 10 * 1024; // GameText.cpp

// --- little-endian byte sink (stands in for the C++ FILE* fwrites) -----------

typedef std::vector<uint8_t> Sink;

static void put_bytes(Sink &file, const void *data, size_t len)
{
        const uint8_t *p = (const uint8_t *)data;
        file.insert(file.end(), p, p + len);
}

static void put_int(Sink &file, int value)
{
        uint8_t b[4] = {
                (uint8_t)(value & 0xff), (uint8_t)((value >> 8) & 0xff),
                (uint8_t)((value >> 16) & 0xff), (uint8_t)((value >> 24) & 0xff)
        };
        put_bytes(file, b, 4);
}

static void put_u16(Sink &file, uint16_t value)
{
        uint8_t b[2] = {(uint8_t)(value & 0xff), (uint8_t)((value >> 8) & 0xff)};
        put_bytes(file, b, 2);
}

static void put_wide(Sink &file, const WideChar *string, size_t units)
{
        for (size_t i = 0; i < units; ++i)
        {
                put_u16(file, string[i]);
        }
}

// --- the writer transcription (Babylon expimp.cpp) ---------------------------

// TransDB.cpp: static LANGINFO langinfo[] — only the US row is exercised.
typedef struct
{
        int langid;
        const char *name;
        const char *initials;
        const char *character; // single character identifier

} LANGINFO;

static LANGINFO langinfo[] = {
        { 0 /* LANGID_US */, "US", "us", "e" },
};

static LANGINFO *GetLangInfo(int index)
{
        if (index >= 0 && index < (int)(sizeof(langinfo) / sizeof(langinfo[0])))
        {
                return &langinfo[index];
        }
        return nullptr;
}

static WideChar olebuf[MAX_UITEXT_LENGTH * 2]; // Babylon's shared scratch buffer

// The corpus database: a label with its strings (text + optional wave).
struct BabylonText
{
        std::vector<WideChar> text; // no embedded NUL (C-string semantics)
        std::string wave;           // "" = plain " RTS" chunk
};

struct BabylonLabel
{
        std::string name; // single-byte label
        std::vector<BabylonText> texts;
};

// writeCSFString, verbatim minus the tool transforms (see file comment).
static int writeCSFString(Sink &file, const std::vector<WideChar> &string,
                          const std::string &wave, LANGINFO *linfo)
{
        int id = CSF_STRING;
        size_t wlen = wave.size();

        if (wlen)
        {
                id = CSF_STRINGWITHWAVE;
        }

        // wcscpy(olebuf, string) — string has no embedded NUL.
        for (size_t i = 0; i < string.size(); ++i)
        {
                olebuf[i] = string[i];
        }
        olebuf[string.size()] = 0;
        // StripSpaces(olebuf); ConvertMetaChars(olebuf);  — omitted (tool transforms)
        size_t len = string.size();

        {
                WideChar *ptr = olebuf;

                while (*ptr)
                {
                        *ptr = ~*ptr;
                        ptr++;
                }
        }

        put_int(file, id);
        put_int(file, (int)len);

        if (len)
        {
                put_wide(file, olebuf, len);
        }

        if (wlen)
        {
                wlen++;
                put_int(file, (int)wlen);
                put_bytes(file, wave.data(), wlen - 1); // the path, no NUL
                put_bytes(file, linfo->character, 1);   // the language letter
        }

        return 1;
}

// writeCSFLabel, verbatim (including the empty-name refusal).
static int writeCSFLabel(Sink &file, const BabylonLabel &label)
{
        int id = CSF_LABEL;
        int len = (int)label.name.size();
        int strings = (int)label.texts.size();

        put_int(file, id);
        put_int(file, strings);
        put_int(file, len);

        if (!len)
        {
                return 0;
        }

        put_bytes(file, label.name.data(), (size_t)len);

        return 1;
}

// generate_csf, verbatim: header first, body with counting, header rewrite.
static bool generate_csf(const std::vector<BabylonLabel> &db, int langid, Sink &file)
{
        LANGINFO *linfo = GetLangInfo(langid);

        int header_offset = (int)file.size();
        put_int(file, CSF_ID);
        put_int(file, CSF_VERSION);
        put_int(file, 0); // num_labels, patched below
        put_int(file, 0); // num_strings, patched below
        put_int(file, 0); // skip
        put_int(file, langid);

        int num_labels = 0;
        int num_strings = 0;

        for (const BabylonLabel &label : db)
        {
                if (!writeCSFLabel(file, label))
                {
                        return false;
                }
                num_labels++;

                for (const BabylonText &text : label.texts)
                {
                        if (!writeCSFString(file, text.text, text.wave, linfo))
                        {
                                return false;
                        }
                        num_strings++;
                }
        }

        // fseek back and rewrite the header with the final counts.
        file[header_offset + 8] = (uint8_t)(num_labels & 0xff);
        file[header_offset + 9] = (uint8_t)((num_labels >> 8) & 0xff);
        file[header_offset + 10] = (uint8_t)((num_labels >> 16) & 0xff);
        file[header_offset + 11] = (uint8_t)((num_labels >> 24) & 0xff);
        file[header_offset + 12] = (uint8_t)(num_strings & 0xff);
        file[header_offset + 13] = (uint8_t)((num_strings >> 8) & 0xff);
        file[header_offset + 14] = (uint8_t)((num_strings >> 16) & 0xff);
        file[header_offset + 15] = (uint8_t)((num_strings >> 24) & 0xff);

        return true;
}

// --- the reader transcription (GameText.cpp) ---------------------------------

typedef std::vector<uint8_t> Bytes;

struct Cursor
{
        const uint8_t *data;
        size_t size;
        size_t pos;
};

// File::read semantics for the pinned paths: true iff exactly len bytes.
static bool cursor_read(Cursor &c, void *dst, size_t len)
{
        if (c.size - c.pos < len)
        {
                return false;
        }
        if (dst)
        {
                memcpy(dst, c.data + c.pos, len);
        }
        c.pos += len;
        return true;
}

static int cursor_read_int(Cursor &c, bool *ok)
{
        uint8_t b[4];
        *ok = cursor_read(c, b, 4);
        if (!*ok)
        {
                return 0;
        }
        return (int)((uint32_t)b[0] | ((uint32_t)b[1] << 8) | ((uint32_t)b[2] << 16) |
                     ((uint32_t)b[3] << 24));
}

// GameTextManager::stripSpaces, verbatim.
static void stripSpaces(WideChar *string)
{
        WideChar *str, *ptr;
        WideChar ch, last = 0;
        int skipall = 1;

        str = ptr = string;

        while ((ch = *ptr++) != 0)
        {
                if (ch == ' ')
                {
                        if (last == ' ' || skipall)
                        {
                                continue;
                        }
                }

                if (ch == '\n' || ch == '\t')
                {
                        // remove last space
                        if (last == ' ')
                        {
                                str--;
                        }

                        skipall = 1; // skip all spaces
                        last = *str++ = ch;
                        continue;
                }

                last = *str++ = ch;
                skipall = 0;
        }

        if (last == ' ')
        {
                str--;
        }

        *str = 0;
}

// The game-visible strings of one accepted label (StringInfo).
struct StringInfo
{
        std::vector<uint8_t> label;  // AsciiString = C string view
        std::vector<WideChar> text;  // UnicodeString = C string view
        std::vector<uint8_t> speech; // AsciiString = C string view
};

// GameTextManager::parseCSF, verbatim over a memory cursor. Returns the
// accepted labels; `ok` mirrors parseCSF's return value. The fixed member
// buffers are transcribed as real arrays; corpora never overflow them.
static bool parseCSF(Cursor &file, std::vector<StringInfo> &out)
{
        uint8_t header[24];
        if (!cursor_read(file, header, sizeof(header)))
        {
                return false;
        }

        int id;
        int len;
        bool ok;

        out.clear();

        char m_buffer[MAX_UITEXT_LENGTH];
        WideChar m_tbuffer[MAX_UITEXT_LENGTH * 2];

        for (;;)
        {
                if (!cursor_read(file, &id, sizeof(id)))
                {
                        break; // while (file->read(&id, 4) == 4)
                }

                if (id != CSF_LABEL)
                {
                        return false; // goto quit
                }

                int num_strings = cursor_read_int(file, &ok);
                if (!ok)
                {
                        return false; // unpinned short-read zone; corpora avoid it
                }

                len = cursor_read_int(file, &ok);
                if (!ok)
                {
                        return false;
                }

                if (len)
                {
                        if (!cursor_read(file, m_buffer, (size_t)len))
                        {
                                return false;
                        }
                }

                m_buffer[len] = 0;

                StringInfo info;
                info.label.assign(m_buffer, m_buffer + strlen(m_buffer));

                int num = 0;

                while (num < num_strings)
                {
                        id = cursor_read_int(file, &ok);
                        if (!ok)
                        {
                                return false;
                        }

                        if (id != CSF_STRING && id != CSF_STRINGWITHWAVE)
                        {
                                return false; // goto quit
                        }

                        len = cursor_read_int(file, &ok);
                        if (!ok)
                        {
                                return false;
                        }

                        if (len)
                        {
                                if (!cursor_read(file, m_tbuffer, (size_t)len * sizeof(WideChar)))
                                {
                                        return false;
                                }
                        }

                        if (num == 0)
                        {
                                // only use the first string found
                                m_tbuffer[len] = 0;

                                {
                                        WideChar *ptr;

                                        ptr = m_tbuffer;

                                        while (*ptr)
                                        {
                                                *ptr = ~*ptr;
                                                ptr++;
                                        }
                                }

                                stripSpaces(m_tbuffer);
                                // UnicodeString operator= copies up to the NUL:
                                for (WideChar *p = m_tbuffer; *p; ++p)
                                {
                                        info.text.push_back(*p);
                                }
                        }

                        if (id == CSF_STRINGWITHWAVE)
                        {
                                len = cursor_read_int(file, &ok);
                                if (!ok)
                                {
                                        return false;
                                }
                                if (len)
                                {
                                        if (!cursor_read(file, m_buffer, (size_t)len))
                                        {
                                                return false;
                                        }
                                }
                                m_buffer[len] = 0;

                                if (num == 0 && len)
                                {
                                        // only use the first string found
                                        info.speech.assign(m_buffer, m_buffer + strlen(m_buffer));
                                }
                        }

                        num++;
                }

                out.push_back(info);
        }

        return true;
}

// GameTextManager::getCSFInfo: header sniff + language selection.
static bool getCSFInfo(const Bytes &bytes, int *language)
{
        if (bytes.size() < 24)
        {
                return false;
        }

        int id = (int)((uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
                       ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24));
        int version = (int)((uint32_t)bytes[4] | ((uint32_t)bytes[5] << 8) |
                            ((uint32_t)bytes[6] << 16) | ((uint32_t)bytes[7] << 24));
        int langid = (int)((uint32_t)bytes[20] | ((uint32_t)bytes[21] << 8) |
                           ((uint32_t)bytes[22] << 16) | ((uint32_t)bytes[23] << 24));

        if (id != CSF_ID)
        {
                return false;
        }

        *language = (version >= 2) ? langid : 0 /* LANGUAGE_ID_US */;
        return true;
}

// --- the golden script --------------------------------------------------------

static FILE *g_out;

static void dump_hex(const void *data, size_t len)
{
        const uint8_t *p = (const uint8_t *)data;
        if (!len)
        {
                fprintf(g_out, "-");
                return;
        }
        for (size_t i = 0; i < len; ++i)
        {
                fprintf(g_out, "%02x", p[i]);
        }
}

static void dump_wide_hex(const std::vector<WideChar> &units)
{
        if (units.empty())
        {
                fprintf(g_out, "-");
                return;
        }
        for (WideChar u : units)
        {
                fprintf(g_out, "%02x%02x", (unsigned)(u & 0xff), (unsigned)((u >> 8) & 0xff));
        }
}

static void emit_file(const char *id, const Bytes &bytes)
{
        fprintf(g_out, "file %s ", id);
        dump_hex(bytes.data(), bytes.size());
        fprintf(g_out, "\n");
}

static void emit_gen(const char *id, const Bytes &bytes)
{
        fprintf(g_out, "gen %s ", id);
        dump_hex(bytes.data(), bytes.size());
        fprintf(g_out, "\n");
}

// Runs the transcribed reader on `bytes` and emits the verdict + views.
static void emit_reader(const char *id, const Bytes &bytes)
{
        Cursor cursor{bytes.data(), bytes.size(), 0};
        std::vector<StringInfo> labels;
        bool ok = parseCSF(cursor, labels);

        int language = 0;
        bool has_info = getCSFInfo(bytes, &language);
        fprintf(g_out, "info %s ", id);
        if (has_info)
        {
                fprintf(g_out, "%d", language);
        }
        else
        {
                fprintf(g_out, "-");
        }
        fprintf(g_out, "\n");

        if (!ok)
        {
                fprintf(g_out, "err %s\n", id);
                return;
        }

        fprintf(g_out, "ok %s\n", id);
        fprintf(g_out, "count %s %zu\n", id, labels.size());
        for (size_t i = 0; i < labels.size(); ++i)
        {
                fprintf(g_out, "view %s %zu ", id, i);
                dump_hex(labels[i].label.data(), labels[i].label.size());
                fprintf(g_out, " ");
                dump_wide_hex(labels[i].text);
                fprintf(g_out, " ");
                dump_hex(labels[i].speech.data(), labels[i].speech.size());
                fprintf(g_out, "\n");
        }
}

// A writer corpus: emit the transcribed writer's bytes, then the reader.
static void emit_corpus(const char *id, const std::vector<BabylonLabel> &db, int langid)
{
        Bytes bytes;
        if (!generate_csf(db, langid, bytes))
        {
                fprintf(stderr, "error: writer refused corpus %s\n", id);
                exit(1);
        }

        emit_file(id, bytes);
        emit_gen(id, bytes);
        emit_reader(id, bytes);
}

// Hand-assembled blobs for reader paths the Babylon writer cannot produce
// (assembled to the documented chunk layout, reader-direction only).
static void emit_raw_corpus(const char *id, const Bytes &bytes)
{
        emit_file(id, bytes);
        emit_reader(id, bytes);
}

static void patch_int(Bytes &bytes, size_t offset, int value)
{
        bytes[offset] = (uint8_t)(value & 0xff);
        bytes[offset + 1] = (uint8_t)((value >> 8) & 0xff);
        bytes[offset + 2] = (uint8_t)((value >> 16) & 0xff);
        bytes[offset + 3] = (uint8_t)((value >> 24) & 0xff);
}

static std::vector<WideChar> widen(const char *ascii)
{
        std::vector<WideChar> out;
        for (const char *p = ascii; *p; ++p)
        {
                out.push_back((WideChar)(uint8_t)*p);
        }
        return out;
}

int main(int argc, char **argv)
{
        if (argc != 2)
        {
                fprintf(stderr, "usage: gen_csf_golden <output-file>\n");
                return 1;
        }

        g_out = fopen(argv[1], "w");
        if (!g_out)
        {
                perror("fopen");
                return 1;
        }

        fprintf(g_out, "# CSF golden vectors (parity level P0).\n");
        fprintf(g_out, "# Generated by tools/differential/gen_csf_golden.cpp: the Babylon\n");
        fprintf(g_out, "# writer transcription (expimp.cpp, minus its tool-only\n");
        fprintf(g_out, "# StripSpaces/ConvertMetaChars transforms) pins the byte direction;\n");
        fprintf(g_out, "# the runtime reader transcription (GameText.cpp parseCSF +\n");
        fprintf(g_out, "# stripSpaces) pins the game-visible view. Hex is lowercase, '-'\n");
        fprintf(g_out, "# is empty. Regenerate with tools/differential/regenerate.sh.\n");

        // --- writer-direction corpora ---------------------------------------

        BabylonLabel basic0;
        basic0.name = "GUI:Hello";
        basic0.texts.push_back({widen("Hello World"), ""});

        BabylonLabel basic1;
        basic1.name = "GUI:Spaces";
        basic1.texts.push_back({widen("  padded   text  "), ""});

        BabylonLabel basic2;
        basic2.name = "GUI:Newline";
        basic2.texts.push_back({widen("line one\n  line two\t and tab"), ""});

        emit_corpus("basic", {basic0, basic1, basic2}, 0);

        BabylonLabel waves0;
        waves0.name = "SND:Talk";
        waves0.texts.push_back({widen("Hello there"), "SPEECH:hello.wav"});

        BabylonLabel waves1;
        waves1.name = "SND:Quiet";
        waves1.texts.push_back({widen("No wave here"), ""});

        emit_corpus("waves", {waves0, waves1}, 0);

        BabylonLabel multi0;
        multi0.name = "MULTI:Two";
        multi0.texts.push_back({widen("first string"), ""});
        multi0.texts.push_back({widen("second string"), ""});

        emit_corpus("multi", {multi0}, 0);

        BabylonLabel empty0;
        empty0.name = "EMPTY:Text";
        empty0.texts.push_back({{}, ""}); // L""

        emit_corpus("empty_text", {empty0}, 0);

        BabylonLabel wide0;
        wide0.name = "WIDE:CJK";
        // U+65E5 U+672C U+8A9E plus a surrogate pair (U+1F600).
        wide0.texts.push_back({{0x65E5, 0x672C, 0x8A9E, 0xD83D, 0xDE00}, ""});

        emit_corpus("wide", {wide0}, 0);

        BabylonLabel ffff0;
        ffff0.name = "QUIRK:FFFF";
        // The 0xFFFF unit's stored form is 0x0000: the reader's
        // invert-until-zero loop stops there and the visible text ends.
        ffff0.texts.push_back({{(WideChar)'a', 0xFFFF, (WideChar)'b'}, ""});

        emit_corpus("ffff", {ffff0}, 0);

        // --- reader-direction blobs -----------------------------------------

        // A label with zero string chunks (writeCSFLabel can emit it, but
        // only for a named label; assembled directly to also pin the
        // empty-name view).
        {
                Bytes bytes;
                put_int(bytes, CSF_ID);
                put_int(bytes, CSF_VERSION);
                put_int(bytes, 2); // num_labels
                put_int(bytes, 1); // num_strings
                put_int(bytes, 0); // skip
                put_int(bytes, 0); // langid

                put_int(bytes, CSF_LABEL);
                put_int(bytes, 0); // num_strings
                put_int(bytes, 9); // name length
                put_bytes(bytes, "NAMED:Empty", 9);

                put_int(bytes, CSF_LABEL);
                put_int(bytes, 0); // num_strings
                put_int(bytes, 0); // name length
                emit_raw_corpus("empty_label", bytes);
        }

        // A decoded 0x0000 unit (stored 0xFFFF): the inversion writes a
        // zero mid-buffer and UnicodeString stops there.
        {
                Bytes bytes;
                put_int(bytes, CSF_ID);
                put_int(bytes, CSF_VERSION);
                put_int(bytes, 1);
                put_int(bytes, 1);
                put_int(bytes, 0);
                put_int(bytes, 0);

                put_int(bytes, CSF_LABEL);
                put_int(bytes, 1); // num_strings
                put_int(bytes, 10);
                put_bytes(bytes, "QUIRK:Zero", 10);

                put_int(bytes, CSF_STRING);
                put_int(bytes, 3); // text units
                // stored units: ~'a', 0xFFFF, ~'b'  → decoded 'a', 0x0000, 'b'
                put_u16(bytes, (uint16_t)~(uint16_t)'a');
                put_u16(bytes, (uint16_t)0xFFFF);
                put_u16(bytes, (uint16_t)~(uint16_t)'b');
                emit_raw_corpus("zero", bytes);
        }

        // The other wave shape: NUL instead of the language letter (what
        // non-Babylon writers emit).
        {
                Bytes bytes;
                put_int(bytes, CSF_ID);
                put_int(bytes, CSF_VERSION);
                put_int(bytes, 1);
                put_int(bytes, 1);
                put_int(bytes, 0);
                put_int(bytes, 0);

                put_int(bytes, CSF_LABEL);
                put_int(bytes, 1);
                put_int(bytes, 8);
                put_bytes(bytes, "WAVE:Nul", 8);

                put_int(bytes, CSF_STRINGWITHWAVE);
                put_int(bytes, 5); // text units
                for (int i = 0; i < 5; ++i)
                {
                        put_u16(bytes, (uint16_t)~(uint16_t)("wave!"[i]));
                }
                put_int(bytes, 16); // wave_len = strlen + 1
                put_bytes(bytes, "SPEECH:wave.wav", 15);
                static const uint8_t nul = 0;
                put_bytes(bytes, &nul, 1); // the NUL terminator
                emit_raw_corpus("wave_nul", bytes);
        }

        // Trailing bytes shorter than a chunk id: the reader's loop simply
        // ends and the file is accepted.
        {
                std::vector<BabylonLabel> db;
                BabylonLabel label;
                label.name = "TRAIL:Short";
                label.texts.push_back({widen("ok"), ""});
                db.push_back(label);

                Bytes bytes;
                if (!generate_csf(db, 0, bytes))
                {
                        return 1;
                }
                bytes.push_back(0x42);
                bytes.push_back(0x00);
                emit_raw_corpus("trailing_short", bytes);
        }

        // Four-plus trailing bytes must start a label chunk: the reader
        // bails out (goto quit) and rejects the whole file.
        {
                std::vector<BabylonLabel> db;
                BabylonLabel label;
                label.name = "TRAIL:Junk";
                label.texts.push_back({widen("ok"), ""});
                db.push_back(label);

                Bytes bytes;
                if (!generate_csf(db, 0, bytes))
                {
                        return 1;
                }
                const char *junk = "JUNKJUNK";
                bytes.insert(bytes.end(), junk, junk + 8);
                emit_raw_corpus("trailing_junk", bytes);
        }

        // Version 1: getCSFInfo falls back to US English. Patched copy of
        // `basic` (the writer always emits version 3).
        {
                std::vector<BabylonLabel> db;
                BabylonLabel label;
                label.name = "GUI:Hello";
                label.texts.push_back({widen("Hello World"), ""});
                db.push_back(label);

                Bytes bytes;
                if (!generate_csf(db, 7 /* a non-US langid */, bytes))
                {
                        return 1;
                }
                patch_int(bytes, 4, 1); // version = 1
                emit_raw_corpus("v1", bytes);
        }

        // Version 2: getCSFInfo takes the file's langid. Same patch shape.
        {
                std::vector<BabylonLabel> db;
                BabylonLabel label;
                label.name = "GUI:Hello";
                label.texts.push_back({widen("Hello World"), ""});
                db.push_back(label);

                Bytes bytes;
                if (!generate_csf(db, 4 /* LANGUAGE_ID_SPANISH */, bytes))
                {
                        return 1;
                }
                patch_int(bytes, 4, 2); // version = 2
                emit_raw_corpus("v2", bytes);
        }

        fclose(g_out);
        g_out = nullptr;
        return 0;
}
