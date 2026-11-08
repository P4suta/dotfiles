# Scope formal verification

Select verification for the actual property and the project's requirements.
A required formal claim about an implementation needs a verifier that covers the production semantics.
A mathematical claim needs a checked theorem.
Applying the theorem to code also needs a justified implementation correspondence.
External behavior needs integration or fault-injection checks for the boundary a prover doesn't establish.
Empirical claims need reproducible observations and suitable controls rather than a theorem about assumed facts.

When a verifier doesn't cover the property, try isolation, decomposition, or a suitable alternative.
Keep the conclusion within the coverage actually established.
A timeout doesn't establish an exemption, and a counterexample requires correcting the contract or implementation.
Resolve a required missing check before claiming completion.
If the required check remains blocked, ask the user for a concrete decision with its cause and proposed alternative.

Give a short reason for the completed verification.
Don't require a property matrix, signed receipt, detailed evidence file, or separate completion report unless the user explicitly requests it.
