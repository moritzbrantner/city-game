# Save-load planning validation

## Cost model

Persisted planning state references the immutable imported scenario in two ways:

- player road and zone IDs must not collide with imported scenario IDs; and
- suppressed scenario IDs must still exist in the imported scenario.

For a scenario with `N` imported entities and `M` persisted planning identifiers, bulk
save-load validation should perform one `O(N)` pass to prepare membership and `O(M)`
membership queries. It must not rescan all imported entity arrays for every planning ID.

## Ownership and lifetime

`CityScenario` remains the only authoritative imported entity collection. Save JSON and
`SAVE_SCHEMA_VERSION` are unchanged.

During `CityPlanningOverlay::validate_against_scenario`, validation builds one temporary
`HashSet<&str>` that borrows canonical IDs from the scenario. The set exists only for that
validation call and is never stored in `CitySave`, serialized, reused as simulation state, or
exposed to the browser.

One-off planning commands keep the existing direct scenario membership check. Building a full
index for a single command would trade one scan for avoidable allocation and setup.

## Deterministic regression contract

The unit regression constructs a 4,096-road scenario plus 2,048 persisted planning identifiers
(1,024 suppressed imported entities, 512 player roads, and 512 zones), serializes it through the
normal save contract, and deserializes it again.

Test-only work counters require exactly:

- 4,096 imported-entity visits to prepare membership; and
- 2,048 membership lookups for persisted planning references.

The same suite retains failure coverage for reserved imported IDs, unknown suppressed IDs,
mismatched persisted map keys, and invalid planning geometry.

These operation counts are the blocking performance contract. They are distinct from wall-clock
latency and do not change the persistence format.
