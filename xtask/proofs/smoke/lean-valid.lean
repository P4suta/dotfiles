theorem verification_smoke (value : Bool) : value || !value = true := by
  cases value <;> rfl

#print axioms verification_smoke
