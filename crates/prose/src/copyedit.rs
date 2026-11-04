//! Explicit multilingual copyediting, separate from the persisted English gate.

mod calibration;
mod content;
mod diagnostics;
mod grammar;
mod grammar_cache;
mod metadata;
mod nlp;
mod projection;
mod review;
mod style;

use anyhow::{Result, ensure};
pub use diagnostics::{Diagnostic, Severity};
pub(crate) use nlp::analyze as analyze_house;
pub use nlp::{Annotation, Entity, Lexeme, NlpReport, Span};
pub(crate) use projection::project_visible;
pub(crate) use projection::{Kind, project_body};
pub use projection::{Segment, project};
pub use review::{Document, Report, ReviewDecisions, inspect};
use serde::{Deserialize, Serialize};
pub use style::NativeStyleMessage;
pub(crate) use style::check as check_house_style;

/// Typechecks the library adapter and exercises every selected native language engine.
///
/// # Errors
///
/// Refuses unavailable tools, failed static checks, invalid annotations, or findings in the accepted controls.
pub fn verify_runtime(bundle: &crate::Bundle) -> Result<()> {
    nlp::verify_adapter(bundle)?;
    for (language, text) in [
        (Language::English, "The museum opens at noon.\n"),
        (Language::Japanese, "庭園の池の周りを歩きます。\n"),
        (Language::Chinese, "我\u{4eec}沿着庭\u{56ed}的步道散步。\n"),
    ] {
        let document = Document {
            path: format!("runtime-control-{}.md", language.code()),
            text: text.to_owned(),
        };
        let report = inspect(bundle, language, &[document])?;
        ensure!(
            report.clean(),
            "{} runtime control produced findings: {:?}",
            language.code(),
            report.diagnostics
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[value(name = "en")]
    #[serde(rename = "en")]
    English,
    #[value(name = "ja")]
    #[serde(rename = "ja")]
    Japanese,
    #[value(name = "zh")]
    #[serde(rename = "zh")]
    Chinese,
}

impl Language {
    pub(crate) const fn core(self) -> rules::copyedit_rules::Language {
        use rules::copyedit_rules::Language as Core;
        match self {
            Self::English => Core::English,
            Self::Japanese => Core::Japanese,
            Self::Chinese => Core::Chinese,
        }
    }

    pub(crate) const fn profile(self) -> rules::copyedit_rules::Profile {
        rules::copyedit_rules::profile(self.core())
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Japanese => "ja",
            Self::Chinese => "zh",
        }
    }
}
