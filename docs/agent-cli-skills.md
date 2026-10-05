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
variants carry the same `references/cli-access.md` command reference.

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

## Changing a skill

- Edit the skill here, in the same change as the binary behavior it describes.
- Keep the `cli-access.md` copies identical until they are split into shared
  read and write references.
- A changed skill gets a new digest. Registered runs keep the digest they
  recorded, so pin the new commit in the harness for any new condition.
- Skills are not part of a published crate. A skill-only edit does not bump the
  version; a change that ships with binary behavior follows that change's bump.

`crates/ara-cli/tests/skills.rs` checks that every skill's `name:` matches its
directory, that each has a `LICENSE`, that the `cli-access.md` copies are
identical, and that every `ara <subcommand>` command line in a skill names a
subcommand the binary has.
