# Stage 1 — `ara-core` Schema + Parse + `ara validate`

**PR target:** `stage1-core-parse-validate` → `main`. **Depends on:** Stage 0.
**Version bump:** `0.0.1 → 0.0.2` (workspace `Cargo.toml` `[workspace.package] version`).

> Reviewed via `/plan-eng-review` (2026-07-08). Scope, data model, and API
> shape below are the post-review decisions. See `## GSTACK REVIEW REPORT` at
> the end. Format-level problems found during review are logged for the ARA
> maintainer in `docs/ara-format-feedback.md`.

## Problem background

Everything downstream (server, wasm client, layout) consumes one normalized
`Manifest`. If the parser drifts or the schema is loose, the whole system
inherits it. We need a tolerant, deterministic parser and a `Manifest` wire type
**provisionally frozen** (full freeze is end of Stage 2, when geometry lands —
per `stage-overview.md`) against the **official** ARA corpus before any UI work.

This stage delivers parse + normalization + binding resolution + `ara validate`
**without layout** (layout is Stage 2). Here the manifest is the logical graph
only.

### Scope decision: canonical corpus only

`bara` supports the **official** ARA format only: the
`Agent-Native-Research-Artifact/examples/` artifacts (`minimal-artifact`,
`resnet-ara-example`) and `ara-compiler` output. Hand-authored variants
(`SOULFuzz`'s `reason:`/`verifies:`, `LoongDoc`'s `provenance:`/`timestamp:`)
are **out of scope**; those dialect problems are documented upstream in
`docs/ara-format-feedback.md`. Fixtures are **copies of the official examples**,
not generated.

Canonical facts the parser is built against (verified from the two official
examples):

- Root dialect: `tree:` (list). `root:` (single) also supported defensively.
- Node types (5): `question`, `experiment`, `decision`, `dead_end`, `insight`.
- Every canonical node has `title:`. Common metadata: `support_level`
  (`explicit|inferred`), `source_refs` (list), `description`.
- Type-specific body: `result` (experiment), `why_failed` (dead_end),
  `choice`/`alternatives`/`rationale` (decision).
- Edges: `children:` (nesting) and `also_depends_on:` (cross-ref). No
  `verifies:` in canonical (SOULFuzz-only).
- Node→claim refs: `evidence:` only, a mixed list like `[C01, "Table 2"]`.
- Claims: `logic/claims.md`, Markdown, `## C01: Title` headers + `- **Key**:`
  bullets (`Statement`, `Status`, `Proof: [E01]`, `Dependencies: [C01]`, ...).
- `E##` proof refs resolve to **nothing** — no evidence registry exists yet.

## Data flow

