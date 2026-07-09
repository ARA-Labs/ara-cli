# `ara-core` — Schema, Parse & `ara validate` (Stage 1)

Design doc for the Stage-1 implementation (workspace `0.0.2`). Everything
downstream — server, wasm client, layout — consumes one normalized `Manifest`,
so this stage delivers a tolerant, deterministic parser and a provisionally
frozen wire type. Layout/geometry is **not** here (Stage 2); the manifest is the
logical graph only.

## Scope: canonical corpus only

`ara-core` targets the **official** ARA format: the two
`Agent-Native-Research-Artifact/examples/` artifacts (`minimal-artifact`,
`resnet-ara-example`) and `ara-compiler` output. Hand-authored dialects
(SOULFuzz's `reason:`/`verifies:`, LoongDoc's `provenance:`) are out of scope.
Unknown fields are captured and warned, never fatal.

The parser is robust well beyond that scope: run over the real corpora
`AmberLJC/ara-paperbench` (32 artifacts) and `ARA-Labs/ARA-Demo` (2), it parses
all 34 **without panicking**, but none parse cleanly — real artifacts use a
superset schema (extra node fields, a `pivot` type, an `ara-2.0` streams
document). Widening the model to that superset is tracked as `T-REAL-CORPUS`;
the drift is catalogued in [`ara-format-feedback.md`](ara-format-feedback.md)
§13.

## Data flow

```
                         parse_dir(&Path)   [native feature only]
                                │  reads
              ┌─────────────────┴──────────────────┐
     trace/exploration_tree.yaml           logic/claims.md (optional)
              │                                     │
              ▼                                     ▼
        schema.rs (raw serde)                 claims.rs (markdown)
        RawDoc{tree|root}                     `## C\d+: title` + bullets
        RawNode{canonical fields}             (lenient; token-scan refs)
        #[flatten] extra → unknown keys
              │                                     │
              ▼   parse_sources(tree: &str, claims: Option<&str>)  [pure, wasm-safe]
                          parse.rs normalize()
      • pre-order DFS: RawNode tree → nodes[] (SOURCE ORDER)
      • children → Link{Child};  also_depends_on → Link{DependsOn}
      • evidence: split → [C##] bindings | "prose" evidence_notes
      • claims.md → claims[];  resolve node→claim + claim→claim
      • dup ids / broken refs / CYCLE (DFS back-edge) → errors
      • unknown fields, unresolved (claims=None) → warnings
                                │
                                ▼
             Result<(Manifest, ParseReport), ParseReport>
   Ok((m, report)) = success + warnings ;  Err(report) = errors
                                │
                                ▼  ara validate <dir> [--json] [--strict]
             print report (human|json) ; exit 0 if no errors else 1
```

`serde-saphyr` (pinned `=0.0.29`) is confined to `schema.rs`/`claims.rs`; it does
not appear in the public `Manifest`/`Diagnostic` API, keeping a future
YAML-backend swap cheap. `extra` maps use `serde::de::IgnoredAny` (only unknown
key *names* are needed, for warnings).

## Normalized types (`manifest.rs`)

```rust
pub struct Manifest {
    pub nodes:    Vec<Node>,      // pre-order DFS, source order preserved
    pub links:    Vec<Link>,      // node -> node
    pub bindings: Vec<Binding>,   // node -> claim (resolved)
    pub claims:   Vec<Claim>,     // claim content for the viewer
}

pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: Option<String>,          // from `title` only; consumers fall back to id
    pub support_level: Option<String>,  // "explicit" | "inferred"
    pub source_refs: Vec<String>,
    pub description: Option<String>,
    pub fields: NodeFields,             // typed per-kind body
    pub evidence_notes: Vec<String>,    // free-text evidence ("Table 2")
}

pub enum NodeKind { Question, Experiment, Decision, DeadEnd, Insight, Other(String) }

pub enum NodeFields {
    Question,
    Experiment { result: Option<String> },
    Decision   { choice: Option<String>, alternatives: Vec<String>, rationale: Option<String> },
    DeadEnd    { why_failed: Option<String> },
    Insight,
    Other,
}

pub struct Link    { pub from: NodeId, pub to: NodeId, pub kind: LinkKind }
pub enum   LinkKind { Child, DependsOn }

pub struct Binding { pub node: NodeId, pub claim: ClaimId, pub role: BindingRole }
pub enum   BindingRole { Evidence }   // #[non_exhaustive]; Verifies is out of scope

pub struct Claim {
    pub id: ClaimId,
    pub title: String,
    pub statement: Option<String>,
    pub status: Option<String>,
    pub proof: Vec<String>,   // E## refs, stored raw, NOT validated
    pub deps: Vec<ClaimId>,   // claim -> claim
}
```

All types are `Serialize`/`Deserialize`. `NodeId`/`ClaimId` are transparent
newtypes over `String`, normalized (trimmed, case-sensitive, canonical grammar
`^N\d+$` / `^C\d+$` checkable via `is_canonical`).

Determinism is guaranteed by **preserving input order** — `nodes` are DFS order,
`links`/`bindings` follow source order, and cycle detection uses `BTreeMap` so
even error ordering is hash-seed independent. Nothing is sorted by id.

## Validation severity

- **ERROR (exit 1):** broken node→claim (`evidence:[C##]` missing), broken
  claim→claim (`Dependencies:[C##]` missing), broken node→node
  (`also_depends_on` missing), duplicate node id, duplicate claim id, **cycle**
  (`children`+`also_depends_on` back-edge), both `tree:`+`root:`, neither
  present, multi-document YAML, non-mapping root, missing node id.
- **WARNING (exit 0):** unknown fields, unresolved bindings (`claims_md = None`),
  duplicate/redundant link, `tree: []` (empty manifest), unknown `type:` body
  fields dropped, missing `type:`.
- **IGNORED (stored raw):** `Proof:[E##]` — no evidence registry exists yet
  (tracked as `T-EVIDENCE`).

`Diagnostic { severity, path, message }` uses a **logical** path
(e.g. `nodes[N07].evidence[0]`), not a source `line:column` — `serde-saphyr`
does not expose reliable spans through serde.

## CLI: `ara validate <dir> [--json] [--strict]`

Reads `trace/exploration_tree.yaml` (required) and `logic/claims.md` (optional)
via `parse_dir`, prints the report (human or `--json`), and exits non-zero on any
error. `--strict` promotes warnings to a non-zero exit. A missing directory or
missing tree file is a clean error, never a panic.

## Acceptance (met)

`ara validate` on both official fixtures exits 0 with **zero warnings** (every
canonical field is modeled), and `--strict` also exits 0. Broken fixtures exit 1
with the expected diagnostic. `cargo test --workspace` is green; `ara-core`
compiles for `wasm32-unknown-unknown` in the wasm-safe path. The `Manifest`
schema is **provisionally frozen** (geometry in Stage 2 is the only planned
addition; full freeze is end of Stage 2).

## Deferred

DAG layout (Stage 2); HTTP/serve + wasm rendering (Stages 3–4); `notify`
file-watching (Stage 4); `E##` evidence resolution (`T-EVIDENCE`); the real-
corpus schema widening and `ara-2.0` support (`T-REAL-CORPUS`); adopting an
upstream schema (`T-ARA-SCHEMA`).
