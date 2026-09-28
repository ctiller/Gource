//! UTF-8 sanitising of untrusted log input (port of `RCommitLog::filter_utf8`).
//!
//! Replicates `utf8::replace_invalid(start, end, out, '?')` from utfcpp in `src/core/utf8`.

#[derive(Debug, PartialEq, Eq)]
enum UtfError {
    Ok(u32, usize),
    NotEnoughRoom,
    InvalidLead,
    IncompleteSequence,
    OverlongSequence,
    InvalidCodePoint,
}

fn sequence_length(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if (lead >> 5) == 0x06 {
        2
    } else if (lead >> 4) == 0x0e {
        3
    } else if (lead >> 3) == 0x1e {
        4
    } else {
        0
    }
}

fn is_trail(b: u8) -> bool {
    (b >> 6) == 0x02
}

fn is_surrogate(cp: u32) -> bool {
    (0xD800..=0xDFFF).contains(&cp)
}

fn is_code_point_valid(cp: u32) -> bool {
    cp <= 0x10FFFF && !is_surrogate(cp)
}

fn is_overlong_sequence(cp: u32, length: usize) -> bool {
    if cp < 0x80 {
        length != 1
    } else if cp < 0x800 {
        length != 2
    } else if cp < 0x10000 {
        length != 3
    } else {
        false
    }
}

fn validate_next(bytes: &[u8]) -> UtfError {
    if bytes.is_empty() {
        return UtfError::NotEnoughRoom;
    }
    let lead = bytes[0];
    let length = sequence_length(lead);
    if length == 0 {
        return UtfError::InvalidLead;
    }
    if length == 1 {
        return UtfError::Ok(lead as u32, 1);
    }

    // Read trail bytes
    let mut cp = lead as u32;
    match length {
        2 => {
            if bytes.len() < 2 {
                return UtfError::NotEnoughRoom;
            }
            if !is_trail(bytes[1]) {
                return UtfError::IncompleteSequence;
            }
            cp = ((cp << 6) & 0x7ff) + (bytes[1] as u32 & 0x3f);
        }
        3 => {
            if bytes.len() < 2 {
                return UtfError::NotEnoughRoom;
            }
            if !is_trail(bytes[1]) {
                return UtfError::IncompleteSequence;
            }
            if bytes.len() < 3 {
                return UtfError::NotEnoughRoom;
            }
            if !is_trail(bytes[2]) {
                return UtfError::IncompleteSequence;
            }
            cp = ((cp << 12) & 0xffff)
                + (((bytes[1] as u32) << 6) & 0xfff)
                + (bytes[2] as u32 & 0x3f);
        }
        4 => {
            if bytes.len() < 2 {
                return UtfError::NotEnoughRoom;
            }
            if !is_trail(bytes[1]) {
                return UtfError::IncompleteSequence;
            }
            if bytes.len() < 3 {
                return UtfError::NotEnoughRoom;
            }
            if !is_trail(bytes[2]) {
                return UtfError::IncompleteSequence;
            }
            if bytes.len() < 4 {
                return UtfError::NotEnoughRoom;
            }
            if !is_trail(bytes[3]) {
                return UtfError::IncompleteSequence;
            }
            cp = ((cp << 18) & 0x1fffff)
                + (((bytes[1] as u32) << 12) & 0x3ffff)
                + (((bytes[2] as u32) << 6) & 0xfff)
                + (bytes[3] as u32 & 0x3f);
        }
        _ => unreachable!(),
    }

    if is_code_point_valid(cp) {
        if is_overlong_sequence(cp, length) {
            UtfError::OverlongSequence
        } else {
            UtfError::Ok(cp, length)
        }
    } else {
        UtfError::InvalidCodePoint
    }
}

