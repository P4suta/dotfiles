//! Text contrast between two colors, computed from a committed linearization table with integer arithmetic.

/// Each 8-bit sRGB channel value as linear light, scaled by 10^7 and rounded.
/// `xtask/tests/theme.rs` recomputes every entry from the sRGB transfer function.
pub const LINEAR: [u32; 256] = [
    0, 3035, 6071, 9106, 12141, 15176, 18212, 21247, 24282, 27317, 30353, 33465, 36765, 40247,
    43914, 47770, 51815, 56054, 60488, 65121, 69954, 74990, 80232, 85681, 91341, 97212, 103_298,
    109_601, 116_122, 122_865, 129_830, 137_021, 144_438, 152_085, 159_963, 168_074, 176_420,
    185_002, 193_824, 202_886, 212_190, 221_739, 231_534, 241_576, 251_869, 262_412, 273_209,
    284_260, 295_568, 307_134, 318_960, 331_048, 343_398, 356_013, 368_895, 382_044, 395_462,
    409_152, 423_114, 437_350, 451_862, 466_651, 481_718, 497_066, 512_695, 528_606, 544_803,
    561_285, 578_054, 595_112, 612_461, 630_100, 648_033, 666_259, 684_782, 703_601, 722_719,
    742_136, 761_854, 781_874, 802_198, 822_827, 843_762, 865_005, 886_556, 908_417, 930_590,
    953_075, 975_873, 998_987, 1_022_417, 1_046_165, 1_070_231, 1_094_617, 1_119_324, 1_144_354,
    1_169_707, 1_195_384, 1_221_388, 1_247_718, 1_274_377, 1_301_365, 1_328_683, 1_356_333,
    1_384_316, 1_412_633, 1_441_285, 1_470_273, 1_499_598, 1_529_262, 1_559_265, 1_589_608,
    1_620_294, 1_651_322, 1_682_694, 1_714_411, 1_746_474, 1_778_884, 1_811_642, 1_844_750,
    1_878_208, 1_912_017, 1_946_178, 1_980_693, 2_015_563, 2_050_787, 2_086_369, 2_122_308,
    2_158_605, 2_195_262, 2_232_280, 2_269_659, 2_307_400, 2_345_506, 2_383_976, 2_422_811,
    2_462_013, 2_501_583, 2_541_521, 2_581_829, 2_622_507, 2_663_556, 2_704_978, 2_746_773,
    2_788_943, 2_831_487, 2_874_408, 2_917_706, 2_961_383, 3_005_438, 3_049_873, 3_094_689,
    3_139_887, 3_185_468, 3_231_432, 3_277_781, 3_324_515, 3_371_636, 3_419_144, 3_467_041,
    3_515_326, 3_564_001, 3_613_068, 3_662_526, 3_712_377, 3_762_621, 3_813_260, 3_864_294,
    3_915_725, 3_967_552, 4_019_778, 4_072_402, 4_125_426, 4_178_851, 4_232_677, 4_286_905,
    4_341_536, 4_396_572, 4_452_012, 4_507_858, 4_564_110, 4_620_770, 4_677_838, 4_735_315,
    4_793_202, 4_851_499, 4_910_208, 4_969_330, 5_028_865, 5_088_813, 5_149_177, 5_209_956,
    5_271_151, 5_332_764, 5_394_795, 5_457_245, 5_520_114, 5_583_404, 5_647_115, 5_711_248,
    5_775_804, 5_840_784, 5_906_188, 5_972_018, 6_038_273, 6_104_956, 6_172_066, 6_239_604,
    6_307_571, 6_375_969, 6_444_797, 6_514_056, 6_583_748, 6_653_873, 6_724_432, 6_795_425,
    6_866_853, 6_938_718, 7_011_019, 7_083_758, 7_156_935, 7_230_551, 7_304_607, 7_379_104,
    7_454_042, 7_529_422, 7_605_245, 7_681_511, 7_758_222, 7_835_378, 7_912_979, 7_991_027,
    8_069_523, 8_148_466, 8_227_858, 8_307_699, 8_387_990, 8_468_732, 8_549_926, 8_631_572,
    8_713_671, 8_796_224, 8_879_231, 8_962_694, 9_046_612, 9_130_987, 9_215_819, 9_301_109,
    9_386_857, 9_473_065, 9_559_734, 9_646_862, 9_734_453, 9_822_506, 9_911_021, 10_000_000,
];

/// Luminance weights for red, green, and blue, scaled by 10^4.
const WEIGHTS: [u64; 3] = [2126, 7152, 722];

/// The 0.05 flare on the 10^11 luminance scale.
const FLARE: u64 = 5_000_000_000;

/// The numerator and denominator of the 4.5:1 text contrast threshold.
const LEAST_NUMERATOR: u64 = 9;
const LEAST_DENOMINATOR: u64 = 2;

