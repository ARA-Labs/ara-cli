---
name: collective-frontier-cli
description: >
  Independently installable frontier context component for one-host collective ARA
  research. Trigger when the immutable collective run configuration enables frontier;
  preserves common roles, failure policy and the same total allocation across all conditions.
argument-hint: "[pinned run configuration]"
allowed-tools: Read, Bash(ara *|python *)
metadata:
  author: ara-commons
  version: "1.0.0"
  tags: [collective-research, frontier]
---

# Independent frontier component

This install surface is separate from interface-only CLI skills and from the other component.
Read directly, in this order:

- `references/roles.md` — exact common authority page used by every collective control.
- `references/failure-policy.md` — exact common budget/failure page used by every control.
- `references/frontier.md` — only this selected component's additional context/protocol.

Run each `ara` command as its own shell call with quoted arguments (quote any argument
containing spaces or `#`, such as `'logic/claims.md#C04'`) and no pipes, redirects, `&&`, `;` or
globs. Some harnesses reject composed commands; other shells may allow them, but the single-call
form is the portable recipe. Bound output with `ara`'s own `show --lines`, `show --max-bytes`,
`find --limit`, `find --context` and `ls <path>`, and follow a printed `next: --lines X:` window
instead of reading harness logs.

Do not load the other component unless separately installed and selected by the immutable run
configuration. Both components can compose, deduplicating common pages by their exact digest
but recording/counting every supplied page. The plain Files and plain CLI collective controls
must receive the same common roles/failure pages. Noncollective interface-only copies receive
none of these pages. Check the exact contract/run/component/role approval revision before a
scored run; upstream roles/protocol approval and external harness15 integration are pending.
Standalone deterministic community-smoke scenarios are transport/fidelity checks, not scored
experiments, performance equivalence, distributed deployment or a hidden extra reasoning agent.
