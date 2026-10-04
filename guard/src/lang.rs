//! Text that does not belong in this machine's repositories.
//!
//! dotguard:allow-foreign — this file has to contain the characters it detects, and its tests have to contain Chinese sentences to assert they are caught.
//!
//! Three separate problems wear the same coat, and all three are things that arrive without anyone typing them — pasted from a search result, produced by a model that slipped language mid-sentence, copied out of a rendered page:
//!
//! 1. **Another writing system.** Everything here is written in Japanese and English.
//!    Hangul, Cyrillic, Thai, Devanagari and the rest are, in this context, always contamination rather than content.
//!
//! 2. **Simplified Chinese.** The hard case, because Japanese and Chinese share most of their ideographs — 日本語 and 日本語 are the same codepoints in both.
//!    What is *not* shared is the PRC's simplified forms: a Chinese sentence of any length contains several of 这 个 说 时 门 现, none of which is a Japanese kanji.
//!    Unicode laid most of them out in contiguous runs by radical (the whole 讠 block U+8BA0–U+8C36, the 钅 block, the 纟 block …)
//!    , so a dozen ranges plus a list of the common stragglers covers it without shipping a Unihan table.
//!
//! 3. **Invisible characters.** Zero-width spaces, bidirectional overrides, byte-order marks.
//!    The bidi ones are the Trojan Source attack: text that a reviewer reads in one order and a compiler reads in another.
//!    These are never intentional in a commit message or a source file.
//!
//! Latin-1 accents pass — `naïve`, `Gödel`, `São` are ordinary English and ordinary names.
//! Greek passes one letter at a time, because `λ`, `μs` and `Σ` are technical notation, but a run of three or more is a Greek *word*.
//!
//! Cyrillic is refused even as a single character, and that rule earns its keep twice over: Cyrillic `а е о р с у х` are pixel-identical to Latin `a e o p c y x`, which is how a homoglyph gets into an identifier, a URL or a package name and survives review.

/// Writing systems that are never in play here.
/// Each entry is an inclusive codepoint range and the name used when reporting it.
const BLOCKED: &[(u32, u32, &str)] = &[
    (0x0400, 0x052F, "Cyrillic"),
    (0x2DE0, 0x2DFF, "Cyrillic"),
    (0xA640, 0xA69F, "Cyrillic"),
    (0x0530, 0x058F, "Armenian"),
    (0x0590, 0x05FF, "Hebrew"),
    (0x0600, 0x06FF, "Arabic"),
    (0x0750, 0x077F, "Arabic"),
    (0x08A0, 0x08FF, "Arabic"),
    (0xFB50, 0xFDFF, "Arabic"),
    (0xFE70, 0xFEFE, "Arabic"),
    (0x0900, 0x097F, "Devanagari"),
    (0x0980, 0x09FF, "Bengali"),
    (0x0B80, 0x0BFF, "Tamil"),
    (0x0E00, 0x0E7F, "Thai"),
    (0x0E80, 0x0EFF, "Lao"),
    (0x1000, 0x109F, "Myanmar"),
    (0x10A0, 0x10FF, "Georgian"),
    (0x1200, 0x137F, "Ethiopic"),
    (0x1780, 0x17FF, "Khmer"),
    // Hangul: Jamo, Compatibility Jamo, Extended-A, and the syllable block.
    // U+3190–U+319F, immediately after Compatibility Jamo, is Kanbun — Japanese annotation marks — which is why the range stops at U+318F.
    (0x1100, 0x11FF, "Hangul"),
    (0x3130, 0x318F, "Hangul"),
    (0xA960, 0xA97F, "Hangul"),
    (0xAC00, 0xD7FF, "Hangul"),
    // Bopomofo is Mandarin phonetic notation and has no Japanese use at all.
    (0x3100, 0x312F, "Bopomofo"),
    (0x31A0, 0x31BF, "Bopomofo"),
];

