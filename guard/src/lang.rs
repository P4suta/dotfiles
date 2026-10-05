//! Text that has no place in these repositories.
//!
//! `dotguard:allow-foreign` exempts this file, because it must contain the characters it detects.
//!
//! Three problems arrive without anyone typing them, from a search result, a model that drifts between languages, or a rendered page:
//!
//! 1. **Another writing system.** These repositories use Japanese and English, so Hangul, Cyrillic, Thai, Devanagari, and the rest count as contamination.
//!
//! 2. **Simplified Chinese.** Japanese and Chinese share most ideographs, so `日本語` reads the same in both.
//!    A Chinese sentence of any length contains simplified forms such as `这 个 说 时 门 现`, which Japanese lacks.
//!
//! 3. **Invisible characters.** Zero-width spaces, bidirectional overrides, and byte-order marks.
//!
//! Latin-1 accents pass, as in `naïve`, `Gödel`, and `São`.
//! Greek passes one letter at a time, so notation such as `λ`, `μs`, and `Σ` passes, but a run of three or more letters counts as a word.
//!

/// Writing systems with no use here.
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
    // Hangul Jamo, Compatibility Jamo, Extended-A, and syllables.
    // The range stops at U+318F, because Kanbun, the Japanese annotation marks, follows at U+3190–U+319F.
    (0x1100, 0x11FF, "Hangul"),
    (0x3130, 0x318F, "Hangul"),
    (0xA960, 0xA97F, "Hangul"),
    (0xAC00, 0xD7FF, "Hangul"),
    // Bopomofo, the Mandarin phonetic notation, has no Japanese use.
    (0x3100, 0x312F, "Bopomofo"),
    (0x31A0, 0x31BF, "Bopomofo"),
];

/// Simplified-Chinese ideographs, by radical run.
///
/// Each range ends one codepoint before a kanji in daily Japanese use.
/// Those kanji: U+8C37 `谷`, U+8D64 `赤`, U+8F9B `辛`, U+961C `阜`, U+7F36 `缶`, U+9577 `長`, U+9996 `首`, U+9AA8 `骨`, U+9E75 `鹵`, U+89D2 `角`, U+98A8 `風`, U+98DB `飛`, and U+9F8D `龍`.
const SIMPLIFIED_RANGES: &[(u32, u32)] = &[
    (0x8BA0, 0x8C36), // `讠` speech
    (0x8D1D, 0x8D63), // `贝` shell
    (0x8F66, 0x8F9A), // `车` cart
    (0x95E8, 0x961B), // `门` gate
    (0x7EA0, 0x7F35), // `纟` silk
    (0x9485, 0x9576), // `钅` metal
    (0x9963, 0x9995), // `饣` food
    (0x9A6C, 0x9AA7), // `马` horse
    (0x9C7C, 0x9CE2), // `鱼` fish
    (0x9E1F, 0x9E74), // `鸟` bird
    (0x89C1, 0x89D1), // `见` see
    (0x9875, 0x98A7), // `页` page
    (0x98CE, 0x98DA), // `风` wind
    (0x9F7F, 0x9F8C), // `齿` tooth
];

/// The frequent simplified forms outside those runs.
///
/// Shinjitai that look simplified stay off this list, because Japan simplified them the same way, and `shinjitai_are_not_mistaken_for_simplified_chinese` tests them.
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

/// The count of consecutive Greek letters that turns notation into a word.
const GREEK_RUN: usize = 3;

/// Which rule set to apply.
/// *Commit messages* on this machine use English, and their only other characters belong to typography and notation, such as `—`, `§`, `…`, `→`, `×`, `Θ(n)`, and `µs`.
///
/// *File content* includes fixtures, i18n test cases, and Japanese typesetting code with Cyrillic, Arabic, and kanji, so a language rule would only get switched off.
/// Content gets a contamination filter instead: unused scripts, simplified Chinese forms, and invisible characters.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Commit messages with Latin letters only.
    English,
    /// Commit messages in a repository about Japanese text.
    Japanese,
    /// Staged file content.
    Content,
}

