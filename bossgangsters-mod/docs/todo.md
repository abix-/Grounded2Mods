# bossgangsters-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | bossgangsters-mod/src/punching_bag.rs | [ ] Replace the diagnostic wait-sweep with the final auto-hit: one press per prompt at the 0.30 s wait that measured Good on every tier | A tier 2 round logs every prompt Good or better with no sweep or x2 lines, and the sweep code is gone |
| 2 | bossgangsters-mod/src/punching_bag.rs | [ ] Find a press pattern that grades Perfect on tier 2/3, or close the question | Either a log line shows Perfect on tier 2 from pressing alone, or the operator picks between keeping all-Good and extending reactionWindowTimer. Measured so far: every single-press wait 0 to 1.2 s grades Good, Miss, OffTimingHit, or Started, never Perfect; the 0.35 to 0.45 fine sweep with the 0.18 s double-tap is deployed but its results are not yet reviewed |
