use rules::figure_migration_rules::{Evidence, Mode, ScopeMatch, may_retire, permits_retirement};
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::path::Path;

pub const TOKEN: &str = "DOTGUARD_FIGURE_RETIREMENT";
pub const FILE: &str = "dotguard-figure-retirement.json";
const MAX_PERMIT_BYTES: u64 = 4096;
const OBJECT_ID_DIGITS: usize = 40;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Branch {
    Diagrams,
    Screenshots,
}

impl Branch {
    fn reference(self) -> &'static str {
        match self {
            Self::Diagrams => "refs/heads/diagrams",
            Self::Screenshots => "refs/heads/screenshots",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Permit {
    token: String,
    remote: String,
    branch: Branch,
    oid: String,
}

impl Permit {
    pub fn verified(
        remote: &str,
        branch: &str,
        oid: &str,
        token: &str,
        evidence: Evidence,
    ) -> Option<Self> {
        if !may_retire(Mode::Apply, evidence) {
            return None;
        }
        let branch = match branch {
            "diagrams" => Branch::Diagrams,
            "screenshots" => Branch::Screenshots,
            _ => return None,
        };
        let permit = Self {
            token: token.into(),
            remote: remote.into(),
            branch,
            oid: oid.into(),
        };
        permit.valid().then_some(permit)
    }

    fn valid(&self) -> bool {
        !self.token.is_empty()
            && !self.remote.is_empty()
            && !self.remote.starts_with('-')
            && self.oid.len() == OBJECT_ID_DIGITS
            && self.oid.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    pub fn arguments(&self) -> [String; 4] {
        let reference = self.branch.reference();
        [
            "push".into(),
            format!("--force-with-lease={reference}:{}", self.oid),
            self.remote.clone(),
            format!(":{reference}"),
        ]
    }

    fn matches(&self, token: &str, request: bool) -> bool {
        let matched = |value| {
            if value {
                ScopeMatch::Exact
            } else {
                ScopeMatch::Different
            }
        };
        permits_retirement(
            matched(token == self.token),
            matched(self.valid() && request),
        )
    }
}

fn context() -> Option<(Permit, String)> {
    let token = std::env::var(TOKEN).ok()?;
    let directory = crate::real_git::capture(&["rev-parse", "--git-common-dir"])?;
    let mut bytes = Vec::new();
    std::fs::File::open(Path::new(directory.trim()).join(FILE))
        .ok()?
        .take(MAX_PERMIT_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).ok()? > MAX_PERMIT_BYTES {
        return None;
    }
    let permit = serde_json::from_slice(&bytes).ok()?;
    Some((permit, token))
}

pub fn command_permitted(arguments: &[String]) -> bool {
    let Some((permit, token)) = context() else {
        return false;
    };
    permit.matches(&token, arguments == permit.arguments())
}

pub fn update_permitted(remote: &str, input: &str) -> bool {
    let Some((permit, token)) = context() else {
        return false;
    };
    let zeros = "0".repeat(OBJECT_ID_DIGITS);
    let fields: Vec<_> = input.split_whitespace().collect();
    let expected = [
        "(delete)",
        zeros.as_str(),
        permit.branch.reference(),
        permit.oid.as_str(),
    ];
    permit.matches(&token, remote == permit.remote && fields == expected)
}