impl Mode {
    /// Parse a `guard.lang` value.
    /// Unset means English, and `off` turns the gate off.
    pub fn parse(value: Option<&str>) -> Option<Self> {
        match value.map(str::trim) {
            Some("japanese" | "ja" | "jp") => Some(Self::Japanese),
            Some("off" | "false" | "0") => None,
            // Unset, empty, `english`, and unrecognized values.
            _ => Some(Self::English),
        }
    }
}

/// ASCII letters, the Latin-1 accents, and the extended Latin blocks U+0100–U+024F and U+1E00–U+1EFF.
const fn latin(c: char) -> bool {
    let n = c as u32;
    c.is_ascii_alphabetic()
        || n == 0x00B5 // `µ` micro sign, as in `µs`
        || n == 0x00AA // `ª`
        || n == 0x00BA // `º`
        || (0x00C0 <= n && n <= 0x024F)
        || (0x1E00 <= n && n <= 0x1EFF)
}

/// Kana, kanji, and the punctuation and fullwidth forms that come with them.
const fn japanese(c: char) -> bool {
    let n = c as u32;
    (0x3000 <= n && n <= 0x303F) // `、。「」〜` and the iteration marks
        || (0x3040 <= n && n <= 0x30FF) // hiragana, katakana
        || (0x31F0 <= n && n <= 0x31FF) // phonetic extensions for Ainu
        || (0x3400 <= n && n <= 0x9FFF) // CJK ideographs
        || (0xF900 <= n && n <= 0xFAFF) // compatibility ideographs
        || (0xFF00 <= n && n <= 0xFFEF) // halfwidth and fullwidth forms
}

/// The script name of a hit, for the refusal message.
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

            // Both surfaces refuse invisible text.
            if let Some(kind) = invisible(c) {
                hits.push(Hit {
                    line: line_no,
                    col,
                    ch: c,
                    kind,
                });
                continue;
            }
            // Both surfaces refuse a form that Japanese lacks, which marks the sentence as Chinese.
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

/// The commit message body without the `#` commentary of git and the `--verbose` diff below the scissors line, which would re-flag content that the content gate already passed.
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

    /// Every non-ASCII character found in the real commit messages of this machine, except the stripped attribution.
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
        // Only English mode catches Chinese written in characters that Japanese shares, because those characters fall outside Latin.
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
    #[test]
    fn shinjitai_are_not_mistaken_for_simplified_chinese() {
        let shared = "声 壮 双 属 恋 弥 灯 炉 状 独 猫 猪 献 楼 欧 枢 湾 湿 滞 称 窃 胆 担 芦 茎                       莱 蚕 蛮 随 誉 励 厨 惨 碍 礼 耻 痒 谷 赤 辛 阜 缶 長 首 骨 鹵 角 風 飛 龍";
        assert_eq!(kinds(shared, Mode::Japanese), NONE);
        assert_eq!(kinds(shared, Mode::Content), NONE);
    }

    #[test]
    fn content_mode_is_a_contamination_filter_not_a_language_rule() {
        // Fixtures and Japanese typesetting count as legitimate content.
        assert_eq!(
            kinds("const SAMPLE: &str = \"日本語のテスト\";", Mode::Content),
            NONE
        );
        assert_eq!(
            kinds("let x = 1; // ordinary English comment", Mode::Content),
            NONE
        );
        // Unused scripts fail.
        assert!(kinds("// Исправлено", Mode::Content).contains(&"Cyrillic"));
        assert!(kinds("// 这个", Mode::Content).contains(&"simplified Chinese"));
    }

    #[test]
    fn a_single_cyrillic_homoglyph_is_caught_in_every_mode() {
        // The `с` here, U+0441, differs from the Latin `c`.
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
        // A typo never turns the gate off.
        assert!(matches!(Mode::parse(Some("englsh")), Some(Mode::English)));
    }
}