/// Contrast ratios in thousandths.
const MILLIS: u64 = 1000;
#[cfg(any(test, kani))]
const MAX_LINEAR: u64 = 10_000_000;
#[cfg(any(test, kani))]
const MAX_LUMINANCE: u64 = MAX_LINEAR * 10_000;
#[cfg(any(test, kani))]
const MAX_RATIO: u64 = 21 * MILLIS;

/// The hexadecimal digits of one `#rrggbb` literal.
const HEX_DIGITS: usize = 6;
/// The hexadecimal digits of one channel.
const DIGITS_PER_CHANNEL: usize = 2;
const HEX_RADIX: u8 = 16;
const NIBBLE: u32 = 4;

/// An sRGB color with 8-bit channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[cfg_attr(kani, kani::ensures(|result: &u64| *result <= MAX_LINEAR))]
fn linear(channel: u8) -> u64 {
    LINEAR
        .get(usize::from(channel))
        .copied()
        .map_or(0, u64::from)
}

/// The relative luminance of a color, scaled by 10^11.
#[cfg_attr(kani, kani::ensures(|result: &u64| *result <= MAX_LUMINANCE))]
fn luminance(color: Rgb) -> u64 {
    let [red, green, blue] = WEIGHTS;
    red * linear(color.red) + green * linear(color.green) + blue * linear(color.blue)
}

/// The lighter and the darker of two luminances, each with the flare added.
const fn ordered_luminances(first: u64, second: u64) -> (u64, u64) {
    let first = FLARE + first;
    let second = FLARE + second;
    if first >= second {
        (first, second)
    } else {
        (second, first)
    }
}

/// Whether two luminances keep 4.5:1, compared without a division.
const fn keeps_least_contrast(first: u64, second: u64) -> bool {
    let (lighter, darker) = ordered_luminances(first, second);
    LEAST_DENOMINATOR * lighter >= LEAST_NUMERATOR * darker
}

/// The contrast ratio of two colors in thousandths, rounded down, which the gate reports for the lowest pair.
#[must_use]
pub fn ratio_millis(first: Rgb, second: Rgb) -> u64 {
    ratio_luminances(luminance(first), luminance(second))
}

#[cfg_attr(kani, kani::requires(first <= MAX_LUMINANCE && second <= MAX_LUMINANCE))]
fn ratio_luminances(first: u64, second: u64) -> u64 {
    let (lighter, darker) = ordered_luminances(first, second);
    ratio_ordered(lighter, darker)
}

#[cfg_attr(kani, kani::requires(darker >= FLARE && lighter >= darker && lighter <= MAX_LUMINANCE + FLARE))]
fn ratio_ordered(lighter: u64, darker: u64) -> u64 {
    lighter * MILLIS / darker
}

/// Whether text in one color on the other keeps 4.5:1 contrast.
#[must_use]
pub fn legible(foreground: Rgb, background: Rgb) -> bool {
    keeps_least_contrast(luminance(foreground), luminance(background))
}

fn digit(byte: u8) -> Option<u8> {
    char::from(byte)
        .to_digit(u32::from(HEX_RADIX))
        .and_then(|value| u8::try_from(value).ok())
}

