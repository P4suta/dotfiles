//! Completeness of portable uv sidecars, independent of the installing host.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Facts {
    pub pypi: bool,
    pub uv: bool,
    pub locked: bool,
    pub sidecar: Sidecar,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Sidecar {
    pub path: bool,
    pub digest: bool,
    pub directory: bool,
    pub graph: Graph,
    pub project: Project,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Graph {
    pub present: bool,
    pub checksum: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Project {
    pub present: bool,
    pub pin: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Complete,
    MissingEntry,
    MissingUv,
    MissingPath,
    MissingDigest,
    MissingDirectory,
    MissingGraph,
    DigestMismatch,
    MissingProject,
    PinMismatch,
    Unreferenced,
}

#[must_use]
pub const fn entry(facts: Facts) -> Verdict {
    if !facts.pypi && !facts.uv {
        Verdict::Complete
    } else if !facts.locked {
        Verdict::MissingEntry
    } else if !facts.uv {
        Verdict::MissingUv
    } else if !facts.sidecar.path {
        Verdict::MissingPath
    } else if !facts.sidecar.digest {
        Verdict::MissingDigest
    } else if !facts.sidecar.directory {
        Verdict::MissingDirectory
    } else if !facts.sidecar.graph.present {
        Verdict::MissingGraph
    } else if !facts.sidecar.graph.checksum {
        Verdict::DigestMismatch
    } else if !facts.sidecar.project.present {
        Verdict::MissingProject
    } else if !facts.sidecar.project.pin {
        Verdict::PinMismatch
    } else {
        Verdict::Complete
    }
}

#[must_use]
pub const fn directory(referenced: bool) -> Verdict {
    if referenced {
        Verdict::Complete
    } else {
        Verdict::Unreferenced
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn specification(facts: Facts) {
        let verdict = entry(facts);
        let required = facts.pypi || facts.uv;
        let complete = facts.locked
            && facts.uv
            && facts.sidecar.path
            && facts.sidecar.digest
            && facts.sidecar.directory
            && facts.sidecar.graph.present
            && facts.sidecar.graph.checksum
            && facts.sidecar.project.present
            && facts.sidecar.project.pin;
        assert_eq!(verdict == Verdict::Complete, !required || complete);
        assert_eq!(verdict == Verdict::MissingEntry, required && !facts.locked);
        let locked = required && facts.locked;
        assert_eq!(verdict == Verdict::MissingUv, locked && !facts.uv);
        let uv = locked && facts.uv;
        assert_eq!(verdict == Verdict::MissingPath, uv && !facts.sidecar.path);
        let path = uv && facts.sidecar.path;
        assert_eq!(
            verdict == Verdict::MissingDigest,
            path && !facts.sidecar.digest
        );
        let digest = path && facts.sidecar.digest;
        assert_eq!(
            verdict == Verdict::MissingDirectory,
            digest && !facts.sidecar.directory
        );
        let directory = digest && facts.sidecar.directory;
        assert_eq!(
            verdict == Verdict::MissingGraph,
            directory && !facts.sidecar.graph.present
        );
        let graph = directory && facts.sidecar.graph.present;
        assert_eq!(
            verdict == Verdict::DigestMismatch,
            graph && !facts.sidecar.graph.checksum
        );
        let checksum = graph && facts.sidecar.graph.checksum;
        assert_eq!(
            verdict == Verdict::MissingProject,
            checksum && !facts.sidecar.project.present
        );
        assert_eq!(
            verdict == Verdict::PinMismatch,
            checksum && facts.sidecar.project.present && !facts.sidecar.project.pin
        );
    }

    /// The ten observed flags form a bounded model of 1,024 entry states.
    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(1)]
    fn every_uv_entry_requires_all_sidecar_facts() {
        let facts: Facts = kani::any();
        specification(facts);
        let verdict = entry(facts);
        kani::cover!(verdict == Verdict::Complete && facts.pypi);
        kani::cover!(verdict == Verdict::Complete && !facts.pypi && !facts.uv);
        kani::cover!(verdict == Verdict::MissingEntry);
        kani::cover!(verdict == Verdict::MissingUv);
        kani::cover!(verdict == Verdict::MissingPath);
        kani::cover!(verdict == Verdict::MissingDigest);
        kani::cover!(verdict == Verdict::MissingDirectory);
        kani::cover!(verdict == Verdict::MissingGraph);
        kani::cover!(verdict == Verdict::DigestMismatch);
        kani::cover!(verdict == Verdict::MissingProject);
        kani::cover!(verdict == Verdict::PinMismatch);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(1)]
    fn unreferenced_directories_are_refused() {
        let referenced: bool = kani::any();
        assert_eq!(directory(referenced) == Verdict::Complete, referenced);
        assert_eq!(directory(referenced) == Verdict::Unreferenced, !referenced);
        kani::cover!(directory(referenced) == Verdict::Unreferenced);
        kani::cover!(directory(referenced) == Verdict::Complete);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_entry_states_obey_the_contract() {
        for [
            pypi,
            uv,
            locked,
            path,
            digest,
            directory,
            graph,
            checksum,
            project,
            pin,
        ] in crate::domain::flags()
        {
            proofs::specification(Facts {
                pypi,
                uv,
                locked,
                sidecar: Sidecar {
                    path,
                    digest,
                    directory,
                    graph: Graph {
                        present: graph,
                        checksum,
                    },
                    project: Project {
                        present: project,
                        pin,
                    },
                },
            });
        }
    }

    #[test]
    fn directories_require_a_lock_reference() {
        assert_eq!(directory(false), Verdict::Unreferenced);
        assert_eq!(directory(true), Verdict::Complete);
    }
}
