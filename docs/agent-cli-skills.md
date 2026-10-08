# CLI agent skills

The skills that tell an agent to reach ARA knowledge through `ara` live in this
repository under [`skills/`](../skills). The Files-access skills they are derived
from stay in the upstream protocol repository
([ARA-Labs/Agent-Native-Research-Artifact](https://github.com/ARA-Labs/Agent-Native-Research-Artifact)).
Command contracts are in [agent-cli.md](agent-cli.md).

## Why they live here

- **They change with the binary.** A skill names commands, flags, output fields
  and error codes. Keeping it next to the code means one change and one pin when
  either side moves.
- **Experiments see one version per condition.** Upstream holds only the Files
  skills, and this repository holds only the CLI skills. An agent that installs
  the upstream skill set no longer gets two near-identical skills with
  overlapping triggers. A harness pins the Files condition to an upstream commit
  and the CLI condition to an `ara-cli` commit.

## Layout

| Skill | Files counterpart | Purpose |
|---|---|---|
| `research-foresight-cli` | `research-foresight` | Read-only question answering over one artifact |
| `research-manager-cli` | `research-manager` | End-of-turn research recording through `ara` writes |
| `compiler-cli` | `compiler` | Compiling research input into a new artifact |
| `collective-research-cli` | — | Shared frontier and intentions across agents |
| `collective-frontier-cli` | — | Shared frontier only |
| `collective-intentions-cli` | — | Shared intentions only |

Each skill has `SKILL.md`, `references/` and an MIT `LICENSE` (the skills keep the
protocol repository's license; the rest of this repository is MPL-2.0). The three
variants carry the same `references/cli-access.md` reading reference. The two
writers, `research-manager-cli` and `compiler-cli`, also carry the same
`references/cli-write.md` write reference; the reader does not, so a read-only
session never loads JSONL transactions, guarded digests or role write
procedures.

## Provenance

The skills were imported byte-for-byte from the protocol repository at
`03f19c7767ec993ae53a0698417b8b68040d7fee` (draft PR #38), the commit the
`e1-test` run used. The import was checked three ways:

- all 21 variant files match `evaluation/agent-cli/variant-lock.json`;
- all 13 collective files match `evaluation/agent-cli/collective-contract.json`;
- `research-foresight-cli` has the `skill_digests.cli` value registered in the
  `e1-test` manifest (`ara-eval` `directory_digest`: SHA-256 over sorted relative
  paths, a NUL byte, and file bytes).

The only addition is a `LICENSE` file in each `collective-*` skill, which the
protocol repository covered with its root license.

The proof tooling that derives the variants from upstream pages
(`verify-variants.py`, `access-diff.json`, the baseline locks) stays in the
protocol repository and remains valid for that commit. It does not check later
edits made here.

The skills have since been edited here (rubric and file-access routing, then the
read interface below), so they no longer match the imported `03f19c7` bytes or
the digests in those locks. The `e1-test` run keeps the digest it registered and
its vendored inputs stay frozen; only conditions pinned to a later `ara-cli`
commit see the edited skills. The upstream Files baseline skills are unchanged.

## How the skills read

The access pages (`references/cli-access.md`, the "CLI-only access boundary"
block at the top of each page, and the collective `SKILL.md` files) teach one
recipe:

- **One shell call per `ara` command.** Quote arguments that contain spaces or
  `#` (`'logic/claims.md#C04'`); no pipes, redirects, `&&`, `;` or globs. Some
  harnesses reject composed commands; the single-call form works in all shells.
- **`ara`'s own bounds instead of shell filters:** `ls <path>`, `find --limit`,
  `find --context` (never `-C`, which selects the artifact), `show --lines` and
  `show --max-bytes`, following a printed `next: --lines X:` window rather than
  reading harness logs.
- **Orient, search, read, cite:** `ls`, then `ls <path>`; `find '<terms>'
  --context 2`; `show` an address the output printed; cite that address
  (`logic/claims.md#C04`, `trace:N01`, a canonical `#h/` heading address). On
  `unknown_id` or `ambiguous_heading`, choose from the printed candidates.
- **Fewer requests.** One `find` query carries every keyword for one
  information need (BM25 scores all its terms together); `show` takes several
  addresses (`show 'logic/claims.md#C03' E01 trace:N04`); independent `ara`
  commands go out as separate shell tool calls in the same response; and `ls`
  is skipped when the question already names what to search for.
- **Reading grounds like Files.** When a `find` hit is too terse to judge, the
  reader shows the surrounding section or windows the source document around
  the hit, then cites the printed address. Whole-section reads stay a writer
  rule, before a guarded write and in the PM and compiler procedures.
- **Two line systems.** `find` prints line numbers of the whole source
  document; `show <address> --lines` counts from the first body line of that
  entry or heading. The pages read around a hit with `show --document <source>
  --source --lines A:B`, keep address windows for numbers counted inside the
  selection (`next:`), and window a projection's named source document,
  because node, observation and session addresses reject `--lines`.
- **Brief text for reading, JSON for structured use.** `--json` stays on write,
  `apply` and `merge` commands and where a step consumes fields (`status --json`
  counts, `ls --json` typed counts, frontier records).
- **Two digest scopes before a guarded write** (`references/cli-write.md`).
  `document.replace`, `logic.revise` Body and `paper.edit` guard the heading body (heading line
  excluded) or the whole document: the skill reads the selection with
  `show --document … --heading … --source --full` and uses its printed
  `source_digest` as `expected`; a page's digest never authorizes replacing
  unseen content. `entry.rename` and `entry.remove` guard the entry span, the
  heading line plus its body, which no `show` line prints. The skill reads that
  span as a document window from the heading's `find` line `H` through `H+N`
  (`N` = the body's line count), checks that it is the heading line plus the
  body it read, and hashes the window's JSON `content` itself.

## Matching the simplified binary

Install all six live skills from the same ara-cli revision as the binary. Frozen runs and upstream locks retain their historical bytes; a new condition must pin new skill digests rather than reuse historical registration. Read modes consume `entries[].relations.path`, `entries[].relations.refs`, `ls --unfinished` `entries`, and identity `requested_address`/`resolved_target`. Task-oriented shared subsections teach research turns, initialization, staging, audited revisions and integration without changing scientific procedures or role permissions. Writer requests use scratch JSONL with literal complete text, one quoted invocation per shell call, and `ara.apply/v1` operations/bindings.

`skills.rs` includes actual subprocess coverage for a complete summarized research turn, ancestry, document citations, unfinished rows, PAPER readback and structural checking against the matching binary. The integration owner runs those checks; documentation does not claim a new model experiment or upstream approval.

## Changing a skill

- Edit the skill here, in the same change as the binary behavior it describes.
- Keep the `cli-access.md` copies identical, and the `cli-write.md` copies in
  the two writer skills identical. Reading guidance goes in `cli-access.md`;
  anything only a writer needs goes in `cli-write.md`. The two `frontier.md`
  copies and the common collective pages (`roles.md`, `failure-policy.md`, `intentions.md`) are also
  identical across the collective skills.
- A changed skill gets a new digest. Registered runs keep the digest they
  recorded, so pin the new commit in the harness for any new condition.
- Skills are not part of a published crate. A skill-only edit does not bump the
  version; a change that ships with binary behavior follows that change's bump.

`crates/ara-cli/tests/skills.rs` checks that every skill's `name:` matches its
directory, that each has a `LICENSE`, that the `cli-access.md` copies and the
writer `cli-write.md` copies are identical, that the reader has no
`cli-write.md`, that every shared task subsection is linked from a skill
page, and that every `ara <subcommand>` command line in a skill names a
subcommand the binary has. It also walks nested subcommands (`merge resolve`)
and checks that every long flag in such a command line appears in that
subcommand's `--help`, so renaming or removing a flag the skills use, such as
`--lines`, `--max-bytes`, `--context`, `--limit` or `--heading`, fails the test.
An inline command that wraps onto the next line is checked as one command.
Three more tests run command lines the access pages print verbatim. One
checks that the batched `show` prints every requested address in order.
Another runs both digest recipes on a scratch artifact and dry-runs every guarded write:
the printed heading-body digest must pass `document.replace` and `logic.revise`
Body and fail `entry.rename`/`entry.remove`, and the computed entry-span digest
must do the reverse. The third checks on the `agent-cli` fixture that a `find`
hit line selects the hit through `show --document … --source --lines`, while
the same number on the entry address fails with `line_out_of_range` and a node
address rejects `--lines` with `lines_unavailable`.
