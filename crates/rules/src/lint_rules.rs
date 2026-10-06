//! The text encoding every tracked and rendered file keeps, and the shrink-only baselines of the lint stages.

/// What keeps a text file from UTF-8 without a byte order mark that ends with a newline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fault {
    ByteOrderMark,
    NotUtf8,
    NoFinalNewline,
}

impl Fault {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::ByteOrderMark => "byte-order-mark",
            Self::NotUtf8 => "not-utf-8",
            Self::NoFinalNewline => "no-final-newline",
        }
    }
}

/// Whether `bytes` hold well-formed UTF-8: shortest forms only, no surrogates, and nothing past U+10FFFF.
#[must_use]
pub fn is_utf8(bytes: &[u8]) -> bool {
    let mut index = 0;
    while let Some(&lead) = bytes.get(index) {
        let (length, least, mut point) = if lead < 0x80 {
            (1, 0, u32::from(lead))
        } else if lead & 0xE0 == 0xC0 {
            (2, 0x80, u32::from(lead & 0x1F))
        } else if lead & 0xF0 == 0xE0 {
            (3, 0x800, u32::from(lead & 0x0F))
        } else if lead & 0xF8 == 0xF0 {
            (4, 0x1_0000, u32::from(lead & 0x07))
        } else {
            return false;
        };
        if bytes.len() - index < length {
            return false;
        }
        let mut offset = 1;
        while offset < length {
            let Some(&next) = bytes.get(index + offset) else {
                return false;
            };
            if next & 0xC0 != 0x80 {
                return false;
            }
            point = (point << 6) | u32::from(next & 0x3F);
            offset += 1;
        }
        if point < least || point > 0x10_FFFF || (0xD800..=0xDFFF).contains(&point) {
            return false;
        }
        index += length;
    }
    true
}

/// The first fault of a text file's bytes, or `None` for an empty file or one that keeps the encoding.
#[must_use]
pub fn fault(bytes: &[u8]) -> Option<Fault> {
    if matches!(bytes, [0xEF, 0xBB, 0xBF, ..]) {
        Some(Fault::ByteOrderMark)
    } else if !is_utf8(bytes) {
        Some(Fault::NotUtf8)
    } else if bytes.last().is_some_and(|&last| last != b'\n') {
        Some(Fault::NoFinalNewline)
    } else {
        None
    }
}

/// How the findings of one rule in one path compare with the count the baseline admits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Standing {
    /// The path holds exactly what the baseline admits.
    Within,
    /// The path holds fewer findings, so the baseline must shrink to keep them from coming back.
    Shrunk,
    /// The path holds a finding the baseline doesn't admit.
    Exceeded,
}

#[must_use]
pub const fn standing(admitted: u32, found: u32) -> Standing {
    if found > admitted {
        Standing::Exceeded
    } else if found < admitted {
        Standing::Shrunk
    } else {
        Standing::Within
    }
}

/// Whether a baseline edit only removes or lowers what it admits, from `before` at the reference to `after` in the change.
#[must_use]
pub const fn shrinks(before: u32, after: u32) -> bool {
    after <= before
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(6)]
fn the_validator_accepts_every_scalar_value_and_refuses_impossible_leads() {
    let point: u32 = kani::any();
    kani::assume(point <= 0x10_FFFF && !(0xD800..=0xDFFF).contains(&point));
    let mut encoded = [0u8; 4];
    let length = if point < 0x80 {
        encoded[0] = point as u8;
        1
    } else if point < 0x800 {
        encoded[0] = 0xC0 | (point >> 6) as u8;
        encoded[1] = 0x80 | (point & 0x3F) as u8;
        2
    } else if point < 0x1_0000 {
        encoded[0] = 0xE0 | (point >> 12) as u8;
        encoded[1] = 0x80 | ((point >> 6) & 0x3F) as u8;
        encoded[2] = 0x80 | (point & 0x3F) as u8;
        3
    } else {
        encoded[0] = 0xF0 | (point >> 18) as u8;
        encoded[1] = 0x80 | ((point >> 12) & 0x3F) as u8;
        encoded[2] = 0x80 | ((point >> 6) & 0x3F) as u8;
        encoded[3] = 0x80 | (point & 0x3F) as u8;
        4
    };
    assert!(is_utf8(&encoded[..length]));
    let other: [u8; 2] = kani::any();
    let impossible =
        other[0] == 0xC0 || other[0] == 0xC1 || other[0] >= 0xF5 || other[0] & 0xC0 == 0x80;
    if impossible {
        assert!(!is_utf8(&other));
    }
    kani::cover!(length == 4);
    kani::cover!(impossible && other[0] == 0xC0);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(6)]
fn a_file_passes_only_as_utf8_without_a_mark_that_ends_with_a_newline() {
    let bytes: [u8; 4] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= 4);
    let text = &bytes[..length];
    let passes = fault(text).is_none();
    if passes {
        assert!(is_utf8(text));
        assert!(length == 0 || text[length - 1] == b'\n');
        assert!(!(length >= 3 && text[0] == 0xEF && text[1] == 0xBB && text[2] == 0xBF));
    } else {
        assert!(length > 0);
    }
    kani::cover!(passes && length == 4);
    kani::cover!(fault(text) == Some(Fault::NoFinalNewline));
}

#[cfg(kani)]
#[kani::proof]
fn a_baseline_admits_no_new_finding_and_keeps_shrinking() {
    let before: u32 = kani::any();
    let after: u32 = kani::any();
    let found: u32 = kani::any();
    let accepted = shrinks(before, after) && standing(after, found) == Standing::Within;
    if accepted {
        assert!(found <= before);
    }
    assert!((standing(after, found) == Standing::Exceeded) == (found > after));
    kani::cover!(accepted && found < before);
    kani::cover!(!accepted && found > before);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encoding_refuses_a_mark_broken_utf8_and_a_missing_final_newline() {
        assert_eq!(fault(b""), None);
        assert_eq!(fault("na\u{ef}ve\n".as_bytes()), None);
        assert_eq!(fault(b"\xEF\xBB\xBFtext\n"), Some(Fault::ByteOrderMark));
        assert_eq!(fault(b"na\xEFve\n"), Some(Fault::NotUtf8));
        assert_eq!(fault(b"text"), Some(Fault::NoFinalNewline));
    }

    #[test]
    fn the_validator_refuses_overlong_forms_surrogates_and_points_past_the_range() {
        assert!(is_utf8("\u{10FFFF}".as_bytes()));
        assert!(!is_utf8(b"\xC0\xAF"));
        assert!(!is_utf8(b"\xED\xA0\x80"));
        assert!(!is_utf8(b"\xF4\x90\x80\x80"));
        assert!(!is_utf8(b"\xE2\x82"));
    }

    #[test]
    fn a_baseline_only_shrinks_and_refuses_both_new_and_vanished_findings() {
        assert_eq!(standing(2, 2), Standing::Within);
        assert_eq!(standing(2, 1), Standing::Shrunk);
        assert_eq!(standing(0, 1), Standing::Exceeded);
        assert!(shrinks(3, 2));
        assert!(!shrinks(0, 1));
    }
}
