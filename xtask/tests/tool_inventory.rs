mod inventory {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;
    use xtask::inventory::{Installed, Inventory, NoSkill, Policy, Reason, declared, refusals};

    fn inventory(pin: &str) -> Inventory {
        let mut inventory = Inventory::new();
        inventory.insert(
            "orbstack".into(),
            Installed {
                profiles: BTreeSet::from(["mac"]),
                versions: BTreeSet::from(["latest".to_owned()]),
            },
        );
        inventory.insert(
            "mise".into(),
            Installed {
                profiles: BTreeSet::from(["mac", "linux", "windows", "wsl"]),
                versions: BTreeSet::from([pin.to_owned()]),
            },
        );
        inventory
    }

    fn policy(none: Vec<NoSkill>) -> Policy {
        Policy {
            steps: BTreeMap::new(),
            platform_specific: BTreeMap::from([("orbstack".to_owned(), vec!["mac".to_owned()])]),
            none,
        }
    }

    fn skill(tools: &str) -> BTreeMap<String, String> {
        let bindings = tools
            .split(',')
            .map(str::trim)
            .map(|pair| {
                let (tool, version) = pair.split_once('@').unwrap();
                format!("{tool:?} = {version:?}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        declared(&format!("[tools]\n{bindings}\n")).unwrap()
    }

    fn mac_only() -> BTreeSet<String> {
        BTreeSet::from(["mac".to_owned()])
    }

    #[test]
    fn removing_the_skill_of_an_inventoried_tool_fails_until_a_decision_covers_it() {
        let skills = BTreeMap::from([
            ("mise".to_owned(), skill("mise@2026.10.0")),
            ("orbstack".to_owned(), skill("orbstack@latest")),
        ]);
        let placements = BTreeMap::from([("orbstack".to_owned(), mac_only())]);
        assert_eq!(
            refusals(
                &inventory("2026.10.0"),
                &skills,
                &policy(Vec::new()),
                &placements
            ),
            Vec::<String>::new()
        );
        let without = BTreeMap::from([("mise".to_owned(), skill("mise@2026.10.0"))]);
        let refused = refusals(
            &inventory("2026.10.0"),
            &without,
            &policy(Vec::new()),
            &BTreeMap::new(),
        );
        assert!(
            refused
                .iter()
                .any(|refusal| refusal.starts_with("orbstack installs on mac with no skill")),
            "{refused:?}"
        );
        let decided = policy(vec![NoSkill {
            reason: Reason::Desktop,
            why: "An application the owner drives.".into(),
            tools: vec!["orbstack".into()],
        }]);
        assert_eq!(
            refusals(
                &inventory("2026.10.0"),
                &without,
                &decided,
                &BTreeMap::new()
            ),
            Vec::<String>::new()
        );
        let unexplained = policy(vec![NoSkill {
            reason: Reason::Desktop,
            why: " ".into(),
            tools: vec!["orbstack".into()],
        }]);
        assert_eq!(
            refusals(
                &inventory("2026.10.0"),
                &without,
                &unexplained,
                &BTreeMap::new()
            )
            .len(),
            1
        );
    }

    #[test]
    fn bumping_a_pin_marks_the_bound_skill_stale() {
        let skills = BTreeMap::from([("mise".to_owned(), skill("mise@2026.10.0"))]);
        let decided = policy(vec![NoSkill {
            reason: Reason::Desktop,
            why: "An application the owner drives.".into(),
            tools: vec!["orbstack".into()],
        }]);
        let refused = refusals(&inventory("2026.11.0"), &skills, &decided, &BTreeMap::new());
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert!(
            refused[0].contains("mise was written against mise 2026.10.0")
                && refused[0].contains("just decide mise")
        );
    }

    #[test]
    fn a_skill_for_a_platform_specific_tool_installs_only_on_that_platform() {
        let skills = BTreeMap::from([
            ("mise".to_owned(), skill("mise@2026.10.0")),
            ("orbstack".to_owned(), skill("orbstack@latest")),
        ]);
        let refused = refusals(
            &inventory("2026.10.0"),
            &skills,
            &policy(Vec::new()),
            &BTreeMap::new(),
        );
        assert!(
            refused
                .iter()
                .any(|refusal| refusal.contains("restrict .agents/skills/orbstack")),
            "{refused:?}"
        );
        let placements = BTreeMap::from([
            ("mise".to_owned(), mac_only()),
            ("orbstack".to_owned(), mac_only()),
        ]);
        let refused = refusals(
            &inventory("2026.10.0"),
            &skills,
            &policy(Vec::new()),
            &placements,
        );
        assert!(
            refused
                .iter()
                .any(|refusal| refusal.contains("mise installs on fewer than every profile")),
            "{refused:?}"
        );
    }

    /// The Mac profile installs `OrbStack`, so its skill exists, binds the tool, and installs only there.
    #[test]
    fn orbstack_has_a_mac_only_skill() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let text = std::fs::read_to_string(root.join("home/dot_agents/skills/orbstack/tools.toml"))
            .unwrap();
        assert!(declared(&text).unwrap().contains_key("orbstack"));
        let targets = std::fs::read_to_string(root.join("home/.chezmoidata/targets.toml")).unwrap();
        for target in [".agents/skills/orbstack", ".claude/skills/orbstack"] {
            assert!(
                targets.contains(&format!("\"{target}\" = \"mac\"")),
                "{target}"
            );
        }
        assert!(xtask::inventory::check(&root).unwrap() > 0);
    }
}