/// Simplified-Chinese ideographs, by radical run.
///
/// Each range ends one codepoint before the first Japanese kanji that follows it, which is why the bounds look arbitrary: U+8C37 谷, U+8D64 赤, U+8F9B 辛, U+961C 阜, U+7F36 缶, U+9577 長, U+9996 首, U+9AA8 骨, U+9E75 鹵, U+89D2 角, U+98A8 風, U+98DB 飛 and U+9F8D 龍 are all in daily Japanese use and all sit directly above one of these runs.
const SIMPLIFIED_RANGES: &[(u32, u32)] = &[
    (0x8BA0, 0x8C36), // 讠 speech
    (0x8D1D, 0x8D63), // 贝 shell
    (0x8F66, 0x8F9A), // 车 cart
    (0x95E8, 0x961B), // 门 gate
    (0x7EA0, 0x7F35), // 纟 silk
    (0x9485, 0x9576), // 钅 metal
    (0x9963, 0x9995), // 饣 food
    (0x9A6C, 0x9AA7), // 马 horse
    (0x9C7C, 0x9CE2), // 鱼 fish
    (0x9E1F, 0x9E74), // 鸟 bird
    (0x89C1, 0x89D1), // 见 see
    (0x9875, 0x98A7), // 页 page
    (0x98CE, 0x98DA), // 风 wind
    (0x9F7F, 0x9F8C), // 齿 tooth
];

/// The frequent simplified forms that fall outside those runs.
///
/// Every character here was checked against Japanese usage individually, and the near-misses are the interesting part: 声 壮 双 属 恋 弥 灯 炉 状 独 猫 猪 献 楼 欧 枢 湾 湿 滞 称 窃 胆 担 芦 茎 莱 蚕 蛮 随 誉 励 厨 惨 碍 礼 耻 痒 and 蚕 all *look* like simplified characters and are all perfectly ordinary Japanese — they are shinjitai, which China and Japan simplified the same way.
/// None of them are listed below.
const SIMPLIFIED_EXTRA: &str = "\
这过还进远违连迟运迈达边辽迁选逊递逻遗适\
个们为么义习书买产亲亿从仅仓价众优伟传伤伦伪侧债储\
关兴养兽农军凤凭击划则刚创删别劝办务动劳势华协单卫厂厅历压厌县变发叹吗员响哑\
团园围图圆圣场坏块坚坛垄垒垫墙壳处备复够头夹夺奋奖妆妇妈娄婴孙实宠审宪宽宾对寻导尘尝层\
岁岂岗岛币帅师帐带帮应库开张弹归录彻忆怀态总恳恶悬惊惭愤懒战\
扑执扩扫扬扰抚抢护拟择挡挤挥损换掷揽摄摆摊敌敛斋时显晓暂权杨极构枪标栋栏树样桥检欢歼毁毕毙\
汉汤沟沦沧泼洁浊测济浏浑浓涛润涨涩渐渔渗溃滚滤滥滨滩澜灭灵灿烂烛烦烧热焕爱爷牵牺狮狱猎\
玛环现玺珑琐电畅疗疟疮疯癞皑盏监盘睁瞒矫矿码砖础硕碱离种积稳穷窍窜窝窥竖竞笔笼筛筹签简篮类粪粮紧\
网罗罚罢羁翘聂聋职聪肃肠肤肿胀胁脏脑脓脸腻舰舱艰艳艺节苍苏荐荚荣药莲获莹莺萝萤营萧萨蓝蔷\
虏虑虾蚀蚁蚂蜗蝇衅补衬袄袜袭邓郑酝释阳阴阵阶际陆陇陈陕陨险隐隶难雏雾韦韧韩飞龙龚龛龟";

/// Characters that occupy no visual space, or reorder what surrounds them.
const INVISIBLE: &[(u32, u32, &str)] = &[
    (0x200B, 0x200F, "zero-width / bidi mark"),
    (0x202A, 0x202E, "bidi override"),
    (0x2060, 0x2064, "invisible operator"),
    (0x2066, 0x2069, "bidi isolate"),
    (0xFEFF, 0xFEFF, "byte-order mark"),
    (0x00AD, 0x00AD, "soft hyphen"),
];

#[derive(Debug, PartialEq, Eq)]
pub struct Hit {
    pub line: usize,
    pub col: usize,
    pub ch: char,
    pub kind: &'static str,
}

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let n = c as u32;
    ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&n))
}

fn blocked_script(c: char) -> Option<&'static str> {
    let n = c as u32;
    BLOCKED
        .iter()
        .find(|&&(lo, hi, _)| (lo..=hi).contains(&n))
        .map(|&(_, _, name)| name)
}

fn invisible(c: char) -> Option<&'static str> {
    let n = c as u32;
    INVISIBLE
        .iter()
        .find(|&&(lo, hi, _)| (lo..=hi).contains(&n))
        .map(|&(_, _, name)| name)
}

fn simplified(c: char) -> bool {
    in_ranges(c, SIMPLIFIED_RANGES) || SIMPLIFIED_EXTRA.contains(c)
}

