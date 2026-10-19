/-!
# The skill load plan

`skill_plan_rules::plan` in `crates/rules` orders the requested skills after every skill they rely on.
This file defines the same algorithm over skill indices and proves it correct for every catalog:
a plan holds each requested skill once, after every skill it relies on, and a refusal names an unknown skill or a dependency cycle.
-/

namespace Dotfiles.SkillPlan

/-- The skills each skill relies on, each skill named by its index in the catalog. -/
abbrev Deps := Nat → List Nat

/-- Each skill of `l` comes after every skill it relies on, which lie among `pre` and the skills before it in `l`. -/
def OrderedAfter (deps : Deps) : List Nat → List Nat → Prop
  | _, [] => True
  | pre, x :: rest => (∀ d ∈ deps x, d ∈ pre) ∧ OrderedAfter deps (pre ++ [x]) rest

/-- Each skill of `l` comes after every skill it relies on. -/
def Ordered (deps : Deps) (l : List Nat) : Prop := OrderedAfter deps [] l

/-- Whether every skill that `x` relies on already lies in `placed`. -/
def ready (deps : Deps) (placed : List Nat) (x : Nat) : Bool :=
  (deps x).all fun d => decide (d ∈ placed)

/-- Places the first pending skill whose dependencies all lie in `placed`, until none remains or none can go next. -/
def order (deps : Deps) : Nat → List Nat → List Nat → Option (List Nat)
  | _, [], placed => some placed
  | 0, _ :: _, _ => none
  | fuel + 1, x :: xs, placed =>
    match (x :: xs).find? (ready deps placed) with
    | none => none
    | some y => order deps fuel ((x :: xs).erase y) (placed ++ [y])

/-- The skills of an ordered list that a requested skill needs, found from the last skill back. -/
def needed (deps : Deps) (requested : List Nat) : List Nat → List Nat
  | [] => []
  | x :: rest =>
    let later := needed deps requested rest
    if x ∈ requested ∨ later.any (fun y => decide (x ∈ deps y)) then x :: later else later

/-- Why a plan fails. -/
inductive Failure
  | unknown
  | cycle
  deriving DecidableEq, Repr

/-- Plans compare by value, so a vector checks by evaluation. -/
instance : DecidableEq (Except Failure (List Nat)) := fun left right =>
  match left, right with
  | .ok a, .ok b =>
    if same : a = b then isTrue (same ▸ rfl) else isFalse (fun equal => same (by cases equal; rfl))
  | .error a, .error b =>
    if same : a = b then isTrue (same ▸ rfl) else isFalse (fun equal => same (by cases equal; rfl))
  | .ok _, .error _ => isFalse (fun equal => by cases equal)
  | .error _, .ok _ => isFalse (fun equal => by cases equal)

/-- The plan of `requested` in a catalog of `n` skills: every skill in order, then the needed ones in that order. -/
def plan (deps : Deps) (n : Nat) (requested : List Nat) : Except Failure (List Nat) :=
  if requested.any (fun x => decide (n ≤ x)) ||
      (List.range n).any (fun x => (deps x).any fun d => decide (n ≤ d)) then
    .error .unknown
  else
    match order deps n (List.range n) [] with
    | none => .error .cycle
    | some full => .ok (needed deps requested full)

private theorem orderedAfter_snoc (deps : Deps) :
    ∀ (pre l : List Nat) (y : Nat), OrderedAfter deps pre l →
      (∀ d ∈ deps y, d ∈ pre ++ l) → OrderedAfter deps pre (l ++ [y]) := by
  intro pre l
  induction l generalizing pre with
  | nil =>
    intro y _ ready
    simp only [List.nil_append, OrderedAfter, List.append_nil] at ready ⊢
    exact ⟨ready, trivial⟩
  | cons x rest ih =>
    intro y ordered ready
    obtain ⟨first, later⟩ := ordered
    refine ⟨first, ih (pre ++ [x]) y later ?_⟩
    intro d member
    have := ready d member
    simpa [List.append_assoc] using this

