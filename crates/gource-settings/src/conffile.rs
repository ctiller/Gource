//! INI-style configuration files (port of `core/conffile.{h,cpp}`).
//!
//! A conf file is an ordered list of sections (`[name]`); a section name may
//! repeat (multiple `[gource]` sections = multiple repositories). Entries are
//! `name=value` lines; comments start with `#`. Command line options are
//! converted into entries of the appropriate section.

use crate::SettingsError;
use gource_core::{Vec2, Vec3, Vec4};
use std::collections::BTreeMap;
use std::path::Path;

/// One `name=value` line.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfEntry {
    pub name: String,
    pub value: String,
    /// Line number in the source file (0 if created programmatically).
    pub line: usize,
}

impl ConfEntry {
    pub fn new(name: impl Into<String>, value: impl Into<String>, line: usize) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            line,
        }
    }

    pub fn has_value(&self) -> bool {
        !self.value.is_empty()
    }

    pub fn is_int(&self) -> bool {
        // C++: ConfFile_float_value.match(value) in isInt()
        is_float_str(&self.value)
    }

    pub fn get_int(&self) -> i32 {
        // C++: atoi(value.c_str())
        parse_c_int(&self.value)
    }

    pub fn is_float(&self) -> bool {
        is_float_str(&self.value)
    }

    pub fn get_float(&self) -> f32 {
        // C++: atof(value.c_str())
        parse_c_float(&self.value)
    }

    pub fn is_bool(&self) -> bool {
        matches!(
            self.value.as_str(),
            "1" | "true"
                | "True"
                | "TRUE"
                | "yes"
                | "Yes"
                | "YES"
                | "0"
                | "false"
                | "False"
                | "FALSE"
                | "no"
                | "No"
                | "NO"
        )
    }

    pub fn get_bool(&self) -> bool {
        matches!(
            self.value.as_str(),
            "1" | "true" | "True" | "TRUE" | "yes" | "Yes" | "YES"
        )
    }

    pub fn is_vec2(&self) -> bool {
        parse_vec2_str(&self.value).is_some()
    }

    pub fn get_vec2(&self) -> Vec2 {
        parse_vec2_str(&self.value).unwrap_or(Vec2::ZERO)
    }

    pub fn is_vec3(&self) -> bool {
        parse_vec3_str(&self.value).is_some()
    }

    pub fn get_vec3(&self) -> Vec3 {
        parse_vec3_str(&self.value).unwrap_or(Vec3::ZERO)
    }

    pub fn is_vec4(&self) -> bool {
        parse_vec4_str(&self.value).is_some()
    }

    pub fn get_vec4(&self) -> Vec4 {
        parse_vec4_str(&self.value).unwrap_or(Vec4::ZERO)
    }
}

/// A `[section]` and its entries in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfSection {
    pub name: String,
    pub entries: Vec<ConfEntry>,
    /// Line number of the section header (0 if created programmatically).
    pub line: usize,
}

impl ConfSection {
    /// First entry with this name.
    pub fn entry(&self, name: &str) -> Option<&ConfEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// All entries with this name, in order (e.g. repeated `file-filter`).
    pub fn entries_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a ConfEntry> + 'a {
        self.entries.iter().filter(move |e| e.name == name)
    }

    /// Replace all entries with this name, or append one (matching C++ ConfSection::setEntry).
    pub fn set_entry(&mut self, name: &str, value: &str) {
        self.entries.retain(|e| e.name != name);
        self.add_entry(name, value);
    }

    /// Append an entry (allows duplicates).
    pub fn add_entry(&mut self, name: &str, value: &str) {
        self.entries.push(ConfEntry {
            name: name.to_owned(),
            value: value.to_owned(),
            line: 0,
        });
    }

    pub fn has_value(&self, key: &str) -> bool {
        self.entry(key)
            .map(|e| !e.value.is_empty())
            .unwrap_or(false)
    }

    pub fn get_string(&self, key: &str) -> String {
        self.entry(key).map(|e| e.value.clone()).unwrap_or_default()
    }

    pub fn get_int(&self, key: &str) -> i32 {
        self.entry(key).map(|e| e.get_int()).unwrap_or(0)
    }

    pub fn get_float(&self, key: &str) -> f32 {
        self.entry(key).map(|e| e.get_float()).unwrap_or(0.0)
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.entry(key).map(|e| e.get_bool()).unwrap_or(false)
    }

    pub fn get_vec3(&self, key: &str) -> Vec3 {
        self.entry(key).map(|e| e.get_vec3()).unwrap_or(Vec3::ZERO)
    }

    pub fn get_vec4(&self, key: &str) -> Vec4 {
        self.entry(key).map(|e| e.get_vec4()).unwrap_or(Vec4::ZERO)
    }
}