/// The color a `#rrggbb` literal names, in either case.
#[must_use]
pub fn parse(text: &[u8]) -> Option<Rgb> {
    let [b'#', digits @ ..] = text else {
        return None;
    };
    if digits.len() != HEX_DIGITS {
        return None;
    }
    let mut channels = [0u8; 3];
    for (channel, pair) in channels.iter_mut().zip(digits.chunks(DIGITS_PER_CHANNEL)) {
        let [high, low] = pair else {
            return None;
        };
        *channel = (digit(*high)? << NIBBLE) + digit(*low)?;
    }
    let [red, green, blue] = channels;
    Some(Rgb { red, green, blue })
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_legible(foreground: Rgb, background: Rgb) {
        assert_eq!(
            legible(foreground, background),
            legible(background, foreground)
        );
        assert!(!legible(foreground, foreground));
    }

    #[cfg(kani)]
    #[kani::proof_for_contract(linear)]
    fn every_channel_has_bounded_linear_light() {
        let value = linear(kani::any());
        kani::cover!(value == 0);
        kani::cover!(value > 0);
    }

    #[cfg(kani)]
    #[kani::proof_for_contract(luminance)]
    #[kani::stub_verified(linear)]
    fn every_rgb_color_has_bounded_luminance() {
        let value = luminance(Rgb {
            red: kani::any(),
            green: kani::any(),
            blue: kani::any(),
        });
        kani::cover!(value == 0);
        kani::cover!(value > 0);
    }

    #[cfg(kani)]
    #[kani::proof_for_contract(ratio_luminances)]
    fn every_luminance_pair_has_safe_ratio_arithmetic() {
        let first = kani::any();
        let second = kani::any();
        let _ratio = ratio_luminances(first, second);
        kani::cover!(first > second);
        kani::cover!(first < second);
    }

    #[cfg(kani)]
    #[kani::proof_for_contract(ratio_ordered)]
    fn every_ordered_pair_has_safe_ratio_arithmetic() {
        let lighter = kani::any();
        let darker = kani::any();
        let _ratio = ratio_ordered(lighter, darker);
        kani::cover!(lighter == darker);
        kani::cover!(lighter > darker);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::stub_verified(luminance)]
    fn every_rgb_pair_has_safe_ratio_arithmetic() {
        let first = Rgb {
            red: kani::any(),
            green: kani::any(),
            blue: kani::any(),
        };
        let second = Rgb {
            red: kani::any(),
            green: kani::any(),
            blue: kani::any(),
        };
        let _ratio = ratio_millis(first, second);
        kani::cover!(first.red > second.red);
        kani::cover!(first.red < second.red);
    }

    /// Symmetry and irreflexivity for every luminance pair.
    #[cfg(kani)]
    #[kani::proof]
    fn the_least_contrast_is_symmetric_and_irreflexive_over_every_luminance() {
        let first: u64 = kani::any();
        let second: u64 = kani::any();
        kani::assume(first <= MAX_LUMINANCE && second <= MAX_LUMINANCE);
        assert_eq!(
            keeps_least_contrast(first, second),
            keeps_least_contrast(second, first)
        );
        assert!(!keeps_least_contrast(first, first));
        kani::cover!(keeps_least_contrast(first, second));
        kani::cover!(!keeps_least_contrast(first, second));
    }

    /// A gray of any level meets the specification on black.
    #[cfg(kani)]
    #[kani::proof]
    fn legibility_of_a_gray_on_black_is_symmetric() {
        let level: u8 = kani::any();
        let gray = Rgb {
            red: level,
            green: level,
            blue: level,
        };
        let black = Rgb {
            red: 0,
            green: 0,
            blue: 0,
        };
        spec_legible(gray, black);
        kani::cover!(legible(gray, black));
        kani::cover!(!legible(gray, black));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn a_literal_names_the_channels_its_digits_spell() {
        let digits: [u8; 6] = kani::any();
        let mut text = [b'#'; 7];
        for (slot, byte) in text.iter_mut().skip(1).zip(digits) {
            *slot = byte;
        }
        if let Some(color) = parse(&text) {
            let [high, low, ..] = digits;
            let value = |byte: u8| digit(byte).unwrap_or(0);
            assert_eq!(color.red, (value(high) << NIBBLE) | value(low));
        }
        kani::cover!(parse(&text).is_some());
        kani::cover!(parse(&text).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reported ratio and the decision agree on the text contrast threshold.
    fn spec_ratio(foreground: Rgb, background: Rgb) {
        assert_eq!(
            legible(foreground, background),
            ratio_millis(foreground, background) >= LEAST_NUMERATOR * MILLIS / LEAST_DENOMINATOR
        );
        assert!(ratio_millis(foreground, background) >= MILLIS);
    }

    const BLACK: Rgb = Rgb {
        red: 0,
        green: 0,
        blue: 0,
    };
    const WHITE: Rgb = Rgb {
        red: 255,
        green: 255,
        blue: 255,
    };

    #[test]
    fn contrast_meets_the_wcag_values_and_the_specification() {
        assert_eq!(ratio_millis(BLACK, WHITE), MAX_RATIO);
        assert_eq!(luminance(WHITE), MAX_LUMINANCE);
        assert!(legible(BLACK, WHITE));
        let gray = parse(b"#767676").unwrap();
        assert!(legible(gray, WHITE), "#767676 on white is 4.54:1");
        assert!(!legible(parse(b"#777777").unwrap(), WHITE));
        assert_eq!(
            parse(b"#1A1b26"),
            Some(Rgb {
                red: 0x1a,
                green: 0x1b,
                blue: 0x26
            })
        );
        assert_eq!(parse(b"1a1b26"), None);
        assert_eq!(parse(b"#1a1b2"), None);
        assert_eq!(parse(b"#1a1b2g"), None);
        assert!(
            LINEAR
                .windows(2)
                .all(|pair| matches!(pair, [lower, higher] if lower < higher))
        );
        for level in (0..=255u8).step_by(5) {
            let color = Rgb {
                red: level,
                green: level.wrapping_mul(7),
                blue: 255 - level,
            };
            for other in [BLACK, WHITE, gray, color] {
                proofs::spec_legible(color, other);
                spec_ratio(color, other);
            }
        }
    }
}
