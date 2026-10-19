// The manifest assertions, shared by the symbolic model and the concrete exported relations.
module manifest_checks[Profile, Path, Target, Retired, Hook]

pred tree[parent: Path -> Path] {
  no p: Path | p in p.^parent
}

pred unique[path: Target -> Path] {
  path in Target lone -> one Path
}

fun managed[p: Profile, receives: Target -> Profile, path: Target -> Path, parent: Path -> Path]: set Path {
  (receives.p).path.*parent
}

fun directories[path: Target -> Path, parent: Path -> Path]: set Target {
  { t: Target | some other: Target - t | t.path in other.path.^parent }
}

pred disjoint[receives: Target -> Profile, path: Target -> Path, parent: Path -> Path, removes: Retired -> Profile, at: Retired -> Path] {
  all p: Profile, r: removes.p | r.at not in managed[p, receives, path, parent]
}

pred noEmptyDirectory[receives: Target -> Profile, path: Target -> Path, parent: Path -> Path] {
  all p: Profile, d: directories[path, parent] & receives.p |
    some leaf: (Target - directories[path, parent]) & receives.p | d.path in leaf.path.^parent
}

pred hookBijection[file: Hook -> Target] {
  file in Hook lone -> one Target
}

pred uniformHookMask[file: Hook -> Target, receives: Target -> Profile] {
  all h: Hook | h.file.receives = Profile
}
