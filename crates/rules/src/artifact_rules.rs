//! Whether an acquisition plan enables use of immutable materials.

/// Bound on fetch attempts for a missing artifact.
pub const FETCH_ATTEMPTS: u8 = 2;
pub const CURL_BYTE_LIMIT_VERSION: [u64; 3] = [8, 4, 0];
const SHA256_BYTES: usize = 32;
const SHA256_HEX_BYTES: usize = SHA256_BYTES * 2;
const HEX_DIGIT_BITS: u32 = 4;
const HEX_LETTER_VALUE: u8 = 10;
const DECIMAL_DIGIT_SPAN: u8 = b'9' - b'0';
const HEX_LETTER_SPAN: u8 = b'f' - b'a';
const ASCII_CASE_BIT: u8 = b'a' ^ b'A';
const DEVICE_NUMBER_SPAN: u8 = b'9' - b'1';
const SHORT_DEVICE_STEM_BYTES: usize = b"CON".len();
const NUMBERED_DEVICE_STEM_BYTES: usize = b"COM1".len();
const CONIN_DEVICE_STEM_BYTES: usize = b"CONIN$".len();
const CONOUT_DEVICE_STEM_BYTES: usize = b"CONOUT$".len();
const DEVICE_TRAILER_BYTES: usize = b"$".len();
const FALSE_BIT: u8 = 0;
const TRUE_BIT: u8 = 1;
#[cfg(any(test, kani))]
const HEX_DIGITS: &[u8] = b"0123456789abcdef";
const MAX_VERSION_BYTES: usize = 64;
const MIN_VERSION_PARTS: u8 = 2;
const MAX_VERSION_PARTS: u8 = 4;
const GIT_REVISION_BYTES: usize = 40;
const REVISION_PREFIX: &[u8] = b"rev:";
pub const COMPONENT_BYTE_LIMIT: usize = 255;
const WINDOWS_FILE_PATH_UNITS: usize = 260;
const DIRECTORY_APPEND_UNITS: usize = 12;
const PATH_NUL_UNITS: usize = 1;

