# Stage 1 — `ara-core` Schema + Parse + `ara validate`

**PR target:** `stage1-core-parse-validate` → `main`. **Depends on:** Stage 0.
**Version bump:** `0.0.1 → 0.0.2`.

## Problem background

Everything downstream (server, wasm client, layout) consumes one normalized
`Manifest`. If the parser drifts or the schema is loose, the whole system
inherits it. The ARA source is messy: roots appear as either `tree:` (list) or
`root:` (single), and real files carry unknown/extra keys. We need a tolerant,
deterministic parser and a `Manifest` wire type frozen against a real corpus
before any UI work. `serde_yaml` is dead (archived, `0.9.34+deprecated`), so we
use `serde-saphyr` (serde-native, panic-free, YAML 1.2, DoS budgets).

This stage delivers parse + normalization + `ara validate` **without layout**
(layout is Stage 2). The manifest gains geometry in Stage 2; here it is the
logical graph only.

## Proposed solution

Implement `ara-core` modules `schema.rs`, `manifest.rs`, `parse.rs`, and wire an
`ara validate <dir>` subcommand in `ara-cli` that parses, resolves bindings,
reports unknown fields and broken references, and exits non-zero on error.
Freeze the `Manifest` types with `#[derive(Serialize, Deserialize)]` so they
cross the wire unchanged later.

## Implementation steps

1. **Dependencies (pinned exactly):** add to `ara-core`
   `serde` (derive), `serde-saphyr` (pinned `=0.0.x`), `serde_json`, `thiserror`.
   Add a feature gate:
   ```toml
   [features]
   default = ["native"]
   native = ["dep:notify"]   # notify added in Stage 4; keep the seam now
   ```
   Keep the parse path wasm-safe: no threads, no filesystem, no `SystemTime`.
2. **`schema.rs` — raw serde types** for both dialects. A `RawDoc` with
   `#[serde(default)] tree: Option<Vec<RawNode>>`, `root: Option<RawNode>`, and
   `#[serde(flatten)] extra: BTreeMap<String, saphyr Value>` at every level.
   **Do not** use `deny_unknown_fields`; collect unknowns into `extra`.
   Deserialize quote/evidence text into owned `String` (byte-preserved; never
   re-emit through a YAML serializer).
3. **`manifest.rs` — normalized types:**
   ```rust
   pub struct Manifest { pub nodes: Vec<Node>, pub links: Vec<Link>, pub bindings: Vec<Binding> }
   pub struct Node { pub id: NodeId, pub kind: NodeKind, pub title: String,
                     pub narrative: Option<String>, pub fields: /* structured */, /* … */ }
   pub enum NodeKind { Question, Experiment, Decision, DeadEnd, Pivot, Insight, Other(String) }
   pub struct Link { pub from: NodeId, pub to: NodeId, pub kind: LinkKind }
   ```
   All `Serialize + Deserialize`. `Other(String)` keeps unknown kinds lossless.
4. **`parse.rs` — `parse_str(&str) -> Result<Manifest, ParseReport>`** and a
   `native`-gated `parse_dir(&Path)`. Steps: deserialize `RawDoc` → normalize
   both dialects into `Manifest` → resolve bindings/edge refs → collect warnings
   (unknown fields) and errors (broken refs, duplicate ids) into a `ParseReport`.
   Deterministic ordering (stable sort by id) so output is byte-stable.
5. **`ParseReport`**: `{ errors: Vec<Diagnostic>, warnings: Vec<Diagnostic> }`,
   each `Diagnostic { severity, path, message }`. `Display` for CLI printing.
6. **`ara-cli`: `ara validate <dir> [--json]`** using `clap`. Parse, print the
   report (human or `--json`), exit non-zero if any error. Also add a
   `--strict` flag that promotes warnings to errors.
7. **Corpus fixtures:** commit a small real ARA (both dialects) under
   `crates/ara-core/tests/fixtures/`. If none is available, generate a
   representative one via the `ara-compiler` skill and pin it.

## Tests / verification

- Per-dialect unit tests (`tree:` and `root:`) → same `Manifest` shape.
- Unknown-field tolerance: extra keys surface as warnings, not parse failures.
- Broken-ref and duplicate-id tests → errors, non-zero exit.
- **`insta` snapshot** of `Manifest` (as JSON) on the corpus fixtures.
- **Determinism:** parse the same input twice, assert byte-identical JSON.
- CLI integration test (`assert_cmd`): `ara validate <good>` exits 0,
  `ara validate <broken>` exits non-zero; `--json` emits valid JSON.

## Milestone / acceptance

`ara validate` is green on the real corpus; the `Manifest` schema is documented
and considered provisionally frozen (geometry added in Stage 2 is the only
allowed addition).

## Out of scope (deferred)

DAG layout/positions (Stage 2); any HTTP/serve or wasm rendering.

## CHANGELOG (Unreleased → Added)

- `ara-core` YAML parser (serde-saphyr) with dual-dialect normalization to a
  `Manifest`, binding resolution, and tolerant unknown-field capture.
- `ara validate <dir>` CLI with `--json` and `--strict`.