const fn greek(c: char) -> bool {
    let n = c as u32;
    (0x0370 <= n && n <= 0x03FF) || (0x1F00 <= n && n <= 0x1FFF)
}

/// How many consecutive Greek letters stop being notation and start being a word.
const GREEK_RUN: usize = 3;

/// Which rule set to apply.
///
/// The two surfaces are deliberately not judged the same way, because the data says they are not the same thing.
///
/// *Commit messages* are prose this machine writes, and across 35,834 lines of real history they are English: the only non-ASCII in them is typography and mathematical notation — em dashes, §, …
/// , →, ×, Θ(n), µs, none of which is a letter in any script.
/// So the message rule is stated as a language rule and nothing else: a character that is a *letter* outside Latin does not belong.
/// Punctuation, symbols, box drawing and emoji are none of its business.
///
/// *File content* is not prose.
/// A unicode test fixture, an i18n test case and a Japanese typesetting library all legitimately contain Cyrillic, Arabic and kanji — they do here, in six repositories — and a rule that refused them would be a rule that gets switched off.
/// Content gets a contamination filter instead: the scripts nobody here writes, the simplified forms that mean a sentence came from the wrong language, and the invisible characters that are never intentional anywhere.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Commit messages: Latin letters only.
    English,
    /// Commit messages in a repository whose subject is Japanese.
    Japanese,
    /// Staged file content.
    Content,
}

impl Mode {
    /// Parse a `guard.lang` value.
    /// Unset means English; `off` means no gate.
    pub fn parse(value: Option<&str>) -> Option<Self> {
        match value.map(str::trim) {
            Some("japanese" | "ja" | "jp") => Some(Self::Japanese),
            Some("off" | "false" | "0") => None,
            // Unset, empty, "english", and anything unrecognised.
            // A typo in this setting must not quietly mean "no checking".
            _ => Some(Self::English),
        }
    }
}

/// Latin: ASCII letters, the Latin-1 accents, Latin Extended-A/B and Additional.
/// `µ` and the ordinal indicators are alphabetic and sit below the Latin-1 letter block, so they are named rather than ranged.
const fn latin(c: char) -> bool {
    let n = c as u32;
    c.is_ascii_alphabetic()
        || n == 0x00B5 // µ MICRO SIGN, as in µs
        || n == 0x00AA // ª
        || n == 0x00BA // º
        || (0x00C0 <= n && n <= 0x024F)
        || (0x1E00 <= n && n <= 0x1EFF)
}

/// Kana, kanji, and the punctuation and fullwidth forms that come with them.
const fn japanese(c: char) -> bool {
    let n = c as u32;
    (0x3000 <= n && n <= 0x303F) // `、。「」〜` and the iteration marks
        || (0x3040 <= n && n <= 0x30FF) // hiragana, katakana
        || (0x31F0 <= n && n <= 0x31FF) // katakana phonetic extensions
        || (0x3400 <= n && n <= 0x9FFF) // CJK ideographs
        || (0xF900 <= n && n <= 0xFAFF) // compatibility ideographs
        || (0xFF00 <= n && n <= 0xFFEF) // halfwidth and fullwidth forms
}

/// A name for what was found, for the refusal message.
fn script_of(c: char) -> &'static str {
    if let Some(name) = blocked_script(c) {
        return name;
    }
    match c as u32 {
        0x3040..=0x309F => "Hiragana",
        0x30A0..=0x30FF | 0x31F0..=0x31FF => "Katakana",
        0x3400..=0x9FFF | 0xF900..=0xFAFF => "Han",
        0x0370..=0x03FF | 0x1F00..=0x1FFF => "Greek",
        _ => "non-Latin letter",
    }
}

