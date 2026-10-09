// The target manifest of `home/.chezmoidata`: which profiles receive each target, which retired targets they remove, and the Git hooks every profile receives.
// The validator facts are what `xtask/src/manifest_model.rs`, `xtask/src/profiles.rs`, and `crates/dotfiles/src/hooks.rs` refuse.
// Each assertion holds under the validators and fails without its validator, and each counterexample is a Rust test:
// Disjoint is `a_planted_over_broad_ignore_names_the_entry_and_the_profile` in xtask/tests/manifest_model.rs,
// NoEmptyDirectory is `empty_directories` in xtask/src/profiles.rs,
// HookBijection is `the_hooks_match_the_rendered_hook_files_and_the_global_configuration` in crates/dotfiles/src/hooks.rs,
// and UniformHookMask is `a_restricted_hook_names_the_entry_and_the_hook_and_the_alloy_instance_is_current` in xtask/tests/manifest_model.rs.
// `specs/manifest_instance.als`, which `just gen` writes, holds the real manifest and checks the same assertions on it.
module manifest
open manifest_checks[Profile, Path, Target, Retired, Hook] as checks

// The profiles by the names `Profile::name` gives them.
abstract sig Profile {}
one sig mac, linux, windows, wsl extends Profile {}

// The sigs are abstract so that the instance's one-sig atoms bound each of them exactly, whatever the scope.
abstract sig Path {
  parent: lone Path
}

fact Tree {
  checks/tree[parent]
}

// A target the source state declares, with the profiles that receive it.
abstract sig Target {
  path: one Path,
  receives: set Profile
}

// A retired target, with the profiles that remove it.
abstract sig Retired {
  at: one Path,
  removes: set Profile
}

// A Git hook and the rendered hook file that runs it.
abstract sig Hook {
  file: one Target
}

fact OneTargetPerPath {
  checks/unique[path]
}

// The paths a profile receives, each with every directory above it.
fun managed[p: Profile]: set Path {
  checks/managed[p, receives, path, parent]
}

// The targets that hold another target below them.
fun directories: set Target {
  checks/directories[path, parent]
}

pred disjoint {
  checks/disjoint[receives, path, parent, removes, at]
}

pred noEmptyDirectory {
  checks/noEmptyDirectory[receives, path, parent]
}

// Each hook runs from its own rendered file.
pred hookBijection {
  checks/hookBijection[file]
}

pred uniformHookMask {
  checks/uniformHookMask[file, receives]
}

// `removed_and_managed`: no retired entry a profile removes covers a target the profile receives, or a directory above one.
pred notRemovedAndManaged {
  no p: Profile, r: removes.p, t: receives.p | r.at in t.path.*parent
}

// `empty_directories`: each directory a profile receives holds some other target the profile receives.
pred noBareDirectory {
  all p: Profile, d: directories & receives.p | some t: receives.p - d | d.path in t.path.^parent
}

// The hook test: no two hooks share a rendered file.
pred distinctHookFiles {
  all disj h, g: Hook | h.file != g.file
}

// The hook test: every profile receives every rendered hook file.
pred unrestrictedHookFiles {
  all h: Hook, p: Profile | h.file in receives.p
}

// What the Rust validators refuse, as facts.
pred validators {
  notRemovedAndManaged
  noBareDirectory
  distinctHookFiles
  unrestrictedHookFiles
}

Disjoint: check { validators implies disjoint } for 4
NoEmptyDirectory: check { validators implies noEmptyDirectory } for 4
HookBijection: check { validators implies hookBijection } for 4
UniformHookMask: check { validators implies uniformHookMask } for 4

Populated: run { validators and some Retired and some directories and some Hook and some t: Target | t.receives != Profile } for 4

// Each validator dropped in turn, which leaves its assertion with a counterexample.
DisjointUnchecked: check { (noBareDirectory and distinctHookFiles and unrestrictedHookFiles) implies disjoint } for 4
NoEmptyDirectoryUnchecked: check { (notRemovedAndManaged and distinctHookFiles and unrestrictedHookFiles) implies noEmptyDirectory } for 4
HookBijectionUnchecked: check { (notRemovedAndManaged and noBareDirectory and unrestrictedHookFiles) implies hookBijection } for 4
UniformHookMaskUnchecked: check { (notRemovedAndManaged and noBareDirectory and distinctHookFiles) implies uniformHookMask } for 4