```
                         parse_dir(&Path)   [native feature only]
                                │  reads
              ┌─────────────────┴──────────────────┐
     trace/exploration_tree.yaml           logic/claims.md (optional)
              │                                     │
              ▼                                     ▼
   ┌─────────────────────┐                ┌────────────────────┐
   │ schema.rs (raw serde)│                │ claims.rs (markdown)│
   │ RawDoc{tree|root}    │                │ `## C\d+: title`    │
   │ RawNode{...canonical │                │ + bullet keys       │
   │   fields modeled...} │                │ (lenient)           │
   │ #[flatten] extra ────┼── unknown keys │                     │
   └──────────┬──────────┘   -> warnings   └─────────┬──────────┘
              │                                       │
              ▼   parse_sources(tree:&str, claims:Option<&str>)  [pure, wasm-safe]
   ┌───────────────────────────────────────────────────────────┐
   │ parse.rs normalize()                                       │
   │  • pre-order DFS: RawNode tree -> nodes[] (SOURCE ORDER)   │
   │  • children -> Link{Child};  also_depends_on -> Link{DependsOn}
   │  • evidence: split -> [C##] bindings | "prose" evidence_notes
   │  • claims.md -> claims[] ;  resolve node->claim + claim->claim
   │  • dup ids / broken refs / CYCLE (DFS) -> errors            │
   │  • unknown fields, unresolved (claims=None) -> warnings     │
   └───────────────────────────┬───────────────────────────────┘
                               ▼
        Result<(Manifest, ParseReport), ParseReport>
        Ok((m, report)) = success + warnings ;  Err(report) = errors
                               │
                               ▼  ara-cli: validate <dir> [--json] [--strict]
              print report (human|json) ; exit 0 if no errors else 1
```

## Normalized types (`manifest.rs`)

```rust
pub struct Manifest {
    pub nodes:    Vec<Node>,      // pre-order DFS, source order preserved
    pub links:    Vec<Link>,      // node -> node
    pub bindings: Vec<Binding>,   // node -> claim (resolved)
    pub claims:   Vec<Claim>,     // claim CONTENT for the viewer
}

pub struct Node {
    pub id:            NodeId,
    pub kind:          NodeKind,
    pub label:         Option<String>,     // from `title` only; consumers fall back to id
    pub support_level: Option<String>,     // "explicit" | "inferred"
    pub source_refs:   Vec<String>,
    pub description:   Option<String>,
    pub fields:        NodeFields,          // typed per-kind body
    pub evidence_notes: Vec<String>,        // free-text evidence ("Table 2")
}

pub enum NodeKind { Question, Experiment, Decision, DeadEnd, Insight, Other(String) }

pub enum NodeFields {                       // typed per canonical kind
    Question,
    Experiment { result: Option<String> },
    Decision   { choice: Option<String>, alternatives: Vec<String>, rationale: Option<String> },
    DeadEnd    { why_failed: Option<String> },
    Insight,
    Other,                                   // unknown kind: body fields captured at raw layer
}

pub struct Link    { pub from: NodeId, pub to: NodeId, pub kind: LinkKind }
pub enum   LinkKind { Child, DependsOn }

pub struct Binding { pub node: NodeId, pub claim: ClaimId, pub role: BindingRole }
pub enum   BindingRole { Evidence }         // non_exhaustive; Verifies is out-of-scope (SOULFuzz)

pub struct Claim {
    pub id:        ClaimId,
    pub title:     String,
    pub statement: Option<String>,
    pub status:    Option<String>,
    pub proof:     Vec<String>,             // E## refs, stored raw, NOT validated
    pub deps:      Vec<ClaimId>,            // claim -> claim
}
```

All `#[derive(Serialize, Deserialize)]`. `NodeId`/`ClaimId` are newtype wrappers
over `String`, normalized (trimmed, case-sensitive, exact `^N\d+$` / `^C\d+$`).

## Implementation steps

1. **Dependencies.**
   - `ara-core` `[dependencies]`: `serde` (derive), `serde-saphyr`
     (**pin the exact published version at implementation time**, e.g.
     `=0.0.N`; verify its value type + `#[serde(flatten)]` support match this
     design before committing), `thiserror`.
   - `ara-core` `[dev-dependencies]`: `insta`, `serde_json` (snapshot the
     Manifest as JSON; `serde_json` is a **dev**-dep here, not a runtime dep).
   - Feature gate (no `notify` yet — deferred to Stage 4):
     ```toml
     [features]
     default = ["native"]
     native  = []          # gates parse_dir + std::fs; keeps parse_sources wasm-safe
     ```
   - Keep the parse path wasm-safe: no threads, filesystem, or `SystemTime` in
     `parse_sources`/normalize. `#[cfg(feature = "native")]` gates `parse_dir`.
   - Confine `serde-saphyr` types to `schema.rs`/`claims.rs`; they must **not**
     appear in the public `Manifest`/`Diagnostic` API (isolation keeps a
     future swap to `yaml-rust2` cheap — master-plan risk #2).

2. **`schema.rs` — raw serde types.** `RawDoc { tree: Option<Vec<RawNode>>,
   root: Option<RawNode>, #[serde(flatten)] extra }`. `RawNode` **models every
   canonical field explicitly** (`id, type, title, support_level, source_refs,
   description, result, why_failed, choice, alternatives, rationale, evidence,
   also_depends_on, children`) plus `#[serde(flatten)] extra: BTreeMap<String,
   saphyr::Value>`. Do **not** use `deny_unknown_fields`. **Only genuinely
   unknown keys** land in `extra` → warnings; canonical fields never warn (this
   keeps official examples clean and `--strict` meaningful). Quote/evidence text
   → owned `String`, byte-preserved, never re-emitted through a YAML serializer.

3. **`claims.rs` — Markdown claim parser (lenient).** Extract `id` + `title`
   from `^## (C\d+):\s*(.+)$` headers (canonical colon style). Claim body = text
   until the next `## ` header. Parse known bullets when present (`Statement`,
   `Status`, `Proof: [E##]`, `Dependencies: [C##]`); **missing bullets are
   tolerated**, not errors. Normalize ids (trim, case-sensitive). Duplicate
   claim id → error. Output `Vec<Claim>`.

4. **`manifest.rs`** — the normalized types above.

5. **`parse.rs`.**
   - `pub fn parse_sources(tree_yaml: &str, claims_md: Option<&str>) ->
     Result<(Manifest, ParseReport), ParseReport>` — pure, wasm-safe.
     `claims_md = None` ⇒ bindings unresolved ⇒ **warning** (not error).
   - `#[cfg(feature = "native")] pub fn parse_dir(dir: &Path) -> Result<(Manifest,
     ParseReport), ParseReport>` — reads `trace/exploration_tree.yaml` +
     `logic/claims.md` (optional), then calls `parse_sources`.
   - Normalize: pre-order DFS **preserving source order**; build `links`
     (`Child` from `children`, `DependsOn` from `also_depends_on`); split
     `evidence:` into `[C##]` bindings vs prose `evidence_notes`; resolve
     node→claim and claim→claim refs against `claims[]`; dedupe identical links
     (duplicate edge → **warning**).
   - Determinism comes from **preserving input order** (nodes = DFS order; links
     = per-node source order then ref order; bindings likewise). **No sort by
     id.**
   - **Callers must not `?`-discard the `Ok` arm's report** — success carries
     warnings that must be printed.

6. **Validation severity.**
   - **ERROR (exit 1):** broken node→claim (`evidence:[C##]` missing claim),
     broken claim→claim (`Dependencies:[C##]` missing), broken node→node
     (`also_depends_on` missing node), duplicate node id, duplicate claim id,
     **cycle** in `children`+`also_depends_on` (DFS back-edge), both `tree:` and
     `root:` present, neither present, multi-document YAML, non-mapping root.
   - **WARNING (exit 0):** unknown fields (`extra`), unresolved bindings
     (`claims_md = None`), duplicate/redundant link, `tree: []` (empty manifest).
   - **IGNORED (stored raw):** `Proof:[E##]` — no evidence registry until a
     later stage.

7. **`ParseReport`** = `{ errors: Vec<Diagnostic>, warnings: Vec<Diagnostic> }`
   with `errors()` / `warnings()` accessors and `is_ok()`. `Diagnostic {
   severity, path, message }` where **`path` is a node/field/claim path (e.g.
   `nodes[N07].evidence[0]`), NOT a source line:column** — `serde-saphyr` may not
   expose reliable spans through serde; do not promise line numbers. `Display`
   for human CLI output.

8. **`ara-cli`: `ara validate <dir> [--json] [--strict]`** (`clap`). Deps live in
   **`ara-cli`**: `clap`, `serde_json` (for `--json`); `[dev-dependencies]`
   `assert_cmd`, `predicates`, `tempfile`. Parse via `parse_dir`, print report
   (human or `--json`), `--strict` promotes warnings to errors, exit non-zero if
   any error.

9. **Fixtures** under `crates/ara-core/tests/fixtures/`:
   - `official/minimal-artifact/` and `official/resnet-ara-example/` — **copied,
     pinned** snapshots of `trace/exploration_tree.yaml` + `logic/claims.md`
     from the official repo, with a `SOURCE.md` recording origin repo, commit,
     and MPL attribution.
   - `synthetic/root_single.yaml` — a small `root:` dialect doc (unit test only).
   - `broken/` — `broken_claim_ref.yaml`, `dup_id.yaml`, `cycle.yaml`,
     `ambiguous_root.yaml` for error-path tests.

## Tests / verification (target: every codepath)

- **Dialect:** `tree:` official fixtures (snapshot); `synthetic/root_single.yaml`
  (unit) normalizes to the same `Manifest` shape as an equivalent `tree:` doc.
- **Unknown-field tolerance:** an extra key → warning, not failure; a canonical
  key → **no** warning.
- **Broken refs / dups / cycle → error, exit ≠ 0:** node→claim, claim→claim,
  node→node, duplicate node id, duplicate claim id, cycle.
- **Evidence split:** `evidence: [C01, "Table 2"]` → one binding + one
  `evidence_notes` entry.
- **parse_sources vs parse_dir:** `claims_md = None` → unresolved-binding
  warning; `parse_dir` with `claims.md` → resolved bindings, no warning.
- **Malformed YAML → panic-free `Diagnostic`** (assert no Rust panic).
- **Unknown `type:` → `NodeKind::Other`** preserved; its body fields → `extra`
  warning, never lost.
- **Claims lenient parse:** a claim missing a bullet still parses.
- **`E##` proof → no error emitted.**
- **Root-doc validation:** both `tree:`+`root:` → error; neither → error;
  `tree: []` → empty manifest + warning; multi-doc → error.
- **`insta` snapshot** of `Manifest` JSON on both official fixtures.
- **Determinism:** parse twice → byte-identical JSON across `nodes`, `links`,
  `bindings`, `claims`.
- **CLI (`assert_cmd`):** `validate <official>` exits 0; `validate <broken>`
  exits ≠ 0; `--json` emits valid JSON; `--strict` on a warn-only doc exits ≠ 0;
  missing dir / missing `exploration_tree.yaml` → clean error, not a panic.

## Milestone / acceptance

`ara validate crates/ara-core/tests/fixtures/official/minimal-artifact` and
`.../resnet-ara-example` both **exit 0 with zero warnings** (all canonical
fields modeled) and `--strict` also exits 0. Broken fixtures exit 1 with the
expected diagnostic. `cargo test --workspace` green. The `Manifest` schema is
documented and **provisionally frozen** (geometry in Stage 2 is the only planned
addition; full freeze is end of Stage 2).

## Out of scope (deferred)

- DAG layout / positions — Stage 2.
- HTTP/serve, wasm rendering — Stages 3–4.
- `notify` file-watching + the `native = ["dep:notify"]` seam — Stage 4 (Stage 1
  ships `native = []`).
- Hand-authored ARA dialects (`SOULFuzz` `reason:`/`verifies:`, `LoongDoc`
  `provenance:`) — canonical-only scope; tracked upstream in
  `docs/ara-format-feedback.md`.
- `E##` evidence-reference resolution — no registry exists; refs stored raw. A
  later "evidence stage" resolves them.
- `verifies:` binding role — SOULFuzz-only; `BindingRole` left `non_exhaustive`.
- Source-line:column diagnostics — `path` is a logical node/field path only.

## CHANGELOG (Unreleased → Added)

- `ara-core` YAML parser (`serde-saphyr`) with dual-dialect (`tree:`/`root:`)
  normalization to a `Manifest { nodes, links, bindings, claims }`, source-order
  preservation, cycle detection, Markdown claim parsing + binding resolution, and
  tolerant unknown-field capture.
- `ara validate <dir>` CLI with `--json` and `--strict`.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | not run |
| Codex Review | `/codex review` | Independent 2nd opinion | 1 | issues_found | 17 missed items; all absorbed or folded |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | clean | 10 issues, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | n/a (no UI in Stage 1) |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | not run |

**Decisions locked (13):** corpus = canonical-only (official examples;
fixtures copied, not generated) · `Manifest{nodes,links,bindings,claims}` ·
`links` node→node (Child/DependsOn), `bindings` node→claim, `evidence_notes` for
prose · `label: Option<String>` from `title` (consumer falls back to id) · typed
per-kind `NodeFields` (5 canonical kinds + `Other`), all canonical fields modeled
so official examples emit 0 warnings, unknown keys → `extra` warning · ref
severity: `C##`/node/dup/cycle = error, `E##` opaque, unresolved bindings =
warning · `parse_sources(tree, Option<claims>)` pure + `parse_dir` native ·
return `Result<(Manifest, ParseReport), ParseReport>` · `native = []` (notify →
Stage 4) · preserve source order (no sort-by-id) · cycle detection in validate ·
claims content stored in Manifest · provisional-not-frozen wording.

**CODEX:** ran (gpt-5.5, read-only, high effort). 17 missed items. Three genuine
forks accepted by the user (claim content in Manifest, source-order preservation,
cycle detection). Remaining refinements folded directly: model all canonical
fields, `parse_sources` claims arg, id normalization, logical (not line:col)
diagnostic paths, root-doc validation, exact `serde-saphyr` pin, CLI dep
placement, named-fixture acceptance, `serde_json` as ara-core dev-dep.

**CROSS-MODEL:** Eng review and Codex agreed on `native = []`, parse-API-needs-
claims, and provisional-not-frozen. No unresolved disagreement — all three Codex
tensions were put to the user and accepted.

**VERDICT:** ENG CLEARED — ready to implement. Format-level ARA problems logged
for the maintainer in `docs/ara-format-feedback.md`; two follow-ups tracked
(`T-EVIDENCE`, `T-ARA-SCHEMA`) in `TODOS.md`.

NO UNRESOLVED DECISIONS