/// Every offending character in `text`, in reading order.
pub fn scan(text: &str, mode: Mode) -> Vec<Hit> {
    let mut hits = Vec::new();

    for (line_no, line) in text.lines().enumerate() {
        let line_no = line_no + 1;
        let mut greek_run: Option<(usize, char)> = None;
        let mut run_len = 0usize;

        let flush = |run_len: usize, start: Option<(usize, char)>, hits: &mut Vec<Hit>| {
            if run_len >= GREEK_RUN
                && let Some((col, ch)) = start
            {
                hits.push(Hit {
                    line: line_no,
                    col,
                    ch,
                    kind: "Greek",
                });
            }
        };

        for (col, c) in line.chars().enumerate() {
            let col = col + 1;

            if greek(c) {
                if greek_run.is_none() {
                    greek_run = Some((col, c));
                }
                run_len += 1;
                continue;
            }
            flush(run_len, greek_run, &mut hits);
            greek_run = None;
            run_len = 0;

            // Never acceptable in either surface: text a reviewer cannot see.
            if let Some(kind) = invisible(c) {
                hits.push(Hit {
                    line: line_no,
                    col,
                    ch: c,
                    kind,
                });
                continue;
            }
            // Never acceptable in either surface: a form Japanese does not have, which means the sentence around it came from the wrong language.
            if simplified(c) {
                hits.push(Hit {
                    line: line_no,
                    col,
                    ch: c,
                    kind: "simplified Chinese",
                });
                continue;
            }

            let offends = match mode {
                Mode::English => c.is_alphabetic() && !latin(c),
                Mode::Japanese => c.is_alphabetic() && !latin(c) && !japanese(c),
                Mode::Content => blocked_script(c).is_some(),
            };
            if offends {
                hits.push(Hit {
                    line: line_no,
                    col,
                    ch: c,
                    kind: script_of(c),
                });
            }
        }
        flush(run_len, greek_run, &mut hits);
    }

    hits
}

