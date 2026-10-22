use super::{Result, Session, decode, json, retire};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct SyncMetadata {
    project: String,
    config: String,
    slug: String,
    description: String,
    enabled: bool,
    status: String,
}

pub(super) fn run(session: &Session, workplace: &str) -> Result<()> {
    let syncs: Vec<SyncMetadata> = retire::syncs(&session.client)?
        .into_iter()
        .map(|value| decode(value, "GitHub sync metadata"))
        .collect::<Result<_>>()?;
    println!(
        "{}",
        json!({"phase":"audit","workplace":workplace,"syncs":syncs})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> serde_json::Value {
        json!({
            "project":"fixture",
            "config":"automation",
            "slug":"sync-fixture",
            "description":"Actions: owner / fixture",
            "enabled":true,
            "status":"synced"
        })
    }

    #[test]
    fn audit_output_excludes_credential_and_unknown_fields() {
        let mut input = metadata();
        input["token"] = json!("private-fixture-input");
        input["connection"] = json!({"credential":"private-fixture-input"});
        let parsed: SyncMetadata = decode(input, "fixture metadata").unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), metadata());
    }

    #[test]
    fn audit_refuses_missing_or_mistyped_metadata() {
        let mut missing = metadata();
        missing.as_object_mut().unwrap().remove("project");
        assert!(decode::<SyncMetadata>(missing, "fixture metadata").is_err());
        let mut mistyped = metadata();
        mistyped["enabled"] = json!("true");
        assert!(decode::<SyncMetadata>(mistyped, "fixture metadata").is_err());
    }
}