/// Port of `ConfFile`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfFile {
    /// Source file name (for error messages), empty if none.
    pub filename: String,
    pub sections: Vec<ConfSection>,
}

impl ConfFile {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a conf file from disk (`ConfFile::load`).
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let filename = path.to_string_lossy().to_string();
        let content = std::fs::read_to_string(path)
            .map_err(|_| SettingsError(format!("failed to open config file {filename}")))?;
        Self::parse(&content, &filename)
    }

    /// Parse conf text; `filename` is used in error messages.
    pub fn parse(text: &str, filename: &str) -> Result<Self, SettingsError> {
        let mut conf = ConfFile {
            filename: filename.to_owned(),
            sections: Vec::new(),
        };

        let mut current_sec: Option<ConfSection> = None;
        let mut lineno = 0;

        // C++ std::getline reads until \n; last line without \n is yielded, empty trailing token after \n is not.
        let lines: Vec<&str> = if text.is_empty() {
            Vec::new()
        } else {
            let mut l: Vec<&str> = text.split('\n').collect();
            if text.ends_with('\n') {
                l.pop();
            }
            l
        };

        for raw_line in lines {
            lineno += 1;

            if raw_line.is_empty() || raw_line.starts_with('#') {
                continue;
            }

            if let Some(sec_name) = parse_section_header(raw_line) {
                if let Some(sec) = current_sec.take() {
                    conf.sections.push(sec);
                }
                current_sec = Some(ConfSection {
                    name: sec_name.to_owned(),
                    entries: Vec::new(),
                    line: lineno,
                });
            } else if let Some((key, val)) = parse_key_value(raw_line) {
                let sec = current_sec.get_or_insert_with(|| ConfSection {
                    name: String::new(),
                    entries: Vec::new(),
                    line: lineno,
                });
                sec.entries.push(ConfEntry {
                    name: key.to_owned(),
                    value: val.to_owned(),
                    line: lineno,
                });
            } else {
                let prefix = if filename.is_empty() {
                    format!("line {lineno}: could not parse line")
                } else {
                    format!("{filename}, line {lineno}: could not parse line")
                };
                return Err(SettingsError(prefix));
            }
        }

        if let Some(sec) = current_sec.take() {
            conf.sections.push(sec);
        }

        Ok(conf)
    }

    /// Serialise in the C++ `ConfFile::save` format.
    /// In C++: sections are grouped and sorted by section name; inside each section name group,
    /// sections preserve insertion order. Inside each section, entries are grouped and sorted
    /// by key name, preserving insertion order among duplicate keys.
    pub fn to_text(&self) -> String {
        let mut section_map: BTreeMap<&str, Vec<&ConfSection>> = BTreeMap::new();
        for sec in &self.sections {
            section_map.entry(&sec.name).or_default().push(sec);
        }

        let mut out = String::new();
        for (_sec_name, sec_list) in section_map {
            for sec in sec_list {
                out.push('[');
                out.push_str(&sec.name);
                out.push_str("]\n");

                let mut entry_map: BTreeMap<&str, Vec<&ConfEntry>> = BTreeMap::new();
                for entry in &sec.entries {
                    entry_map.entry(&entry.name).or_default().push(entry);
                }

                for (_entry_name, entry_list) in entry_map {
                    for entry in entry_list {
                        out.push_str(&entry.name);
                        out.push('=');
                        out.push_str(&entry.value);
                        out.push('\n');
                    }
                }
                out.push('\n');
            }
        }
        out
    }

    /// Write to disk (`ConfFile::save`).
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        std::fs::write(path, self.to_text()).map_err(|e| {
            SettingsError(format!(
                "failed to write config file {}: {e}",
                path.display()
            ))
        })
    }

    /// First section with this name.
    pub fn section(&self, name: &str) -> Option<&ConfSection> {
        self.sections.iter().find(|s| s.name == name)
    }

    pub fn section_mut(&mut self, name: &str) -> Option<&mut ConfSection> {
        self.sections.iter_mut().find(|s| s.name == name)
    }

    /// All sections with this name, in order.
    pub fn sections_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a ConfSection> + 'a {
        self.sections.iter().filter(move |s| s.name == name)
    }

    pub fn count_sections(&self, name: &str) -> usize {
        self.sections_named(name).count()
    }

    /// Append a new empty section and return it.
    pub fn add_section(&mut self, name: &str) -> &mut ConfSection {
        self.sections.push(ConfSection {
            name: name.to_owned(),
            entries: Vec::new(),
            line: 0,
        });
        self.sections.last_mut().expect("just pushed")
    }

    /// Set an entry in the first section with this name, creating the
    /// section if needed (`ConfFile::setEntry`).
    pub fn set_entry(&mut self, section: &str, name: &str, value: &str) {
        if self.section(section).is_none() {
            self.add_section(section);
        }
        self.section_mut(section)
            .expect("section exists")
            .set_entry(name, value);
    }

    /// Format an entry error with filename and line number if available.
    pub fn entry_error(
        &self,
        entry: Option<&ConfEntry>,
        reason: impl std::fmt::Display,
    ) -> SettingsError {
        let lineno = entry.map(|e| e.line).unwrap_or(0);
        SettingsError(format_error_prefix(&self.filename, lineno, reason))
    }

    /// Format a section error with filename and line number if available.
    pub fn section_error(
        &self,
        section: Option<&ConfSection>,
        reason: impl std::fmt::Display,
    ) -> SettingsError {
        let lineno = section.map(|s| s.line).unwrap_or(0);
        SettingsError(format_error_prefix(&self.filename, lineno, reason))
    }

    pub fn invalid_value_error(&self, entry: &ConfEntry) -> SettingsError {
        self.entry_error(Some(entry), format!("invalid '{}' value", entry.name))
    }

    pub fn missing_value_error(&self, entry: &ConfEntry) -> SettingsError {
        self.entry_error(
            Some(entry),
            format!("no value specified for '{}'", entry.name),
        )
    }

    pub fn unknown_option_error(&self, entry: &ConfEntry) -> SettingsError {
        self.entry_error(Some(entry), format!("unknown option '{}'", entry.name))
    }

    pub fn missing_entry_error(&self, section: &ConfSection, entry_name: &str) -> SettingsError {
        self.section_error(
            Some(section),
            format!(
                "section '{}' missing required entry '{entry_name}'",
                section.name
            ),
        )
    }
}

