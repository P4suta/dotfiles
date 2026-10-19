// How `just apply` captures its targets, applies, crashes, and restores, over trees of nested targets with symlinks among their ancestors.
// `crates/dotfiles/src/transaction.rs` implements it, and its crash-point test replays every crash this model reaches against a real tree.
// The counterexample of NonDurable is `a_capture_leaves_its_manifest_whole_and_no_partial_file` there, which requires the durable manifest.
module transaction

abstract sig Content {}
// What a path held before the apply, and what the apply wrote.
one sig Original, Applied extends Content {}

sig Path {
  parent: lone Path,
  // A path that is a symlink names what it points at.
  linked: lone Path,
  // What the path held before the apply, or nothing when it didn't exist.
  original: lone Content
}

fact Tree {
  no p: Path | p in p.^parent
}

// The managed targets, which may nest inside one another.
sig Target in Path {}

fact SomeTarget {
  some Target
}

// The paths at or below a target, which capture copies and restore puts back.
fun covered: set Path {
  Target.*~parent
}

// Capture refuses a target below a symlink, because the copy would follow the link out of the destination.
pred linkFree {
  no t: Target | some t.^parent.linked
}

abstract sig Phase {}
one sig Idle, Captured, Applying, Crashed, Restored extends Phase {}

// A manifest the backup holds on disk, and one still in a write buffer that a crash loses.
var lone sig Manifest {}
var lone sig Buffered {}

one sig Disk {
  var phase: one Phase,
  var holds: Path -> lone Content
}

pred init {
  Disk.phase = Idle
  Disk.holds = original
  no Manifest
  no Buffered
}

// What changes nothing in a step.
pred keep[phases: set Phase] {
  Disk.phase' = Disk.phase
  Disk.holds' = Disk.holds
  Manifest' = Manifest
  Buffered' = Buffered
  Disk.phase in phases
}

// The fixed capture copies the covered paths and writes the manifest durably before the apply starts.
pred captureDurably {
  Disk.phase = Idle
  linkFree
  Disk.phase' = Captured
  Disk.holds' = Disk.holds
  one Manifest'
  no Buffered'
}

// The old capture wrote the manifest last, without a sync, so it sat in a write buffer.
pred captureBuffered {
  Disk.phase = Idle
  linkFree
  Disk.phase' = Captured
  Disk.holds' = Disk.holds
  no Manifest'
  one Buffered'
}

// The system flushes a buffered manifest at some point.
pred flush {
  some Buffered
  no Buffered'
  one Manifest'
  Disk.phase' = Disk.phase
  Disk.holds' = Disk.holds
}

// The apply writes, replaces, or removes one covered path.
pred apply {
  Disk.phase in Captured + Applying
  some p: covered {
    (Disk.holds' = Disk.holds ++ (p -> Applied)) or (Disk.holds' = Disk.holds - (p -> Content))
    Disk.holds' != Disk.holds
  }
  Disk.phase' = Applying
  Manifest' = Manifest
  Buffered' = Buffered
}

// The process dies, and every write still in a buffer is gone.
pred crash {
  Disk.phase in Captured + Applying
  Disk.phase' = Crashed
  Disk.holds' = Disk.holds
  Manifest' = Manifest
  no Buffered'
}

// Restore reads the manifest and puts every covered path back, and running it again changes nothing.
pred restore {
  Disk.phase in Crashed + Restored
  some Manifest
  linkFree
  Disk.phase' = Restored
  Disk.holds' = Disk.holds - (covered -> Content) + (covered <: original)
  Manifest' = Manifest
  Buffered' = Buffered
}

pred fixed {
  init
  always (captureDurably or apply or crash or restore or keep[Phase])
}

pred old {
  init
  always (captureBuffered or flush or apply or crash or restore or keep[Phase])
}

// A crash that leaves a changed path always leaves a manifest to restore it from.
pred restorable {
  always ((Disk.phase = Crashed and Disk.holds != original) implies some Manifest)
}

// A finished restore holds exactly what the paths held before the apply.
pred exact {
  always (Disk.phase = Restored implies Disk.holds = original)
}

Durable: check { fixed implies (restorable and exact) } for 4 but 12 steps

Rollback: run { fixed and eventually (Disk.phase = Crashed and Disk.holds != original and eventually Disk.phase = Restored) } for 4 but 12 steps

NonDurable: check { old implies restorable } for 4 but 12 steps