/// The commit message as the author wrote it: git's own `#` commentary and the `--verbose` diff below the scissors line are not ours to judge, and the diff in particular would re-flag content this gate already let through once.
pub fn commit_message_body(raw: &str, comment_char: char) -> String {
    raw.lines()
        .take_while(|l| !l.starts_with(&format!("{comment_char} ------------------------ >8 ")))
        .filter(|l| !l.starts_with(comment_char))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render hits for a human, one line each, with the source line beneath.
pub fn report(label: &str, text: &str, hits: &[Hit]) {
    let lines: Vec<&str> = text.lines().collect();
    let mut last_line = 0;
    for h in hits {
        eprintln!(
            "  {label}:{}:{}  {:?}  U+{:04X}  [{}]",
            h.line, h.col, h.ch, h.ch as u32, h.kind
        );
        if h.line != last_line
            && let Some(src) = lines.get(h.line - 1)
        {
            let shown: String = src.chars().take(160).collect();
            eprintln!("      | {shown}");
            last_line = h.line;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, commit_message_body, scan};

    fn kinds(s: &str, mode: Mode) -> Vec<&'static str> {
        scan(s, mode).into_iter().map(|h| h.kind).collect()
    }
    const NONE: Vec<&'static str> = Vec::new();

    /// Every non-ASCII character that appears in 35,834 lines of this machine's real commit messages, other than the agent attribution the stripper removes.
    /// None of them is a letter, so none of them is this gate's business — which is the whole argument for keying on letters.
    #[test]
    fn real_english_commit_messages_pass() {
        for s in [
            "feat: verify the signatures a forge writes, not just our own",
            "refactor(core): split the resolver — it was doing two jobs",
            "docs: note §4.2 and the Θ(n log n) bound",
            "perf: 120µs → 8µs on the hot path (×15)",
            "fix: handle ‘smart quotes’ and “doubled” ones, …ellipses too",
            "chore: François and Gödel are contributors, not typos",
            "test: ①②③ and ✓ ✗ ⚠ · ‥ ± ≦ ⊂ ′ ‖ ¶ ¥ € ° © ® ™ ─│┌┐└┘",
            "feat: ship it 🎉",
        ] {
            assert_eq!(kinds(s, Mode::English), NONE, "false positive: {s}");
        }
    }

    #[test]
    fn english_mode_refuses_every_other_script() {
        assert!(kinds("fix: 設定を読み込む", Mode::English).contains(&"Han"));
        assert!(kinds("fix: せっていをよみこむ", Mode::English).contains(&"Hiragana"));
        assert!(kinds("fix: コミット", Mode::English).contains(&"Katakana"));
        assert!(kinds("커밋 메시지", Mode::English).contains(&"Hangul"));
        assert!(kinds("Исправлено", Mode::English).contains(&"Cyrillic"));
        assert!(kinds("שלום", Mode::English).contains(&"Hebrew"));
        assert!(kinds("مرحبا", Mode::English).contains(&"Arabic"));
        assert!(kinds("สวัสดี", Mode::English).contains(&"Thai"));
        // The gap that only closes in English mode: Chinese written entirely in characters Japan also uses is undetectable as *Chinese*, but it is trivially detectable as "not Latin".
        assert!(kinds("添加新的功能", Mode::English).contains(&"Han"));
    }

    #[test]
    fn japanese_mode_allows_japanese_but_still_refuses_chinese() {
        for s in [
            "feat: 縦組みのルビ配置を JIS X 4051 に合わせる",
            "fix: 全角・半角の混在（「」〜）を正規化する",
        ] {
            assert_eq!(kinds(s, Mode::Japanese), NONE, "false positive: {s}");
        }
        for s in [
            "这个提交信息是中文的",
            "请检查配置文件",
            "增加对新协议的支持",
            "删除无用的调试输出",
            "优化性能并减少内存占用",
        ] {
            assert!(
                kinds(s, Mode::Japanese).contains(&"simplified Chinese"),
                "missed: {s}"
            );
        }
        assert!(kinds("커밋", Mode::Japanese).contains(&"Hangul"));
    }

    /// The kanji China and Japan simplified the same way.
    /// Every one of these is ordinary Japanese and every one of them looks like it should not be.
    #[test]
    fn shinjitai_are_not_mistaken_for_simplified_chinese() {
        let shared = "声 壮 双 属 恋 弥 灯 炉 状 独 猫 猪 献 楼 欧 枢 湾 湿 滞 称 窃 胆 担 芦 茎                       莱 蚕 蛮 随 誉 励 厨 惨 碍 礼 耻 痒 谷 赤 辛 阜 缶 長 首 骨 鹵 角 風 飛 龍";
        assert_eq!(kinds(shared, Mode::Japanese), NONE);
        assert_eq!(kinds(shared, Mode::Content), NONE);
    }

    #[test]
    fn content_mode_is_a_contamination_filter_not_a_language_rule() {
        // Fixtures and a Japanese-typesetting crate are legitimate content.
        assert_eq!(
            kinds("const SAMPLE: &str = \"日本語のテスト\";", Mode::Content),
            NONE
        );
        assert_eq!(
            kinds("let x = 1; // ordinary English comment", Mode::Content),
            NONE
        );
        // Scripts nobody here writes are not.
        assert!(kinds("// Исправлено", Mode::Content).contains(&"Cyrillic"));
        assert!(kinds("// 这个", Mode::Content).contains(&"simplified Chinese"));
    }

    #[test]
    fn a_single_cyrillic_homoglyph_is_caught_in_every_mode() {
        // 'с' here is U+0441, not the Latin 'c'.
        for mode in [Mode::English, Mode::Japanese, Mode::Content] {
            let hits = scan("let sourсe = 1;", mode);
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].kind, "Cyrillic");
        }
    }

    #[test]
    fn invisible_characters_are_caught_in_every_mode() {
        for mode in [Mode::English, Mode::Japanese, Mode::Content] {
            assert_eq!(
                kinds("if (x)\u{202E} // comment", mode),
                vec!["bidi override"]
            );
            assert_eq!(kinds("a\u{200B}b", mode), vec!["zero-width / bidi mark"]);
        }
    }

    #[test]
    fn isolated_greek_is_notation_but_a_word_is_not() {
        assert_eq!(
            kinds("timeout: 500µs, α=0.5, Δt, Θ(n)", Mode::English),
            NONE
        );
        assert_eq!(kinds("Ελληνικά", Mode::English), vec!["Greek"]);
        assert_eq!(kinds("αβγ", Mode::Content), vec!["Greek"]);
    }

    #[test]
    fn git_commentary_and_the_verbose_diff_are_excluded() {
        let raw = "feat: implement it\n\n# Please enter the commit message\n\
                   # ------------------------ >8 ------------------------\n\
                   diff --git a/x b/x\n+这个\n";
        let body = commit_message_body(raw, '#');
        assert_eq!(body, "feat: implement it\n");
        assert_eq!(scan(&body, Mode::English).len(), 0);
    }

    #[test]
    fn mode_parsing_never_fails_open_by_accident() {
        assert!(matches!(Mode::parse(None), Some(Mode::English)));
        assert!(matches!(
            Mode::parse(Some("japanese")),
            Some(Mode::Japanese)
        ));
        assert!(matches!(Mode::parse(Some(" ja\n")), Some(Mode::Japanese)));
        assert!(Mode::parse(Some("off")).is_none());
        // A typo must not silently disable the gate.
        assert!(matches!(Mode::parse(Some("englsh")), Some(Mode::English)));
    }
}
