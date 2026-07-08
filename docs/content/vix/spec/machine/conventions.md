+++
title = "machine: conventions"
+++

Code conventions with teeth. These are spec rules, not style suggestions,
because each one's violation produced a concrete incident in the old
machine.

r[machine.conv.doc-comments]

[SETTLED] Every method has a doc comment stating what it does and why it
exists, enforced by `missing_docs` on machine modules. `alloc_raw_tainted`
with no explanation of "tainted" or "raw" is the incident class.

r[machine.conv.param-discipline]

[SETTLED] More than four parameters means an args struct or a split — the
function is doing too many jobs. (`intern_value_by_children`, seven
parameters, was the specimen.)

r[machine.conv.earned-vocabulary]

[SETTLED] Code may not wear a design's words — grammar, receipt, capability,
identity, canonical, seal — unless it implements that design's contract.
Vocabulary cosplay defeats review by satisfying its greps (`assign_roles`
called its hardcoded match a "grammar" in an error string, and reviews
passed it). Pretenders are renamed on sight, and each design's vocabulary
has one authoritative home.

r[machine.conv.wired-not-just-built]

[DESIGN] A capability that exists but is not wired is not done: the
completion criterion for any mechanism includes its consumers.
(`bind_with_lock` unused during the socket-squatting incident, jitdump
unwired while profiles showed anonymous stencils, `Access` bypassed by
offset math — one failure shape, three incidents, now a rule.)
