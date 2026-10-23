use super::sources::{self, Authority, Source};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_READING_CHARS: usize = 128;
const KATAKANA_OFFSET: u32 = 0x60;
const MORA: &[(&str, &str)] = &[
    ("きゃ", "kya"),
    ("きゅ", "kyu"),
    ("きょ", "kyo"),
    ("しゃ", "sha"),
    ("しゅ", "shu"),
    ("しょ", "sho"),
    ("ちゃ", "cha"),
    ("ちゅ", "chu"),
    ("ちょ", "cho"),
    ("にゃ", "nya"),
    ("にゅ", "nyu"),
    ("にょ", "nyo"),
    ("ひゃ", "hya"),
    ("ひゅ", "hyu"),
    ("ひょ", "hyo"),
    ("みゃ", "mya"),
    ("みゅ", "myu"),
    ("みょ", "myo"),
    ("りゃ", "rya"),
    ("りゅ", "ryu"),
    ("りょ", "ryo"),
    ("ぎゃ", "gya"),
    ("ぎゅ", "gyu"),
    ("ぎょ", "gyo"),
    ("じゃ", "ja"),
    ("じゅ", "ju"),
    ("じょ", "jo"),
    ("ぢゃ", "ja"),
    ("ぢゅ", "ju"),
    ("ぢょ", "jo"),
    ("びゃ", "bya"),
    ("びゅ", "byu"),
    ("びょ", "byo"),
    ("ぴゃ", "pya"),
    ("ぴゅ", "pyu"),
    ("ぴょ", "pyo"),
    ("あ", "a"),
    ("い", "i"),
    ("う", "u"),
    ("え", "e"),
    ("お", "o"),
    ("か", "ka"),
    ("き", "ki"),
    ("く", "ku"),
    ("け", "ke"),
    ("こ", "ko"),
    ("さ", "sa"),
    ("し", "shi"),
    ("す", "su"),
    ("せ", "se"),
    ("そ", "so"),
    ("た", "ta"),
    ("ち", "chi"),
    ("つ", "tsu"),
    ("て", "te"),
    ("と", "to"),
    ("な", "na"),
    ("に", "ni"),
    ("ぬ", "nu"),
    ("ね", "ne"),
    ("の", "no"),
    ("は", "ha"),
    ("ひ", "hi"),
    ("ふ", "fu"),
    ("へ", "he"),
    ("ほ", "ho"),
    ("ま", "ma"),
    ("み", "mi"),
    ("む", "mu"),
    ("め", "me"),
    ("も", "mo"),
    ("や", "ya"),
    ("ゆ", "yu"),
    ("よ", "yo"),
    ("ら", "ra"),
    ("り", "ri"),
    ("る", "ru"),
    ("れ", "re"),
    ("ろ", "ro"),
    ("わ", "wa"),
    ("を", "o"),
    ("が", "ga"),
    ("ぎ", "gi"),
    ("ぐ", "gu"),
    ("げ", "ge"),
    ("ご", "go"),
    ("ざ", "za"),
    ("じ", "ji"),
    ("ず", "zu"),
    ("ぜ", "ze"),
    ("ぞ", "zo"),
    ("だ", "da"),
    ("ぢ", "ji"),
    ("づ", "zu"),
    ("で", "de"),
    ("ど", "do"),
    ("ば", "ba"),
    ("び", "bi"),
    ("ぶ", "bu"),
    ("べ", "be"),
    ("ぼ", "bo"),
    ("ぱ", "pa"),
    ("ぴ", "pi"),
    ("ぷ", "pu"),
    ("ぺ", "pe"),
    ("ぽ", "po"),
];

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "name", rename_all = "kebab-case", deny_unknown_fields)]
enum Standard {
    Cabinet2025Kana,
    Attested { title: String, source: Source },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reading {
    written: String,
    kana: String,
    romanized: String,
    context: String,
    source: Source,
    standard: Standard,
}

pub fn records(reading: &Reading) -> Vec<&Source> {
    let mut sources = vec![&reading.source];
    if let Standard::Attested { source, .. } = &reading.standard {
        sources.push(source);
    }
    sources
}

fn hiragana(text: &str) -> String {
    text.chars()
        .map(|character| {
            if ('ァ'..='ヶ').contains(&character) {
                char::from_u32(u32::from(character) - KATAKANA_OFFSET).unwrap_or(character)
            } else {
                character
            }
        })
        .collect()
}

/// Convert one confirmed lexical kana form.
fn romanize(kana: &str) -> Result<String> {
    ensure!(
        !kana.is_empty() && kana.chars().count() <= MAX_READING_CHARS,
        "reading is empty or exceeds its structural bound"
    );
    let normalized = hiragana(kana);
    let mut remaining = normalized.as_str();
    let mut output = String::new();
    let mut geminate = false;
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix('っ') {
            ensure!(!geminate, "repeated sokuon needs a lexical exception");
            geminate = true;
            remaining = rest;
            continue;
        }
        if let Some(rest) = remaining.strip_prefix('ん') {
            ensure!(!geminate, "sokuon cannot precede the moraic nasal");
            output.push('n');
            if rest.starts_with(['あ', 'い', 'う', 'え', 'お', 'や', 'ゆ', 'よ']) {
                output.push('\'');
            }
            remaining = rest;
            continue;
        }
        if let Some(rest) = remaining.strip_prefix('ー') {
            ensure!(!geminate, "sokuon cannot precede a long-vowel mark");
            let vowel = output
                .chars()
                .last()
                .filter(|last| "aiueo".contains(*last))
                .context("long-vowel mark has no preceding vowel")?;
            output.push(vowel);
            remaining = rest;
            continue;
        }
        let Some((mora, romanized)) = MORA.iter().find(|(mora, _)| remaining.starts_with(mora))
        else {
            bail!(
                "reading contains an unsupported mora; preserve the Japanese term or supply an attested spelling"
            );
        };
        if geminate {
            let consonant = romanized
                .chars()
                .next()
                .filter(|first| "kcstphfgzdbj".contains(*first))
                .context("sokuon precedes an unsupported consonant")?;
            output.push(consonant);
            geminate = false;
        }
        output.push_str(romanized);
        remaining = remaining
            .strip_prefix(mora)
            .context("matched mora disappeared")?;
    }
    ensure!(
        !geminate,
        "terminal sokuon needs context and cannot form a lexical spelling"
    );
    Ok(output)
}

