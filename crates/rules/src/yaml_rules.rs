/// The raw control character restriction shared by YAML's `c-printable` and `nb-json` productions.
#[must_use]
pub const fn raw_character(character: char) -> bool {
    character >= ' ' || matches!(character, '\t' | '\n' | '\r')
}

#[cfg(any(test, kani))]
mod proofs {
    use super::raw_character;

    fn spec_raw_character(character: char) {
        let forbidden_control =
            matches!(character, '\0'..='\u{1f}') && !matches!(character, '\t' | '\n' | '\r');
        assert_eq!(raw_character(character), !forbidden_control);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(1)]
    fn raw_character_rejects_exactly_forbidden_c0_controls() {
        let character: char = kani::any();
        spec_raw_character(character);
        kani::cover!(character == '\0' && !raw_character(character));
        kani::cover!(character == '\n' && raw_character(character));
        kani::cover!(character == '\u{10ffff}' && raw_character(character));
    }

    #[test]
    fn controls_layout_and_unicode_follow_the_source_alphabet() {
        for character in [
            '\0',
            '\u{1}',
            '\u{8}',
            '\t',
            '\n',
            '\u{b}',
            '\u{c}',
            '\r',
            '\u{1f}',
            ' ',
            '~',
            '\u{7f}',
            '\u{85}',
            '\u{4e00}',
            '\u{10ffff}',
        ] {
            spec_raw_character(character);
        }
    }
}