/-- Each placement keeps the placed skills ordered, distinct, and made of what was placed or pending. -/
private theorem order_some (deps : Deps) :
    ∀ (fuel : Nat) (pending placed out : List Nat),
      order deps fuel pending placed = some out →
      pending.Nodup → placed.Nodup → (∀ y ∈ pending, y ∉ placed) → Ordered deps placed →
      out.Nodup ∧ Ordered deps out ∧ ∀ y, y ∈ out ↔ y ∈ placed ∨ y ∈ pending := by
  intro fuel
  induction fuel with
  | zero =>
    intro pending placed out run _ distinct _ ordered
    cases pending with
    | nil =>
      simp only [order, Option.some.injEq] at run
      subst run
      exact ⟨distinct, ordered, by simp⟩
    | cons x xs => simp [order] at run
  | succ fuel ih =>
    intro pending placed out run unique distinct apart ordered
    cases pending with
    | nil =>
      simp only [order, Option.some.injEq] at run
      subst run
      exact ⟨distinct, ordered, by simp⟩
    | cons x xs =>
      simp only [order] at run
      split at run
      · exact absurd run (by simp)
      · rename_i y found
        have member : y ∈ x :: xs := List.mem_of_find?_eq_some found
        have isReady : ready deps placed y = true := List.find?_some found
        have fresh : y ∉ placed := apart y member
        have rest := ih ((x :: xs).erase y) (placed ++ [y]) out run
          (unique.erase y)
          (List.nodup_append.mpr ⟨distinct, by simp, by
            intro a ha b hb; simp at hb; subst hb; intro same; subst same; exact fresh ha⟩)
          (by
            intro z hz
            have zin : z ∈ x :: xs := List.mem_of_mem_erase hz
            have zne : z ≠ y := by
              intro same; subst same
              exact (List.Nodup.not_mem_erase unique) hz
            simp only [List.mem_append, List.mem_singleton, not_or]
            exact ⟨apart z zin, zne⟩)
          (by
            apply orderedAfter_snoc deps [] placed y ordered
            intro d member'
            simp only [ready, List.all_eq_true, decide_eq_true_eq] at isReady
            simpa using isReady d member')
        obtain ⟨outUnique, outOrdered, outMembers⟩ := rest
        refine ⟨outUnique, outOrdered, ?_⟩
        intro z
        rw [outMembers z]
        constructor
        · rintro (inPlaced | inPending)
          · simp only [List.mem_append, List.mem_singleton] at inPlaced
            rcases inPlaced with inPlaced | same
            · exact Or.inl inPlaced
            · exact Or.inr (same ▸ member)
          · exact Or.inr (List.mem_of_mem_erase inPending)
        · rintro (inPlaced | inPending)
          · exact Or.inl (List.mem_append_left _ inPlaced)
          · by_cases same : z = y
            · exact Or.inl (by simp [same])
            · exact Or.inr ((List.mem_erase_of_ne same).mpr inPending)

/-- With enough fuel and pending skills whose dependencies stay among the pending and placed ones, a refusal leaves a nonempty set in which every skill relies on another member: a cycle. -/
private theorem order_none (deps : Deps) :
    ∀ (fuel : Nat) (pending placed : List Nat),
      pending.length ≤ fuel →
      (∀ y ∈ pending, ∀ d ∈ deps y, d ∈ pending ∨ d ∈ placed) →
      order deps fuel pending placed = none →
      ∃ cycle : List Nat, cycle ≠ [] ∧ ∀ y ∈ cycle, ∃ d ∈ deps y, d ∈ cycle := by
  intro fuel
  induction fuel with
  | zero =>
    intro pending placed short _ run
    cases pending with
    | nil => simp [order] at run
    | cons x xs => simp at short
  | succ fuel ih =>
    intro pending placed short closed run
    cases pending with
    | nil => simp [order] at run
    | cons x xs =>
      simp only [order] at run
      split at run
      · rename_i stuck
        refine ⟨x :: xs, by simp, ?_⟩
        intro y member
        have notReady : ready deps placed y = false := by
          have := List.find?_eq_none.mp stuck y member
          simpa using this
        simp only [ready, List.all_eq_false, decide_eq_true_eq] at notReady
        obtain ⟨d, dep, outside⟩ := notReady
        rcases closed y member d dep with inside | placedAlready
        · exact ⟨d, dep, inside⟩
        · exact absurd placedAlready outside
      · rename_i y found
        have member : y ∈ x :: xs := List.mem_of_find?_eq_some found
        apply ih ((x :: xs).erase y) (placed ++ [y])
        · rw [List.length_erase_of_mem member]
          simp at short ⊢
          omega
        · intro z hz d dep
          rcases closed z (List.mem_of_mem_erase hz) d dep with inside | placedAlready
          · by_cases same : d = y
            · exact Or.inr (by simp [same])
            · exact Or.inl ((List.mem_erase_of_ne same).mpr inside)
          · exact Or.inr (List.mem_append_left _ placedAlready)
        · exact run

private theorem orderedAfter_rebase (deps : Deps) :
    ∀ (l pre replacement : List Nat), OrderedAfter deps pre l →
      (∀ y ∈ l, ∀ d ∈ deps y, d ∈ pre → d ∈ replacement) →
      OrderedAfter deps replacement l := by
  intro l
  induction l with
  | nil => simp [OrderedAfter]
  | cons x rest ih =>
    intro pre replacement ordered rebase
    obtain ⟨first, later⟩ := ordered
    refine ⟨fun d dep => rebase x List.mem_cons_self d dep (first d dep), ?_⟩
    apply ih (pre ++ [x]) (replacement ++ [x]) later
    intro y member d dep before
    simp only [List.mem_append, List.mem_singleton] at before ⊢
    rcases before with before | same
    · exact Or.inl (rebase y (List.mem_cons_of_mem _ member) d dep before)
    · exact Or.inr same

private theorem needed_sublist (deps : Deps) (requested : List Nat) :
    ∀ l : List Nat, (needed deps requested l).Sublist l := by
  intro l
  induction l with
  | nil => simp [needed]
  | cons x rest ih =>
    simp only [needed]
    split
    · exact ih.cons_cons x
    · exact ih.cons x

private theorem needed_requested (deps : Deps) (requested : List Nat) :
    ∀ l : List Nat, ∀ x ∈ l, x ∈ requested → x ∈ needed deps requested l := by
  intro l
  induction l with
  | nil => simp
  | cons z rest ih =>
    intro x member asked
    simp only [needed]
    rcases List.mem_cons.mp member with same | later
    · subst same
      simp only [asked, true_or, ↓reduceIte]
      exact List.mem_cons_self
    · split
      · exact List.mem_cons_of_mem _ (ih x later asked)
      · exact ih x later asked

private theorem needed_ordered (deps : Deps) (requested : List Nat) :
    ∀ (l pre : List Nat), OrderedAfter deps pre l →
      OrderedAfter deps pre (needed deps requested l) := by
  intro l
  induction l with
  | nil => simp [needed]
  | cons x rest ih =>
    intro pre ordered
    obtain ⟨first, later⟩ := ordered
    simp only [needed]
    split
    · exact ⟨first, ih (pre ++ [x]) later⟩
    · rename_i skip
      apply orderedAfter_rebase deps _ (pre ++ [x]) pre (ih _ later)
      intro y member d dep before
      simp only [List.mem_append, List.mem_singleton] at before
      rcases before with before | same
      · exact before
      · exfalso
        apply skip
        refine Or.inr ?_
        simp only [List.any_eq_true, decide_eq_true_eq]
        exact ⟨y, member, same ▸ dep⟩

/-- An ordered list holds every skill its skills rely on. -/
private theorem orderedAfter_closed (deps : Deps) :
    ∀ (l pre : List Nat), OrderedAfter deps pre l → ∀ y ∈ l, ∀ d ∈ deps y, d ∈ pre ∨ d ∈ l := by
  intro l
  induction l with
  | nil => simp
  | cons x rest ih =>
    intro pre ordered y member d dep
    obtain ⟨first, later⟩ := ordered
    rcases List.mem_cons.mp member with same | inLater
    · subst same; exact Or.inl (first d dep)
    · rcases ih (pre ++ [x]) later y inLater d dep with before | found
      · simp only [List.mem_append, List.mem_singleton] at before
        rcases before with before | same
        · exact Or.inl before
        · exact Or.inr (same ▸ List.mem_cons_self)
      · exact Or.inr (List.mem_cons_of_mem _ found)

/-- A plan holds each requested skill once, after every skill it relies on, and every skill its skills rely on;
a refusal names an unknown skill or a cycle among the skills. -/
theorem plan_correct (deps : Deps) (n : Nat) (requested : List Nat) :
    (∀ out, plan deps n requested = .ok out →
      out.Nodup ∧ Ordered deps out ∧ (∀ x ∈ requested, x ∈ out) ∧
        ∀ y ∈ out, ∀ d ∈ deps y, d ∈ out) ∧
    (plan deps n requested = .error .unknown →
      (∃ x ∈ requested, n ≤ x) ∨ ∃ x < n, ∃ d ∈ deps x, n ≤ d) ∧
    (plan deps n requested = .error .cycle →
      ∃ cycle : List Nat, cycle ≠ [] ∧ ∀ y ∈ cycle, ∃ d ∈ deps y, d ∈ cycle) := by
  have known : ¬(requested.any (fun x => decide (n ≤ x)) ||
      (List.range n).any (fun x => (deps x).any fun d => decide (n ≤ d))) = true →
      (∀ x ∈ requested, x < n) ∧ ∀ x < n, ∀ d ∈ deps x, d < n := by
    intro clean
    simp only [Bool.or_eq_true, List.any_eq_true, decide_eq_true_eq, List.mem_range, not_or,
      not_exists, not_and, Nat.not_le] at clean
    exact ⟨clean.1, fun x below d dep => clean.2 x below d dep⟩
  have rangeClosed : (∀ x < n, ∀ d ∈ deps x, d < n) →
      ∀ y ∈ List.range n, ∀ d ∈ deps y, d ∈ List.range n ∨ d ∈ ([] : List Nat) := by
    intro bounded y member d dep
    exact Or.inl (List.mem_range.mpr (bounded y (List.mem_range.mp member) d dep))
  refine ⟨?_, ?_, ?_⟩
  · intro out run
    unfold plan at run
    split at run
    · cases run
    · rename_i clean
      obtain ⟨asked, _⟩ := known clean
      split at run
      · cases run
      · rename_i full fullRun
        cases run
        obtain ⟨fullUnique, fullOrdered, fullMembers⟩ :=
          order_some deps n (List.range n) [] full fullRun List.nodup_range List.nodup_nil
            (by simp) trivial
        have outOrdered := needed_ordered deps requested full [] fullOrdered
        refine ⟨fullUnique.sublist (needed_sublist deps requested full), outOrdered, ?_, ?_⟩
        · intro x member
          have inFull : x ∈ full :=
            (fullMembers x).mpr (Or.inr (List.mem_range.mpr (asked x member)))
          exact needed_requested deps requested full x inFull member
        · intro y member d dep
          rcases orderedAfter_closed deps _ [] outOrdered y member d dep with nothing | found
          · simp at nothing
          · exact found
  · intro run
    unfold plan at run
    split at run
    · rename_i dirty
      simp only [Bool.or_eq_true, List.any_eq_true, decide_eq_true_eq, List.mem_range] at dirty
      rcases dirty with ⟨x, member, high⟩ | ⟨x, below, d, dep, high⟩
      · exact Or.inl ⟨x, member, high⟩
      · exact Or.inr ⟨x, below, d, dep, high⟩
    · split at run
      · cases run
      · cases run
  · intro run
    unfold plan at run
    split at run
    · cases run
    · rename_i clean
      obtain ⟨_, bounded⟩ := known clean
      split at run
      · rename_i fullRun
        exact order_none deps n (List.range n) [] (by simp) (rangeClosed bounded) fullRun
      · cases run

end Dotfiles.SkillPlan