pub fn inspect(reading: &Reading, draft: &str) -> Result<()> {
    ensure!(
        !reading.kana.is_empty() && !reading.romanized.trim().is_empty(),
        "a lexical reading and its spelling must be nonempty"
    );
    ensure!(
        reading
            .kana
            .chars()
            .all(|character| ('ぁ'..='ゖ').contains(&character)
                || ('ァ'..='ヺ').contains(&character)
                || character == 'ー'),
        "the lexical reading must contain kana rather than an inferred Latin spelling"
    );
    ensure!(
        !reading.written.is_empty() && !reading.context.trim().is_empty(),
        "a reading needs its original term and context"
    );
    ensure!(
        draft.contains(&reading.written) && draft.contains(&reading.romanized),
        "reading evidence does not refer to this answer draft"
    );
    ensure!(
        reading.source.authority != Authority::Technical,
        "a reading needs a public lexical record or a major dictionary entry"
    );
    sources::validate(&reading.source)?;
    ensure!(
        reading.source.excerpt.contains(&reading.written)
            && hiragana(&reading.source.excerpt).contains(&hiragana(&reading.kana)),
        "lexical entry does not attest this term and kana reading together"
    );
    match &reading.standard {
        Standard::Cabinet2025Kana => ensure!(
            romanize(&reading.kana)? == reading.romanized,
            "romanization differs from the confirmed kana form under the 2025 Cabinet table"
        ),
        Standard::Attested { title, source } => {
            ensure!(
                !title.trim().is_empty(),
                "an attested spelling needs its governing standard"
            );
            sources::validate(source)?;
            ensure!(
                source.excerpt.contains(&reading.written)
                    && source.excerpt.contains(&reading.romanized),
                "the governing record does not attest this spelling for this term"
            );
        }
    }
    Ok(())
}

pub fn check(reading: &Reading, draft: &str, fetched: &mut BTreeMap<String, String>) -> Result<()> {
    inspect(reading, draft)?;
    sources::check(&reading.source, fetched)?;
    if let Standard::Attested { source, .. } = &reading.standard {
        sources::check(source, fetched)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_readings_follow_the_named_table() {
        for (kana, spelling) in [
            ("げんたいけん", "gentaiken"),
            ("ゲンタイケン", "gentaiken"),
            ("へいせい", "heisei"),
            ("とけいだい", "tokeidai"),
            ("しんばし", "shinbashi"),
            ("てんぷら", "tenpura"),
            ("ほんやく", "hon'yaku"),
            ("たんい", "tan'i"),
            ("まっちゃ", "maccha"),
            ("にっちょく", "nicchoku"),
            ("にいがた", "niigata"),
            ("コーヒー", "koohii"),
            ("きょう", "kyou"),
        ] {
            assert_eq!(romanize(kana).unwrap(), spelling, "{kana}");
        }
        assert_ne!(romanize("げんたいけん").unwrap(), "genteiken");
        for invalid in [
            "",
            "原体験",
            "っ",
            "っっか",
            "っあ",
            "っん",
            "ー",
            "ティー",
            "し。",
            "ha",
        ] {
            assert!(romanize(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn every_registered_mora_has_one_spelling_and_composes_at_word_boundaries() {
        let mut unique = std::collections::BTreeSet::new();
        for (kana, spelling) in MORA {
            assert!(unique.insert(kana));
            assert!(!spelling.is_empty() && spelling.bytes().all(|byte| byte.is_ascii_lowercase()));
            assert_eq!(romanize(kana).unwrap(), *spelling);
            assert_eq!(
                romanize(&format!("{kana}ん")).unwrap(),
                format!("{spelling}n")
            );
        }
    }
}
