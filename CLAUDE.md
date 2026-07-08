# AGENTS.md - Project Context for Coding Agents

## Development

- Before start working, refresh your knowledge from contents in `docs/` first.
- Always write unit tests for integration testing and functional testing of new features.
- Always test your code after your implementation.
- Never commit changes or create PRs unless requested by the human developer.
  - Suggest commit messages to the human developer for review after your implementation.
- Before submitting a PR, bump the patch version in `Cargo.toml` and add an entry to `CHANGELOG.md`.
- After document our now features in `docs/`.
- When a bug is reported, first create test cases to reproduce the bug and document the bug in `plans/`.
  Then draft the plan to fix the bug in `plans/`, and implement the fix after the plan is approved by the human developer.
- Use `plans/` for planning out your work.
  - When adding a new feature, ALWAYS first create a plan in `plans/` and ask for review from the human developer before implementation.
  - Always write down your plans and reasoning for future reference when encountering major tasks, like adding a feature.
  - Always include the problem background, the proposed solution, and the implementation steps in your plan.
  - Commit the plan to the repo and ask for review from the human developer before implementation.
  - After the plan is fully implemented, rewrite it as a design doc in `docs/`, and remove it from `plans/`.

### Building

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo run -- list        # Run directly
```

### Testing locally

```bash
cargo run -- tap list
cargo run -- tap add owner/repo --branch dev
cargo run -- list
cargo run -- install EYH0602/skillshub/using-skillshub
cargo run -- link
cargo run -- agents
cargo run -- external list
cargo run -- external scan
cargo run -- doctor
cargo run -- completions bash
```

## Versioning

The project follows [Semantic Versioning](https://semver.org/) (`MAJOR.MINOR.PATCH`).
The version is stored in `pyproject.toml` under `[project] version`.

- **Patch version** (`0.2.x` → `0.2.x+1`): bump for every PR. Each PR is squash-merged into the release branch.
- **Minor version** (`0.x` → `0.x+1`): bump when cutting a release (e.g. alpha → beta, beta → stable). Release branches are merged with a merge commit.
- **Major version**: reserved for breaking changes to public interfaces.

### Rules

- Every PR **must** bump the patch version in `pyproject.toml` before merging.
- Every PR **must** add an entry to `CHANGELOG.md` under the `## [Unreleased]` section.
- When merging a release branch (e.g. `release/beta` → `main`), bump the minor version and move `[Unreleased]` entries under a versioned heading.
- The current release track is `0.2.x` (beta).

### CHANGELOG format

Follow [Keep a Changelog](https://keepachangelog.com/). Group entries under:
`Added`, `Changed`, `Fixed`, `Removed`.

```markdown
## [Unreleased]

### Added
- Short description of what was added (#PR)

### Fixed
- Short description of what was fixed (#PR)
```

## Code Review

- When asked to review PR on this repo, first check the related issue and the PR description to understand the context and the purpose of the changes.
- Always directly comment, request changes, or approve the PR on GitHub.
- Only reply in the PR comment thread when explaining design and after fixing requested changes after code rev