/// Convert raw bytes to a `String`, replacing invalid UTF-8 exactly like
/// utfcpp's `utf8::replace_invalid(.., '?')` does in the C++ version:
/// each invalid sequence becomes a single `'?'`, and any continuation bytes
/// (`0b10xx_xxxx`) directly following an invalid byte are skipped.
pub fn filter_utf8(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }

    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match validate_next(&bytes[i..]) {
            UtfError::Ok(_cp, len) => {
                if let Ok(s) = std::str::from_utf8(&bytes[i..i + len]) {
                    out.push_str(s);
                } else {
                    out.push('?');
                }
                i += len;
            }
            UtfError::NotEnoughRoom => {
                out.push('?');
                break;
            }
            UtfError::InvalidLead => {
                out.push('?');
                i += 1;
            }
            UtfError::IncompleteSequence
            | UtfError::OverlongSequence
            | UtfError::InvalidCodePoint => {
                out.push('?');
                i += 1;
                while i < bytes.len() && is_trail(bytes[i]) {
                    i += 1;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_passthrough() {
        assert_eq!(filter_utf8("héllo wörld".as_bytes()), "héllo wörld");
    }

    #[test]
    fn invalid_lead_skips_trailing_continuations() {
        assert_eq!(filter_utf8(&[0x80, 0x80, b'a']), "??a");
        assert_eq!(filter_utf8(&[b'x', 0xFF, b'y']), "x?y");
    }

    #[test]
    fn truncated_sequence() {
        assert_eq!(filter_utf8(&[0xE2, 0x82, b'a']), "?a");
        assert_eq!(filter_utf8(&[b'a', 0xE2]), "a?");
    }

    #[test]
    fn overlong_and_surrogates_rejected() {
        assert_eq!(filter_utf8(&[0xC0, 0xAF]), "?");
        assert_eq!(filter_utf8(&[0xED, 0xA0, 0x80]), "?");
    }

    #[test]
    fn four_byte_sequences_and_edge_cases() {
        // Valid 4-byte character mixed with invalid byte (so standard std::str::from_utf8 fails)
        // 🚀 (U+1F680): 0xF0, 0x9F, 0x9A, 0x80
        assert_eq!(filter_utf8(&[0xF0, 0x9F, 0x9A, 0x80, 0xFF]), "🚀?");
        // Truncated 4-byte: 1 byte
        assert_eq!(filter_utf8(&[0xF0]), "?");
        // Truncated 4-byte: 2 bytes
        assert_eq!(filter_utf8(&[0xF0, 0x9F]), "?");
        // Truncated 4-byte: 3 bytes
        assert_eq!(filter_utf8(&[0xF0, 0x9F, 0x9A]), "?");
        // 4-byte with invalid trail at position 1
        assert_eq!(filter_utf8(&[0xF0, b'x']), "?x");
        // 4-byte with invalid trail at position 2
        assert_eq!(filter_utf8(&[0xF0, 0x9F, b'x']), "?x");
        // 4-byte with invalid trail at position 3
        assert_eq!(filter_utf8(&[0xF0, 0x9F, 0x9A, b'x']), "?x");
        // Overlong 4-byte: code point < 0x10000 (e.g. 0xF0 0x8F 0xBF 0xBF -> U+FFFF)
        assert_eq!(filter_utf8(&[0xF0, 0x8F, 0xBF, 0xBF]), "?");
        // Truncated 3-byte: 1 byte
        assert_eq!(filter_utf8(&[0xE2]), "?");
        // Truncated 3-byte: 2 bytes
        assert_eq!(filter_utf8(&[0xE2, 0x82]), "?");
        // 3-byte with invalid trail at position 1
        assert_eq!(filter_utf8(&[0xE2, b'x']), "?x");
        // 3-byte with invalid trail at position 2
        assert_eq!(filter_utf8(&[0xE2, 0x82, b'x']), "?x");
        // Overlong 3-byte: code point < 0x800 (e.g. 0xE0 0x9F 0x80 -> U+07C0)
        assert_eq!(filter_utf8(&[0xE0, 0x9F, 0x80]), "?");
        // Overlong 2-byte: code point < 0x80 (e.g. 0xC1 0x81 -> U+0041 'A')
        assert_eq!(filter_utf8(&[0xC1, 0x81]), "?");
        // Code point > 0x10FFFF (e.g. 0xF4 0x90 0x80 0x80 -> U+110000)
        assert_eq!(filter_utf8(&[0xF4, 0x90, 0x80, 0x80]), "?");
    }

    #[test]
    fn matches_cpp_utf8_goldens() {
        let golden_data = include_str!("../tests/data/utf8_golden.txt");
        for line in golden_data.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            let inp_hex = parts[0];
            let out_hex = if parts.len() > 1 { parts[1] } else { "" };

            let inp_bytes: Vec<u8> = (0..inp_hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&inp_hex[i..i + 2], 16).unwrap())
                .collect();
            let expected_bytes: Vec<u8> = (0..out_hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&out_hex[i..i + 2], 16).unwrap())
                .collect();
            let expected_str = std::str::from_utf8(&expected_bytes).unwrap();

            assert_eq!(
                filter_utf8(&inp_bytes),
                expected_str,
                "Failed on input hex: {}",
                inp_hex
            );
        }
    }
}
