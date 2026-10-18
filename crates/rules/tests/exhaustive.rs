#![no_std]

extern crate alloc;

#[path = "../src/domain.rs"]
#[doc(hidden)]
pub mod domain;
#[path = "../src/skill_plan_rules.rs"]
pub mod exhaustive_skill_plan_rules;
#[path = "../src/source_name_rules.rs"]
pub mod exhaustive_source_name_rules;

#[test]
#[cfg_attr(miri, ignore = "Miri takes minutes over every catalog of four skills")]
fn every_catalog_of_up_to_four_skills_meets_plan_correct() {
    exhaustive_skill_plan_rules::tests::every_catalog_of_up_to_four_skills_meets_plan_correct();
}

#[test]
#[cfg_attr(miri, ignore = "Miri takes minutes over every name")]
fn every_short_name_over_the_attribute_letters_meets_the_specification() {
    exhaustive_source_name_rules::tests::every_short_name_over_the_attribute_letters_meets_the_specification();
}
