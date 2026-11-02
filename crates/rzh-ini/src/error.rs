// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
//! Error types for the INI engine.
//!
//! The C++ engine throws one of a dozen `INI_*` constants (INI.h), all of
//! which map to the same underlying `ERROR_BAD_INI` value — the names exist
//! only to document the failure site. This port keeps the names as a
//! [`ErrorKind`] enum, plus the file/line context the retail exception
//! message carried, so callers can report *where* an INI failed.
//!
//! Ported from `Core/GameEngine/Include/Common/INI.h` and the `throw`
//! sites in `Core/GameEngine/Source/Common/INI/INI.cpp` @ 571a7c5.

use core::fmt;

/// Which check in the INI engine failed.
///
/// The names mirror the `INI_*` constants the C++ engine throws. The kinds
/// that never fire in this crate's ported subset (`CantOpenFile` aside,
/// which the loader raises for the byte source) are still part of the
/// vocabulary because the C++ error surface is one flat space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// `INI_UNKNOWN_TOKEN`: the first token on a line is not a known block
    /// type (load loop) or a boolean/flag name could not be resolved.
    UnknownToken,
    /// `INI_MISSING_END_TOKEN`: a data block hit end-of-file without an
    /// `End` line.
    MissingEndToken,
    /// `INI_INVALID_DATA`: a value token could not be scanned, a name was
    /// not found in an index/lookup list, or a validation rule failed.
    InvalidData,
    /// `INI_INVALID_NAME_LIST`: a bit-string list mixed normal entries with
    /// `+`/`-` entries, or the name list itself is empty.
    InvalidNameList,
    /// `INI_CANT_OPEN_FILE`: the byte source for a load was missing.
    CantOpenFile,
    /// `INI_INVALID_PARAMS`: `initFromINI` was given no target.
    ///
    /// Kept for completeness; the Rust type system makes it unreachable.
    InvalidParams,
}

impl ErrorKind {
    /// The C++ constant name for this kind, as documented in `INI.h`.
    #[must_use]
    pub const fn constant_name(self) -> &'static str {
        match self {
            Self::UnknownToken => "INI_UNKNOWN_TOKEN",
            Self::MissingEndToken => "INI_MISSING_END_TOKEN",
            Self::InvalidData => "INI_INVALID_DATA",
            Self::InvalidNameList => "INI_INVALID_NAME_LIST",
            Self::CantOpenFile => "INI_CANT_OPEN_FILE",
            Self::InvalidParams => "INI_INVALID_PARAMS",
        }
    }
}

/// An INI parse failure with file and line context.
///
/// The C++ engine wraps failures into an `INIException` whose message names
/// the file and the offending line; the structured equivalent keeps the
/// failure kind plus the current file and line number (the engine's
/// line counter, which counts every physical line including the synthetic
/// final line produced at EOF).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IniError {
    /// Which check failed.
    pub kind: ErrorKind,
    /// The engine's name for the file being loaded.
    pub file: String,
    /// The engine's line counter at the time of the failure.
    pub line: u32,
    /// The field token whose value failed to parse, when the failure came
    /// from a field inside a data block (the C++ message includes it).
    pub field: Option<String>,
    /// The block declaration line, when the failure came from inside a
    /// data block (the C++ `INIException` message includes it).
    pub block: Option<String>,
}

impl IniError {
    /// Builds an error with the given context.
    #[must_use]
    pub fn new(kind: ErrorKind, file: &str, line: u32) -> Self {
        Self {
            kind,
            file: file.to_owned(),
            line,
            field: None,
            block: None,
        }
    }

    /// Attaches the failing field token (the C++ rethrow in
    /// `initFromINIMulti` names the field in its message).
    #[must_use]
    pub fn in_field(mut self, field: &[u8]) -> Self {
        self.field = Some(String::from_utf8_lossy(field).into_owned());
        self
    }

    /// Attaches the block declaration line (the C++ rethrow in `load`
    /// names the block's first line in its message).
    #[must_use]
    pub fn in_block(mut self, block: &[u8]) -> Self {
        self.block = Some(String::from_utf8_lossy(block).into_owned());
        self
    }
}

impl fmt::Display for IniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} in {} at line {}",
            self.kind.constant_name(),
            self.file,
            self.line
        )?;
        if let Some(field) = &self.field {
            write!(f, " (field '{field}')")?;
        }
        if let Some(block) = &self.block {
            write!(f, " (block '{block}')")?;
        }
        Ok(())
    }
}

impl std::error::Error for IniError {}