pub(crate) fn format_error_prefix(
    filename: &str,
    lineno: usize,
    reason: impl std::fmt::Display,
) -> String {
    if !filename.is_empty() {
        if lineno != 0 {
            format!("{filename}, line {lineno}: {reason}")
        } else {
            format!("{filename}: {reason}")
        }
    } else {
        reason.to_string()
    }
}

// Helpers for parsing conffile syntax

fn parse_section_header(line: &str) -> Option<&str> {
    // Regex: ^\s*\[([^\]]+)\]\s*$
    let trimmed = line.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if !inner.is_empty() && !inner.contains(']') {
            return Some(inner);
        }
    }
    None
}

fn parse_key_value(line: &str) -> Option<(&str, &str)> {
    // Regex: ^\s*([^=\s]+)\s*=\s*([^\s].*)?$
    let s = line.trim_start();
    let eq_idx = s.find('=')?;
    let key_part = &s[..eq_idx];
    if key_part.is_empty() || key_part.contains(|c: char| c.is_whitespace()) {
        return None;
    }
    let key = key_part.trim_end();

    let rest = &s[eq_idx + 1..];
    // value has whitespace trimmed at front (by regex `[^\s].*`) and at back (by ConfFile::trim)
    let val_trimmed =
        rest.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0b' | '\x0c'));
    Some((key, val_trimmed))
}

fn is_float_str(s: &str) -> bool {
    // Regex: ^\s*-?(\d*\.?\d+)\s*$
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    let rest = t.strip_prefix('-').unwrap_or(t);
    if rest.is_empty() {
        return false;
    }
    let mut has_digits = false;
    let mut has_dot = false;
    for c in rest.chars() {
        if c.is_ascii_digit() {
            has_digits = true;
        } else if c == '.' && !has_dot {
            has_dot = true;
        } else {
            return false;
        }
    }
    has_digits
}

pub(crate) fn parse_c_int(s: &str) -> i32 {
    let t = s.trim_start();
    let (neg, digits_part) = if let Some(stripped) = t.strip_prefix('-') {
        (true, stripped)
    } else if let Some(stripped) = t.strip_prefix('+') {
        (false, stripped)
    } else {
        (false, t)
    };
    let num_str: String = digits_part
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if num_str.is_empty() {
        return 0;
    }
    let val: i64 = num_str.parse().unwrap_or(0);
    if neg { (-val) as i32 } else { val as i32 }
}