#[must_use]
pub const fn cache_paths_fit(directory_utf16: usize, longest_file_utf16: usize) -> bool {
    let Some(directory) = directory_utf16.checked_add(DIRECTORY_APPEND_UNITS) else {
        return false;
    };
    let Some(file) = longest_file_utf16.checked_add(PATH_NUL_UNITS) else {
        return false;
    };
    directory <= WINDOWS_FILE_PATH_UNITS && file <= WINDOWS_FILE_PATH_UNITS
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpBinding {
    Matched,
    OptionsMismatch,
    SourceMismatch,
    ChecksumMismatch,
}

#[must_use]
pub const fn http_binding(
    options_match: bool,
    url_match: bool,
    checksum_match: bool,
) -> HttpBinding {
    if !options_match {
        HttpBinding::OptionsMismatch
    } else if !url_match {
        HttpBinding::SourceMismatch
    } else if !checksum_match {
        HttpBinding::ChecksumMismatch
    } else {
        HttpBinding::Matched
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetSelection {
    Required,
    Excluded,
    Refused,
}

#[must_use]
pub const fn target_selection(applicable: bool, explicit: bool) -> TargetSelection {
    if applicable {
        TargetSelection::Required
    } else if explicit {
        TargetSelection::Refused
    } else {
        TargetSelection::Excluded
    }
}

/// Admit portable ASCII archive components without Windows device aliases.
#[must_use]
pub fn portable_component(bytes: &[u8], #[cfg(kani)] witness: usize) -> bool {
    if bytes.is_empty()
        || bytes.len() > COMPONENT_BYTE_LIMIT
        || matches!(bytes.last(), Some(b'.' | b' '))
        || windows_device(bytes)
    {
        return false;
    }
    let mut admitted = TRUE_BIT;
    let mut first_dot_seen = FALSE_BIT;
    let mut previous_space = FALSE_BIT;
    let mut index = 0;
    #[cfg(kani)]
    let witness_byte = component_byte(bytes, witness);
    #[cfg_attr(
        kani,
        kani::loop_invariant(
            index <= bytes.len()
                && bytes.len() <= COMPONENT_BYTE_LIMIT
                && admitted <= TRUE_BIT
                && first_dot_seen <= TRUE_BIT
                && previous_space <= TRUE_BIT
                && (witness >= index
                    || admitted == FALSE_BIT
                    || (witness_byte >= b' '
                        && witness_byte <= b'~'
                        && witness_byte != b'/'
                        && witness_byte != b'\\'
                        && witness_byte != b':'
                        && witness_byte != b'<'
                        && witness_byte != b'>'
                        && witness_byte != b'"'
                        && witness_byte != b'|'
                        && witness_byte != b'?'
                        && witness_byte != b'*'))
        )
    )]
    #[cfg_attr(kani, kani::loop_decreases(bytes.len().wrapping_sub(index)))]
    while index < bytes.len() {
        let byte = component_byte(bytes, index);
        let forbidden = matches!(
            byte,
            b'/' | b'\\' | b':' | b'<' | b'>' | b'"' | b'|' | b'?' | b'*'
        );
        let dot = u8::from(byte == b'.');
        admitted &= u8::from(byte >= b' ')
            & u8::from(byte <= b'~')
            & u8::from(!forbidden)
            & (TRUE_BIT ^ (dot & (TRUE_BIT ^ first_dot_seen) & previous_space));
        first_dot_seen |= dot;
        previous_space = u8::from(byte == b' ');
        index += 1;
    }
    admitted == TRUE_BIT
}

fn windows_device(bytes: &[u8]) -> bool {
    let [first, second, third, ..] = bytes else {
        return false;
    };
    let first = *first | ASCII_CASE_BIT;
    let second = *second | ASCII_CASE_BIT;
    let third = *third | ASCII_CASE_BIT;
    let fourth = component_byte(bytes, SHORT_DEVICE_STEM_BYTES);
    let fifth = component_byte(bytes, NUMBERED_DEVICE_STEM_BYTES);
    let sixth = component_byte(bytes, CONIN_DEVICE_STEM_BYTES - DEVICE_TRAILER_BYTES);
    let seventh = component_byte(bytes, CONOUT_DEVICE_STEM_BYTES - DEVICE_TRAILER_BYTES);
    let eighth = component_byte(bytes, CONOUT_DEVICE_STEM_BYTES);
    let con = first == b'c' && second == b'o' && third == b'n';
    let short = con
        || (first == b'p' && second == b'r' && third == b'n')
        || (first == b'a' && second == b'u' && third == b'x')
        || (first == b'n' && second == b'u' && third == b'l');
    let numbered = ((first == b'c' && second == b'o' && third == b'm')
        || (first == b'l' && second == b'p' && third == b't'))
        && fourth.wrapping_sub(b'1') <= DEVICE_NUMBER_SPAN;
    let conin = con
        && (fourth | ASCII_CASE_BIT) == b'i'
        && (fifth | ASCII_CASE_BIT) == b'n'
        && sixth == b'$';
    let conout = con
        && (fourth | ASCII_CASE_BIT) == b'o'
        && (fifth | ASCII_CASE_BIT) == b'u'
        && (sixth | ASCII_CASE_BIT) == b't'
        && seventh == b'$';
    (short && (bytes.len() == SHORT_DEVICE_STEM_BYTES || fourth == b'.'))
        || (numbered && (bytes.len() == NUMBERED_DEVICE_STEM_BYTES || fifth == b'.'))
        || (conin && (bytes.len() == CONIN_DEVICE_STEM_BYTES || seventh == b'.'))
        || (conout && (bytes.len() == CONOUT_DEVICE_STEM_BYTES || eighth == b'.'))
}

fn component_byte(bytes: &[u8], index: usize) -> u8 {
    bytes.get(index).copied().unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Material {
    Missing,
    Corrupt,
    Verified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acquisition {
    Fetch,
    Reuse,
    RefuseCorrupt,
    RefuseExhausted,
}

#[must_use]
pub const fn acquisition(material: Material, attempts: u8) -> Acquisition {
    match material {
        Material::Corrupt => Acquisition::RefuseCorrupt,
        Material::Verified => Acquisition::Reuse,
        Material::Missing if attempts < FETCH_ATTEMPTS => Acquisition::Fetch,
        Material::Missing => Acquisition::RefuseExhausted,
    }
}

#[must_use]
pub const fn usable(digest: bool, provenance: bool) -> bool {
    digest && provenance
}

/// Require curl's enforced byte limit for responses without an initial size.
#[must_use]
pub const fn bounded_curl(version: [u64; 3]) -> bool {
    version[0] > CURL_BYTE_LIMIT_VERSION[0]
        || (version[0] == CURL_BYTE_LIMIT_VERSION[0] && version[1] >= CURL_BYTE_LIMIT_VERSION[1])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Ready,
    Empty,
    Unresolved,
    Incomplete,
}

#[must_use]
pub const fn admission(required: usize, verified: usize, unresolved: usize) -> Admission {
    if unresolved > 0 {
        Admission::Unresolved
    } else if required == 0 {
        Admission::Empty
    } else if verified != required {
        Admission::Incomplete
    } else {
        Admission::Ready
    }
}

const fn sha256_length(length: usize) -> bool {
    length == SHA256_HEX_BYTES
}

const fn version_length(length: usize) -> bool {
    length > 0 && length <= MAX_VERSION_BYTES
}

/// Decode exactly one lowercase hexadecimal SHA-256 without an algorithm prefix.
#[must_use]
pub fn sha256(bytes: &[u8]) -> Option<[u8; SHA256_BYTES]> {
    if !sha256_length(bytes.len()) {
        return None;
    }
    let mut digest = [0; SHA256_BYTES];
    let mut rest = bytes;
    for byte in &mut digest {
        let [high, low, tail @ ..] = rest else {
            return None;
        };
        *byte = (hexadecimal(*high)? << HEX_DIGIT_BITS) + hexadecimal(*low)?;
        rest = tail;
    }
    Some(digest)
}

const fn hexadecimal(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + HEX_LETTER_VALUE),
        _ => None,
    }
}

const fn lowercase_hexadecimal(byte: u8) -> bool {
    decimal_digit(byte) || byte.wrapping_sub(b'a') <= HEX_LETTER_SPAN
}

const fn decimal_digit(byte: u8) -> bool {
    byte.wrapping_sub(b'0') <= DECIMAL_DIGIT_SPAN
}

/// Admit bounded numeric release identifiers or a complete lowercase Git revision.
#[must_use]
pub fn pinned(version: &[u8]) -> bool {
    if !version_length(version.len()) {
        return false;
    }
    if let Some(revision) = version.strip_prefix(REVISION_PREFIX) {
        let Ok(revision) = <&[u8; GIT_REVISION_BYTES]>::try_from(revision) else {
            return false;
        };
        return revision.iter().fold(true, |canonical, byte| {
            canonical & lowercase_hexadecimal(*byte)
        });
    }
    let mut parts = 1;
    let mut digit = false;
    for byte in version {
        match byte {
            byte if decimal_digit(*byte) => digit = true,
            b'.' if digit && parts < MAX_VERSION_PARTS => {
                parts += 1;
                digit = false;
            }
            _ => return false,
        }
    }
    digit && parts >= MIN_VERSION_PARTS
}

#[cfg(any(test, kani))]
mod reference {
    use super::{MAX_VERSION_PARTS, MIN_VERSION_PARTS};

    #[cfg(test)]
    pub(super) fn split_portable_component(bytes: &[u8]) -> bool {
        if bytes.is_empty() || bytes.len() > super::COMPONENT_BYTE_LIMIT {
            return false;
        }
        let base = bytes.split(|byte| *byte == b'.').next().unwrap_or_default();
        if base.last() == Some(&b' ') {
            return false;
        }
        let reserved = [
            b"CON".as_slice(),
            b"PRN",
            b"AUX",
            b"NUL",
            b"CONIN$",
            b"CONOUT$",
        ]
        .iter()
        .any(|name| base.eq_ignore_ascii_case(name));
        let numbered = matches!(base, [first, second, third, digit]
            if ([*first, *second, *third].eq_ignore_ascii_case(b"COM")
                || [*first, *second, *third].eq_ignore_ascii_case(b"LPT"))
                && matches!(*digit, b'1'..=b'9'));
        !reserved
            && !numbered
            && !matches!(bytes.last(), Some(b'.' | b' '))
            && bytes
                .iter()
                .all(|byte| (b' '..=b'~').contains(byte) && !b"/\\:<>\"|?*".contains(byte))
    }

    pub(super) fn numeric(bytes: &[u8]) -> bool {
        let endpoints = bytes.first().is_some_and(u8::is_ascii_digit)
            && bytes.last().is_some_and(u8::is_ascii_digit);
        if !endpoints {
            return false;
        }
        let (dots, alphabet, adjacent_dots, _) = bytes.iter().fold(
            (0u8, true, false, false),
            |(dots, alphabet, adjacent_dots, previous_dot), byte| {
                let dot = *byte == b'.';
                (
                    (dots + u8::from(dot)).min(MAX_VERSION_PARTS),
                    alphabet && (byte.is_ascii_digit() || dot),
                    adjacent_dots || (previous_dot && dot),
                    dot,
                )
            },
        );
        (MIN_VERSION_PARTS - 1..MAX_VERSION_PARTS).contains(&dots) && alphabet && !adjacent_dots
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    fn reference_hexadecimal(byte: u8) -> Option<u8> {
        if byte.is_ascii_digit() {
            Some(byte - b'0')
        } else if (b'a'..=b'f').contains(&byte) {
            Some(byte - b'a' + HEX_LETTER_VALUE)
        } else {
            None
        }
    }

    #[kani::proof]
    fn http_binding_requires_matching_options_source_and_checksum() {
        let options: bool = kani::any();
        let url: bool = kani::any();
        let checksum: bool = kani::any();
        let result = http_binding(options, url, checksum);
        assert_eq!(result == HttpBinding::Matched, options && url && checksum);
        assert_eq!(result == HttpBinding::OptionsMismatch, !options);
        assert_eq!(result == HttpBinding::SourceMismatch, options && !url);
        assert_eq!(
            result == HttpBinding::ChecksumMismatch,
            options && url && !checksum
        );
        kani::cover!(result == HttpBinding::Matched);
        kani::cover!(result == HttpBinding::OptionsMismatch);
        kani::cover!(result == HttpBinding::SourceMismatch);
        kani::cover!(result == HttpBinding::ChecksumMismatch);
    }

    #[kani::proof]
    fn cache_paths_include_terminators_and_refuse_length_overflow() {
        const MAX_DIRECTORY: usize = 248;
        const MAX_FILE: usize = 259;
        let directory: usize = kani::any();
        let file: usize = kani::any();
        let admitted = cache_paths_fit(directory, file);
        assert_eq!(admitted, directory <= MAX_DIRECTORY && file <= MAX_FILE);
        kani::cover!(admitted);
        kani::cover!(!admitted);
    }
    const HEX_DIGIT_MASK: u8 = 0xf;

    #[kani::proof]
    fn target_selection_requires_applicability_and_refuses_an_explicit_mismatch() {
        let applicable: bool = kani::any();
        let explicit: bool = kani::any();
        let selection = target_selection(applicable, explicit);
        assert_eq!(selection == TargetSelection::Required, applicable);
        assert_eq!(
            selection == TargetSelection::Refused,
            !applicable && explicit
        );
        assert_eq!(
            selection == TargetSelection::Excluded,
            !applicable && !explicit
        );
        kani::cover!(selection == TargetSelection::Required);
        kani::cover!(selection == TargetSelection::Refused);
        kani::cover!(selection == TargetSelection::Excluded);
    }

    #[kani::proof]
    #[kani::unwind(16)]
    #[kani::solver(cadical)]
    fn portable_components_cannot_change_the_archive_destination() {
        let bytes: [u8; COMPONENT_BYTE_LIMIT + 1] = kani::any();
        assert!(u16::try_from(bytes.len()).is_ok());
        let length: u16 = kani::any();
        let length = usize::from(length);
        kani::assume(length <= bytes.len());
        let component = &bytes[..length];
        let position: u8 = kani::any();
        let position = usize::from(position);
        let accepted = portable_component(component, position);
        if accepted {
            assert!(!component.is_empty() && component.len() <= COMPONENT_BYTE_LIMIT);
            assert!(!matches!(component.last(), Some(b'.' | b' ')));
            kani::assume(position < component.len());
            let byte = component[position];
            assert!(byte >= b' ' && byte <= b'~');
            assert!(!matches!(
                byte,
                b'/' | b'\\' | b':' | b'<' | b'>' | b'"' | b'|' | b'?' | b'*'
            ));
        }
        kani::cover!(accepted);
        kani::cover!(!accepted);
    }

    fn device_component<const N: usize>(
        alias: [u8; N],
        mut bytes: [u8; COMPONENT_BYTE_LIMIT],
        casing: u8,
        length: u8,
    ) -> [u8; COMPONENT_BYTE_LIMIT] {
        for (index, byte) in alias.into_iter().enumerate() {
            bytes[index] = if ((casing >> index) & TRUE_BIT) != FALSE_BIT {
                byte.to_ascii_lowercase()
            } else {
                byte
            };
        }
        let length = usize::from(length);
        if length > N {
            bytes[N] = b'.';
        }
        bytes
    }

    #[kani::proof]
    #[kani::unwind(16)]
    #[kani::solver(cadical)]
    fn portable_components_refuse_windows_devices_with_arbitrary_extensions() {
        const DEVICE_FAMILIES: u8 = 24;
        assert!(usize::from(DEVICE_FAMILIES) <= usize::from(u8::MAX) + 1);
        let family: u8 = kani::any();
        kani::assume(family < DEVICE_FAMILIES);
        let alias_length = match family {
            0..=3 => SHORT_DEVICE_STEM_BYTES,
            4 => CONIN_DEVICE_STEM_BYTES,
            5 => CONOUT_DEVICE_STEM_BYTES,
            _ => NUMBERED_DEVICE_STEM_BYTES,
        };
        let bytes: [u8; COMPONENT_BYTE_LIMIT] = kani::any();
        assert!(u8::try_from(bytes.len()).is_ok());
        let length: u8 = kani::any();
        kani::assume(usize::from(length) >= alias_length && usize::from(length) <= bytes.len());
        let casing: u8 = kani::any();
        let bytes = match family {
            0 => device_component(*b"CON", bytes, casing, length),
            1 => device_component(*b"PRN", bytes, casing, length),
            2 => device_component(*b"AUX", bytes, casing, length),
            3 => device_component(*b"NUL", bytes, casing, length),
            4 => device_component(*b"CONIN$", bytes, casing, length),
            5 => device_component(*b"CONOUT$", bytes, casing, length),
            6 => device_component(*b"COM1", bytes, casing, length),
            7 => device_component(*b"COM2", bytes, casing, length),
            8 => device_component(*b"COM3", bytes, casing, length),
            9 => device_component(*b"COM4", bytes, casing, length),
            10 => device_component(*b"COM5", bytes, casing, length),
            11 => device_component(*b"COM6", bytes, casing, length),
            12 => device_component(*b"COM7", bytes, casing, length),
            13 => device_component(*b"COM8", bytes, casing, length),
            14 => device_component(*b"COM9", bytes, casing, length),
            15 => device_component(*b"LPT1", bytes, casing, length),
            16 => device_component(*b"LPT2", bytes, casing, length),
            17 => device_component(*b"LPT3", bytes, casing, length),
            18 => device_component(*b"LPT4", bytes, casing, length),
            19 => device_component(*b"LPT5", bytes, casing, length),
            20 => device_component(*b"LPT6", bytes, casing, length),
            21 => device_component(*b"LPT7", bytes, casing, length),
            22 => device_component(*b"LPT8", bytes, casing, length),
            23 => device_component(*b"LPT9", bytes, casing, length),
            _ => unreachable!(),
        };
        let component = &bytes[..usize::from(length)];
        if windows_device(component) {
            assert!(!portable_component(component, 0));
        } else {
            assert!(false, "a selected Windows device must be recognized");
        }
        kani::cover!(usize::from(length) == alias_length);
        kani::cover!(usize::from(length) == COMPONENT_BYTE_LIMIT);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn curl_has_the_byte_limit_capability_exactly_from_the_supported_release() {
        let version: [u64; 3] = kani::any();
        assert_eq!(bounded_curl(version), version >= CURL_BYTE_LIMIT_VERSION);
        kani::cover!(bounded_curl(version));
        kani::cover!(!bounded_curl(version));
    }

    #[kani::proof]
    fn acquisition_never_reuses_corruption_or_exceeds_the_fetch_budget() {
        let material: Material = kani::any();
        let attempts: u8 = kani::any();
        let result = acquisition(material, attempts);
        assert_eq!(
            result == Acquisition::Fetch,
            material == Material::Missing && attempts < FETCH_ATTEMPTS
        );
        assert_eq!(result == Acquisition::Reuse, material == Material::Verified);
        assert_eq!(
            result == Acquisition::RefuseCorrupt,
            material == Material::Corrupt
        );
        assert_eq!(
            result == Acquisition::RefuseExhausted,
            material == Material::Missing && attempts >= FETCH_ATTEMPTS
        );
        kani::cover!(result == Acquisition::Fetch);
        kani::cover!(result == Acquisition::Reuse);
        kani::cover!(result == Acquisition::RefuseCorrupt);
        kani::cover!(result == Acquisition::RefuseExhausted);
    }

    #[kani::proof]
    fn usable_material_has_both_digest_and_provenance() {
        let digest: bool = kani::any();
        let provenance: bool = kani::any();
        assert_eq!(usable(digest, provenance), digest && provenance);
        kani::cover!(usable(digest, provenance));
        kani::cover!(!usable(digest, provenance));
    }

    #[kani::proof]
    fn admission_requires_every_material_of_a_nonempty_resolved_plan() {
        let required: usize = kani::any();
        let verified: usize = kani::any();
        let unresolved: usize = kani::any();
        let result = admission(required, verified, unresolved);
        assert_eq!(
            result == Admission::Ready,
            required > 0 && verified == required && unresolved == 0
        );
        assert_eq!(result == Admission::Unresolved, unresolved > 0);
        assert_eq!(result == Admission::Empty, unresolved == 0 && required == 0);
        assert_eq!(
            result == Admission::Incomplete,
            unresolved == 0 && required > 0 && verified != required
        );
        kani::cover!(result == Admission::Ready);
        kani::cover!(result == Admission::Unresolved);
        kani::cover!(result == Admission::Empty);
        kani::cover!(result == Admission::Incomplete);
    }

    #[kani::proof]
    fn sha256_length_admits_exactly_the_canonical_encoding_for_every_usize() {
        let length: usize = kani::any();
        let admitted = sha256_length(length);
        assert_eq!(admitted, length == SHA256_HEX_BYTES);
        kani::cover!(admitted);
        kani::cover!(!admitted);
    }

    #[kani::proof]
    fn version_length_admits_exactly_the_supported_domain_for_every_usize() {
        let length: usize = kani::any();
        let admitted = version_length(length);
        assert_eq!(admitted, length > 0 && length <= MAX_VERSION_BYTES);
        kani::cover!(admitted);
        kani::cover!(!admitted && length == 0);
        kani::cover!(!admitted && length > MAX_VERSION_BYTES);
    }

    #[kani::proof]
    #[kani::unwind(66)]
    fn sha256_accepts_exactly_canonical_bytes_and_preserves_the_encoded_value() {
        let encoded: [u8; SHA256_HEX_BYTES] = kani::any();
        let decoded = sha256(&encoded);
        assert_eq!(
            decoded.is_some(),
            encoded
                .iter()
                .all(|byte| reference_hexadecimal(*byte).is_some())
        );
        if let Some(digest) = decoded {
            assert!(u8::try_from(SHA256_BYTES - 1).is_ok());
            let pair: u8 = kani::any();
            let pair = usize::from(pair);
            kani::assume(pair < SHA256_BYTES);
            let [high, low] = encoded.as_chunks::<2>().0[pair];
            let byte = digest[pair];
            assert_eq!(reference_hexadecimal(high), Some(byte >> HEX_DIGIT_BITS));
            assert_eq!(reference_hexadecimal(low), Some(byte & HEX_DIGIT_MASK));
        }
        kani::cover!(decoded.is_some());
        kani::cover!(decoded.is_none());
    }

    #[kani::proof]
    #[kani::unwind(66)]
    fn hexadecimal_reference_matches_the_table_and_actual_decoder_for_every_byte() {
        let byte: u8 = kani::any();
        let expected = reference_hexadecimal(byte);
        assert_eq!(hexadecimal(byte), expected);
        assert_eq!(lowercase_hexadecimal(byte), expected.is_some());
        assert_eq!(expected.is_some(), HEX_DIGITS.contains(&byte));
        assert_eq!(
            expected.is_some(),
            byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
        );
        if let Some(value) = expected {
            assert_eq!(HEX_DIGITS.get(usize::from(value)), Some(&byte));
        }
        kani::cover!(expected.is_some());
        kani::cover!(expected.is_none());
    }

    #[kani::proof]
    #[kani::unwind(66)]
    fn sha256_refuses_wrong_lengths_at_both_encoding_boundaries() {
        let encoded: [u8; SHA256_HEX_BYTES + 1] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= encoded.len());
        kani::assume(length != SHA256_HEX_BYTES);
        assert_eq!(sha256(&encoded[..length]), None);
        kani::cover!(length == 0);
        kani::cover!(length == SHA256_HEX_BYTES + 1);
    }

    #[kani::proof]
    #[kani::unwind(67)]
    fn pins_without_the_revision_prefix_match_the_complete_numeric_grammar() {
        let bytes: [u8; MAX_VERSION_BYTES + 1] = kani::any();
        let length: u8 = kani::any();
        let length = usize::from(length);
        kani::assume(length <= bytes.len());
        let version = &bytes[..length];
        kani::assume(!version.starts_with(REVISION_PREFIX));
        let bounded = !version.is_empty() && length <= MAX_VERSION_BYTES;
        let numeric = reference::numeric(version);
        let result = pinned(version);
        assert!(version.strip_prefix(REVISION_PREFIX).is_none());
        assert_eq!(result, bounded && numeric);
        kani::cover!(result && numeric);
        kani::cover!(!result);
    }

    #[kani::proof]
    #[kani::unwind(67)]
    fn pins_with_the_revision_prefix_match_the_complete_revision_grammar() {
        let bytes: [u8; MAX_VERSION_BYTES + 1] = kani::any();
        let length: u8 = kani::any();
        let length = usize::from(length);
        kani::assume(length <= bytes.len());
        let version = &bytes[..length];
        kani::assume(version.starts_with(REVISION_PREFIX));
        let bounded = !version.is_empty() && length <= MAX_VERSION_BYTES;
        let revision = version.strip_prefix(REVISION_PREFIX).is_some_and(|tail| {
            tail.len() == GIT_REVISION_BYTES
                && tail
                    .iter()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        });
        let result = pinned(version);
        assert!(!reference::numeric(version));
        assert_eq!(result, bounded && revision);
        kani::cover!(result && revision);
        kani::cover!(!result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_bindings_preserve_each_mismatch_and_never_admit_partial_identity() {
        for (options, url, checksum, expected) in [
            (true, true, true, HttpBinding::Matched),
            (false, true, true, HttpBinding::OptionsMismatch),
            (true, false, true, HttpBinding::SourceMismatch),
            (true, true, false, HttpBinding::ChecksumMismatch),
            (false, false, true, HttpBinding::OptionsMismatch),
            (false, true, false, HttpBinding::OptionsMismatch),
            (true, false, false, HttpBinding::SourceMismatch),
            (false, false, false, HttpBinding::OptionsMismatch),
        ] {
            assert_eq!(http_binding(options, url, checksum), expected);
            assert_eq!(expected == HttpBinding::Matched, options && url && checksum);
        }
    }

    #[test]
    fn windows_cache_paths_reserve_directory_append_and_file_terminators() {
        for (directory, file, expected) in [
            (0, 0, true),
            (129, 199, true),
            (189, 259, true),
            (190, 260, false),
            (248, 259, true),
            (249, 259, false),
            (248, 260, false),
            (usize::MAX, 0, false),
            (0, usize::MAX, false),
            (usize::MAX, usize::MAX, false),
        ] {
            assert_eq!(cache_paths_fit(directory, file), expected);
        }
    }

    #[test]
    fn portable_components_match_the_split_reference_at_boundaries() {
        for (value, expected) in [
            ("", false),
            (".", false),
            ("..", false),
            ("a/b", false),
            ("a\\b", false),
            ("C:drive", false),
            ("file.", false),
            ("file ", false),
            ("file .ext", false),
            (" file.ext", true),
            ("file.ext .txt", true),
            (".file .txt", true),
            (" .file", false),
            ("file..ext", true),
            ("file. .ext", true),
            ("conventional.rs", true),
            ("COM0", true),
            ("COM10.txt", true),
            ("LPT10.txt", true),
            ("CONIN$x.txt", true),
            ("日本語", false),
            ("a\n", false),
            ("a\0", false),
        ] {
            assert_eq!(
                reference::split_portable_component(value.as_bytes()),
                expected
            );
            assert_eq!(
                portable_component(
                    value.as_bytes(),
                    #[cfg(kani)]
                    0
                ),
                expected
            );
        }
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(
                portable_component(
                    &[byte],
                    #[cfg(kani)]
                    0
                ),
                reference::split_portable_component(&[byte]),
                "{byte}"
            );
        }
        for length in [0, 1, COMPONENT_BYTE_LIMIT, COMPONENT_BYTE_LIMIT + 1] {
            let bytes = alloc::vec![b'x'; length];
            assert_eq!(
                reference::split_portable_component(&bytes),
                length > 0 && length <= COMPONENT_BYTE_LIMIT
            );
            assert_eq!(
                portable_component(
                    &bytes,
                    #[cfg(kani)]
                    0
                ),
                reference::split_portable_component(&bytes)
            );
        }
    }

    fn assert_device_spellings(cases: &core::ops::Range<u16>) {
        let aliases: [&[u8]; 24] = [
            b"CON", b"PRN", b"AUX", b"NUL", b"CONIN$", b"CONOUT$", b"COM1", b"COM2", b"COM3",
            b"COM4", b"COM5", b"COM6", b"COM7", b"COM8", b"COM9", b"LPT1", b"LPT2", b"LPT3",
            b"LPT4", b"LPT5", b"LPT6", b"LPT7", b"LPT8", b"LPT9",
        ];
        let mut ordinal = 0;
        let mut checked = 0;
        for alias in aliases {
            for casing in 0..(1_u16 << alias.len()) {
                let included = cases.contains(&ordinal);
                ordinal += 1;
                if !included {
                    continue;
                }
                checked += 1;
                let mut spelling = alias.to_vec();
                for (index, byte) in spelling.iter_mut().enumerate() {
                    if casing & (1 << index) != 0 {
                        byte.make_ascii_lowercase();
                    }
                }
                for (suffix, expected) in [
                    (b"".as_slice(), false),
                    (b".txt", false),
                    (b". .txt", false),
                    (b"..txt", false),
                    (b" .txt", false),
                    (b"X.txt", true),
                ] {
                    let mut component = spelling.clone();
                    component.extend_from_slice(suffix);
                    assert_eq!(reference::split_portable_component(&component), expected);
                    assert_eq!(
                        portable_component(
                            &component,
                            #[cfg(kani)]
                            0
                        ),
                        expected,
                        "{component:?}"
                    );
                }
                let mut longest = spelling.clone();
                longest.push(b'.');
                longest.resize(COMPONENT_BYTE_LIMIT, b'x');
                assert!(!reference::split_portable_component(&longest));
                assert!(!portable_component(
                    &longest,
                    #[cfg(kani)]
                    0
                ));
            }
        }
        assert_eq!(ordinal, 512);
        assert_eq!(checked, cases.end - cases.start);
    }

    #[test]
    fn portable_components_match_the_split_reference_for_all_device_spellings_cases_000_063() {
        assert_device_spellings(&(0..64));
    }

    #[test]
    fn portable_components_match_the_split_reference_for_all_device_spellings_cases_064_127() {
        assert_device_spellings(&(64..128));
    }

    #[test]
    fn portable_components_match_the_split_reference_for_all_device_spellings_cases_128_191() {
        assert_device_spellings(&(128..192));
    }

    #[test]
    fn portable_components_match_the_split_reference_for_all_device_spellings_cases_192_255() {
        assert_device_spellings(&(192..256));
    }

    #[test]
    fn portable_components_match_the_split_reference_for_all_device_spellings_cases_256_319() {
        assert_device_spellings(&(256..320));
    }

    #[test]
    fn portable_components_match_the_split_reference_for_all_device_spellings_cases_320_383() {
        assert_device_spellings(&(320..384));
    }

    #[test]
    fn portable_components_match_the_split_reference_for_all_device_spellings_cases_384_447() {
        assert_device_spellings(&(384..448));
    }

    #[test]
    fn portable_components_match_the_split_reference_for_all_device_spellings_cases_448_511() {
        assert_device_spellings(&(448..512));
    }

    fn assert_device_prefix_membership(bytes: &core::ops::RangeInclusive<u8>) {
        for alias in [
            b"CON".as_slice(),
            b"PRN",
            b"AUX",
            b"NUL",
            b"COM1",
            b"LPT1",
            b"CONIN$",
            b"CONOUT$",
        ] {
            for position in 0..alias.len() {
                for byte in (*bytes).clone() {
                    let mut prefix = alias.to_vec();
                    prefix[position] = byte;
                    for suffix in [b"".as_slice(), b".txt"] {
                        let mut component = prefix.clone();
                        component.extend_from_slice(suffix);
                        assert_eq!(
                            portable_component(
                                &component,
                                #[cfg(kani)]
                                0
                            ),
                            reference::split_portable_component(&component),
                            "{component:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_bytes_00_1f()
    {
        assert_device_prefix_membership(&(0x00..=0x1f));
    }

    #[test]
    fn device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_bytes_20_3f()
    {
        assert_device_prefix_membership(&(0x20..=0x3f));
    }

    #[test]
    fn device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_bytes_40_5f()
    {
        assert_device_prefix_membership(&(0x40..=0x5f));
    }

    #[test]
    fn device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_bytes_60_7f()
    {
        assert_device_prefix_membership(&(0x60..=0x7f));
    }

    #[test]
    fn device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_bytes_80_9f()
    {
        assert_device_prefix_membership(&(0x80..=0x9f));
    }

    #[test]
    fn device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_bytes_a0_bf()
    {
        assert_device_prefix_membership(&(0xa0..=0xbf));
    }

    #[test]
    fn device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_bytes_c0_df()
    {
        assert_device_prefix_membership(&(0xc0..=0xdf));
    }

    #[test]
    fn device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_bytes_e0_ff()
    {
        assert_device_prefix_membership(&(0xe0..=0xff));
    }

    #[test]
    fn portable_components_refuse_path_redirects_and_windows_aliases() {
        for value in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "C:",
            "x:",
            "file.",
            "file ",
            "con",
            "cOn.txt",
            "AUX",
            "NUL.rs",
            "PRN",
            "COM1",
            "lpt9.dat",
            "CONIN$",
            "CON .txt",
            "CONOUT$",
            "日本語",
            "a\n",
            "a\0",
        ] {
            assert!(
                !portable_component(
                    value.as_bytes(),
                    #[cfg(kani)]
                    0
                ),
                "{value:?}"
            );
        }
        for value in [
            "Cargo.toml",
            ".gitignore",
            "src",
            "COM0",
            "LPT10",
            "conventional.rs",
            "file name.txt",
        ] {
            assert!(
                portable_component(
                    value.as_bytes(),
                    #[cfg(kani)]
                    0
                ),
                "{value}"
            );
        }
        assert!(portable_component(
            &[b'x'; COMPONENT_BYTE_LIMIT],
            #[cfg(kani)]
            0
        ));
        assert!(!portable_component(
            &[b'x'; COMPONENT_BYTE_LIMIT + 1],
            #[cfg(kani)]
            0
        ));
    }

    #[test]
    fn target_selection_preserves_explicit_requests_and_native_applicability() {
        for (applicable, explicit, expected) in [
            (true, false, TargetSelection::Required),
            (true, true, TargetSelection::Required),
            (false, false, TargetSelection::Excluded),
            (false, true, TargetSelection::Refused),
        ] {
            assert_eq!(target_selection(applicable, explicit), expected);
        }
    }

    #[test]
    fn version_length_rejects_zero_and_accepts_both_inclusive_bounds() {
        for (length, expected) in [
            (0, false),
            (1, true),
            (MAX_VERSION_BYTES, true),
            (MAX_VERSION_BYTES + 1, false),
            (usize::MAX, false),
        ] {
            assert_eq!(version_length(length), expected, "{length}");
        }
    }

    #[test]
    fn numeric_reference_preserves_nonempty_component_grammar() {
        for (version, expected) in [
            ("1.2", true),
            ("00.01", true),
            ("1.2.3.4", true),
            ("", false),
            ("1", false),
            (".1.2", false),
            ("1.2.", false),
            ("1..2", false),
            ("1.2.3.4.5", false),
            ("1.2.3.4.5.6", false),
            ("1.2.3.4.5.6.7.8", false),
            ("1.a", false),
            ("1.a.2", false),
            ("1.*", false),
        ] {
            assert_eq!(
                reference::numeric(version.as_bytes()),
                expected,
                "{version}"
            );
        }
    }

    #[test]
    fn curl_must_enforce_unknown_length_downloads_before_fetching() {
        assert!(!bounded_curl([7, u64::MAX, u64::MAX]));
        assert!(!bounded_curl([8, 3, u64::MAX]));
        assert!(bounded_curl([8, 4, 0]));
        assert!(bounded_curl([8, u64::MAX, 0]));
        assert!(bounded_curl([9, 0, 0]));
        assert!(bounded_curl([u64::MAX, 0, 0]));
    }

    #[test]
    fn corrupt_material_is_never_reused_or_retried() {
        for attempts in 0..=u8::MAX {
            assert_eq!(
                acquisition(Material::Corrupt, attempts),
                Acquisition::RefuseCorrupt
            );
        }
    }

    #[test]
    fn verified_material_is_reused_and_missing_fetches_are_bounded() {
        for attempts in 0..=u8::MAX {
            assert_eq!(
                acquisition(Material::Verified, attempts),
                Acquisition::Reuse
            );
            assert_eq!(
                acquisition(Material::Missing, attempts),
                if attempts < FETCH_ATTEMPTS {
                    Acquisition::Fetch
                } else {
                    Acquisition::RefuseExhausted
                }
            );
        }
    }

    #[test]
    fn usable_material_requires_both_digest_and_provenance() {
        for digest in [false, true] {
            for provenance in [false, true] {
                assert_eq!(usable(digest, provenance), digest && provenance);
            }
        }
    }

    #[test]
    fn admission_requires_a_nonempty_exact_complete_plan() {
        for required in [0, 1, 2, usize::MAX] {
            for verified in [0, 1, 2, usize::MAX] {
                for unresolved in [0, 1, usize::MAX] {
                    let result = admission(required, verified, unresolved);
                    assert_eq!(
                        result == Admission::Ready,
                        required > 0 && verified == required && unresolved == 0
                    );
                    assert_eq!(result == Admission::Unresolved, unresolved > 0);
                    assert_eq!(result == Admission::Empty, unresolved == 0 && required == 0);
                    assert_eq!(
                        result == Admission::Incomplete,
                        unresolved == 0 && required > 0 && verified != required
                    );
                }
            }
        }
    }

    #[test]
    fn decimal_membership_matches_the_independent_ascii_classification_for_every_byte() {
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(decimal_digit(byte), byte.is_ascii_digit());
        }
    }

    #[test]
    fn hexadecimal_membership_matches_the_independent_ascii_classification_for_every_byte() {
        for byte in u8::MIN..=u8::MAX {
            let admitted = hexadecimal(byte).is_some();
            assert_eq!(lowercase_hexadecimal(byte), admitted);
            assert_eq!(admitted, HEX_DIGITS.contains(&byte));
            assert_eq!(
                admitted,
                byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
            );
        }
    }

    #[test]
    fn sha256_decodes_every_lowercase_hex_pair_at_every_position() {
        let digits = HEX_DIGITS;
        for (high, &first) in digits.iter().enumerate() {
            for (low, &second) in digits.iter().enumerate() {
                let mut encoded = [b'0'; SHA256_HEX_BYTES];
                for [left, right] in encoded.as_chunks_mut::<2>().0 {
                    *left = first;
                    *right = second;
                }
                let expected = u8::try_from(high * HEX_DIGITS.len() + low).ok();
                assert!(expected.is_some());
                assert_eq!(
                    sha256(&encoded),
                    expected.map(|value| [value; SHA256_BYTES])
                );
            }
        }
    }

    fn assert_sha256_invalid_bytes(bytes: core::ops::RangeInclusive<u8>) {
        for invalid in bytes {
            if HEX_DIGITS.contains(&invalid) {
                continue;
            }
            for position in 0..SHA256_HEX_BYTES {
                let mut encoded = [b'0'; SHA256_HEX_BYTES];
                if let Some(byte) = encoded.get_mut(position) {
                    *byte = invalid;
                }
                assert_eq!(sha256(&encoded), None);
            }
        }
    }

    #[test]
    fn sha256_rejects_every_noncanonical_byte_at_every_position_bytes_00_1f() {
        assert_sha256_invalid_bytes(0x00..=0x1f);
    }

    #[test]
    fn sha256_rejects_every_noncanonical_byte_at_every_position_bytes_20_3f() {
        assert_sha256_invalid_bytes(0x20..=0x3f);
    }

    #[test]
    fn sha256_rejects_every_noncanonical_byte_at_every_position_bytes_40_5f() {
        assert_sha256_invalid_bytes(0x40..=0x5f);
    }

    #[test]
    fn sha256_rejects_every_noncanonical_byte_at_every_position_bytes_60_7f() {
        assert_sha256_invalid_bytes(0x60..=0x7f);
    }

    #[test]
    fn sha256_rejects_every_noncanonical_byte_at_every_position_bytes_80_9f() {
        assert_sha256_invalid_bytes(0x80..=0x9f);
    }

    #[test]
    fn sha256_rejects_every_noncanonical_byte_at_every_position_bytes_a0_bf() {
        assert_sha256_invalid_bytes(0xa0..=0xbf);
    }

    #[test]
    fn sha256_rejects_every_noncanonical_byte_at_every_position_bytes_c0_df() {
        assert_sha256_invalid_bytes(0xc0..=0xdf);
    }

    #[test]
    fn sha256_rejects_every_noncanonical_byte_at_every_position_bytes_e0_ff() {
        assert_sha256_invalid_bytes(0xe0..=0xff);
    }

    #[test]
    fn sha256_rejects_noncanonical_lengths_and_algorithm_prefixes() {
        assert_eq!(sha256(b""), None);
        assert_eq!(sha256(&[b'0'; SHA256_HEX_BYTES - 1]), None);
        assert_eq!(sha256(&[b'0'; SHA256_HEX_BYTES + 1]), None);
        assert_eq!(
            sha256(b"sha256:0000000000000000000000000000000000000000000000000000000000000000"),
            None
        );
    }

    #[test]
    fn pinned_versions_are_numeric_components_or_complete_lowercase_git_revisions() {
        for version in [
            "5.9",
            "0.2.0",
            "1.2.3.4",
            "2026.10.0",
            "00.01",
            "rev:f8dbfbb67fc7178aa22a44f572d332b7d9b17d0d",
        ] {
            assert!(pinned(version.as_bytes()), "{version}");
        }
        for version in [
            "",
            "1",
            "stable",
            "latest",
            "nightly-2026-08-21",
            "temurin-25",
            "1.*",
            "1.2-beta",
            "v1.2",
            " 1.2",
            "1.2 ",
            ".1.2",
            "1..2",
            "1.2.",
            "1.2.3.4.5",
            "rev:",
            "rev:f8dbfbb67fc7178aa22a44f572d332b7d9b17d0",
            "rev:f8dbfbb67fc7178aa22a44f572d332b7d9b17d0d0",
            "rev:F8dbfbb67fc7178aa22a44f572d332b7d9b17d0d",
            "rev:g8dbfbb67fc7178aa22a44f572d332b7d9b17d0d",
        ] {
            assert!(!pinned(version.as_bytes()), "{version}");
        }
        let mut two_invalid_revision_bytes = [b'0'; REVISION_PREFIX.len() + GIT_REVISION_BYTES];
        two_invalid_revision_bytes[..REVISION_PREFIX.len()].copy_from_slice(REVISION_PREFIX);
        two_invalid_revision_bytes[REVISION_PREFIX.len()] = b'g';
        two_invalid_revision_bytes[REVISION_PREFIX.len() + 1] = b'g';
        assert!(!pinned(&two_invalid_revision_bytes));
        let mut longest = [b'0'; MAX_VERSION_BYTES];
        if let Some(separator) = longest.get_mut(1) {
            *separator = b'.';
        }
        assert!(pinned(&longest));
        let mut overlong = [b'0'; MAX_VERSION_BYTES + 1];
        if let Some(separator) = overlong.get_mut(1) {
            *separator = b'.';
        }
        assert!(!pinned(&overlong));
    }
}