pub(crate) fn parse_c_float(s: &str) -> f32 {
    let t = s.trim_start();
    // Emulate atof
    // Take leading valid float characters
    let mut end = 0;
    let mut chars = t.char_indices().peekable();
    if let Some(&(_, '+' | '-')) = chars.peek() {
        chars.next();
    }
    let mut has_dot = false;
    let mut has_digits = false;
    while let Some(&(i, c)) = chars.peek() {
        if c.is_ascii_digit() {
            has_digits = true;
            chars.next();
            end = i + 1;
        } else if c == '.' && !has_dot {
            has_dot = true;
            chars.next();
            end = i + 1;
        } else {
            break;
        }
    }
    // Handle optional 'e' or 'E'
    if has_digits && matches!(chars.peek(), Some(&(_, 'e' | 'E'))) {
        let mut exp_chars = chars.clone();
        exp_chars.next();
        if let Some(&(_, '+' | '-')) = exp_chars.peek() {
            exp_chars.next();
        }
        let mut exp_digits = false;
        let mut exp_end = end;
        for (i, c) in exp_chars {
            if c.is_ascii_digit() {
                exp_digits = true;
                exp_end = i + 1;
            } else {
                break;
            }
        }
        if exp_digits {
            end = exp_end;
        }
    }

    if end == 0 {
        return 0.0;
    }
    t[..end].parse::<f32>().unwrap_or(0.0)
}

fn parse_vec_elements(s: &str, prefix: &str, count: usize) -> Option<Vec<f32>> {
    let trimmed = s.trim();
    if !trimmed.starts_with(prefix) || !trimmed.ends_with(')') {
        return None;
    }
    let inner = &trimmed[prefix.len()..trimmed.len() - 1];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != count {
        return None;
    }
    let mut result = Vec::with_capacity(count);
    for part in parts {
        let p = part.trim();
        // check numeric: ^-?[0-9.]+$
        if p.is_empty() {
            return None;
        }
        let rest = p.strip_prefix('-').unwrap_or(p);
        if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return None;
        }
        result.push(parse_c_float(p));
    }
    Some(result)
}

pub(crate) fn parse_vec2_str(s: &str) -> Option<Vec2> {
    let vals = parse_vec_elements(s, "vec2(", 2)?;
    Some(Vec2::new(vals[0], vals[1]))
}

pub(crate) fn parse_vec3_str(s: &str) -> Option<Vec3> {
    let vals = parse_vec_elements(s, "vec3(", 3)?;
    Some(Vec3::new(vals[0], vals[1], vals[2]))
}

pub(crate) fn parse_vec4_str(s: &str) -> Option<Vec4> {
    let vals = parse_vec_elements(s, "vec4(", 4)?;
    Some(Vec4::new(vals[0], vals[1], vals[2], vals[3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_to_text_roundtrip() {
        let conf_text = "[gource]\npath=.\nstop-at-end=true\ntitle=My Title\n\n";
        let conf = ConfFile::parse(conf_text, "test.conf").unwrap();
        assert_eq!(conf.sections.len(), 1);
        assert_eq!(conf.sections[0].name, "gource");
        assert_eq!(conf.sections[0].get_string("path"), ".");
        assert!(conf.sections[0].get_bool("stop-at-end"));
        assert_eq!(conf.sections[0].get_string("title"), "My Title");
        assert_eq!(conf.to_text(), conf_text);
    }

    #[test]
    fn test_parse_error() {
        let err = ConfFile::parse("bad line\n", "file.conf").unwrap_err();
        assert_eq!(err.0, "file.conf, line 1: could not parse line");
    }

    #[test]
    fn test_empty_value() {
        let conf = ConfFile::parse("[gource]\nfoo=\n", "").unwrap();
        let entry = conf.section("gource").unwrap().entry("foo").unwrap();
        assert_eq!(entry.value, "");
        assert!(!entry.has_value());
    }

    #[test]
    fn test_helpers_and_errors() {
        let conf = ConfFile::parse(
            "[gource]\nint_val=42\nfloat_val=3.5\nvec_val=vec2(1.0, 2.0)\n",
            "x.conf",
        )
        .unwrap();
        let sec = conf.section("gource").unwrap();
        assert_eq!(sec.get_int("int_val"), 42);
        assert!((sec.get_float("float_val") - 3.5).abs() < 1e-4);
        assert_eq!(
            sec.entry("vec_val").unwrap().get_vec2(),
            Vec2::new(1.0, 2.0)
        );

        let entry = sec.entry("int_val").unwrap();
        assert_eq!(
            conf.invalid_value_error(entry).0,
            "x.conf, line 2: invalid 'int_val' value"
        );
        assert_eq!(
            conf.missing_value_error(entry).0,
            "x.conf, line 2: no value specified for 'int_val'"
        );
        assert_eq!(
            conf.unknown_option_error(entry).0,
            "x.conf, line 2: unknown option 'int_val'"
        );
    }
}
