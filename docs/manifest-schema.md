# Manifest JSON Schema — Geometry Wire Shape

This document describes the **geometry** fields added by `ara-core` layout
(Stage 2). These fields are **frozen** as stable; changes require a coordinated
`ara-core` + client version bump.

## Frozen geometry types

### `Point`

Center position of a node.

```json
{ "x": 205.0, "y": 30.0 }
```

| Field | Type  | Description            |
|-------|-------|------------------------|
| `x`   | `f64` | Horizontal coordinate  |
| `y`   | `f64` | Vertical coordinate    |

### `Rect`

Axis-aligned bounding rectangle.

```json
{ "x": 0.0, "y": 0.0, "width": 410.0, "height": 170.0 }
```

| Field    | Type  | Description         |
|----------|-------|---------------------|
| `x`      | `f64` | Left edge           |
| `y`      | `f64` | Top edge            |
| `width`  | `f64` | Horizontal extent   |
| `height` | `f64` | Vertical extent     |

### `Node.pos`

```json
{ "pos": { "x": 205.0, "y": 30.0 } }
```

- Type: `Option<Point>` (absent when layout has not run).
- Serialized with `skip_serializing_if = "Option::is_none"`.

### `Manifest.bounds`

```json
{ "bounds": { "x": 0.0, "y": 0.0, "width": 410.0, "height": 170.0 } }
```

- Type: `Option<Rect>` (absent when layout has not run).
- Serialized with `skip_serializing_if = "Option::is_none"`.
- Equal to the union of all node rects (center ± half node width/height).

## Coordinate conventions

- All coordinates are in abstract "px" units (no physical meaning until the
  client maps them to screen pixels).
- Values are canonicalized before serialization: rounded to 6 decimal places,
  `-0.0` normalized to `0.0`.
- Rank direction is always top-to-bottom (`TB`); `y` increases downward.

## Node sizing

Layout uses **fixed** node dimensions from `LayoutOptions` (default 180×60).
Real box sizes depend on browser text measurement; the client may relayout (same
`ara-core` wasm) if it needs exact text fit. Core's output is authoritative for
the fixed-size case.

## `Node.isolated`

```json
{ "isolated": true }
```

- Type: `bool` (raw key `isolated:` on a node; defaults to `false`).
- Marks the **root of an isolated subtree** — a branch the exploration reached
  that hangs off the main tree on its own rather than under a normal parent.
  Only the root of such a subtree carries the flag; its children inherit their
  placement from the root.
- Serialized with `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`
  so `false` (the common case) is omitted from the wire form and old manifests
  round-trip unchanged.
- Consumed by the viewer's tree-list mode to render isolated roots inside a
  dedicated "isolated subtree" box. This is a **logical** (not geometry) field,
  so it is additively extensible and needs no coordinated version bump.

## `Exhibit.image`

Figure exhibits may carry an optional `image` string, such as
`"evidence/figures/loss.png"`, relative to the artifact root. Absent images default
to `None` and are omitted during serialization, so old manifests keep their
no-image behavior. `file` remains the Markdown path for a companion pair, or the
actual raster path for an image-only exhibit; `body` remains raw Markdown.

Supported references are local PNG/JPEG filesystem names. The parser and viewer
reject URL schemes, absolute paths, parent components, backslashes, drive paths,
and control characters. Literal spaces, Unicode, `%`, `#`, and `?` are filename
characters and are encoded once per URL segment. The viewer maps the path beside
the successful static manifest or through its matching serve image route.
See [figure-exhibit-images.md](figure-exhibit-images.md) for metadata precedence,
captions, diagnostics, and complete static packaging.

## Claim and node detail fields

`Claim.falsification` is an optional string parsed from the `Falsification criteria` label in `logic/claims.md`. Bullet (`- **Falsification criteria**: text`) and bold-leading (`**Falsification criteria.** text`) forms are supported. Missing or blank values default to `None` and serialize away. Claim cards show the claim ID, criterion, raw experiment `proof` references, and claim `deps` when present. Proof references remain text because experiments have no exhibit-resolution contract.

`Node.artifacts` and `Node.concepts` are explicit CLI extensions to the exploration-tree YAML. They are author-supplied lists, with no automatic binding from mentions or global artifact indexes:

```yaml
tree:
  - id: N01
    type: experiment
    title: Compare optimizers
    artifacts:
      - name: muon_optimizer.py
        pointer: src/execution/muon_optimizer.py
        what: Orthogonalized matrix updates.
    concepts: [Muon Optimizer]
```

The normalized JSON keeps the same array shapes. Artifact entries contain default-empty string fields `name`, `pointer`, and `what`. Pointer values are escaped display text, including external pointer strings; the viewer does not fetch them or generate executable links. Empty subfields and wholly blank entries do not render.

Concept references name full `Concept.term` headings from `logic/concepts.md`, not authored IDs or generated glossary anchors. The viewer trims reference whitespace and compares full terms case-insensitively. A unique match opens that exact definition in the Glossary modal. Repeated resolved references produce one chip in first-reference order. Missing or ambiguous references stay visible as non-interactive chips. Renaming a glossary term requires updating its node references.

Both node arrays default to empty and serialize away when empty. Existing populated JSON manifests remain readable. See [claim-artifact-concept-details.md](claim-artifact-concept-details.md) for the source contract, focus behavior, and limits.

## Logical model extensibility

The **logical** model (`nodes`, `links`, `bindings`, `claims`, `NodeKind`,
`NodeFields`) remains **additively extensible**:

- New node kinds via `NodeKind::Other(String)`.
- Extra fields via the existing `extra` capture at the raw layer.
- Future `schema_version` field for dialect negotiation.

Additive extension is the only *compatible* logical change. Renaming an
existing wire key is a **logical breaking change** — not a geometry one: the
pivot body keys were renamed in `0.1.15` (`from` → `prior_direction`, `to` →
`new_direction`, `trigger` → `reason`). The migration path differs by layer:

- **YAML trees**: the old keys fall into the raw layer's `extra` capture and
  surface as unknown-field warnings. `ara check` migrates in place — ARA005–
  ARA007 rename the keys and recover the values.
- **JSON manifests**: the normalized model has no `extra` capture, so an old
  serialized manifest carrying the legacy keys deserializes with them silently
  ignored (`prior_direction`/`new_direction`/`reason` come out as `null`) and
  no warning fires. Regenerate the manifest with `ara layout` from the fixed
  YAML source instead.

The frozen-geometry contract above is unaffected.

One deliberate name collision: `reason:` is the **canonical** key on a `pivot`
node (ARA007 renames `trigger:` to it) but an **alias** of `why_failed:` on a
`dead_end` node (ARA002 renames it away). The lint rules are kind-scoped — a
key is only flagged when it sits directly on a node of the matching kind — so
the same spelling is treated correctly per node type.

Only a **geometry** change (new fields on `Point`, `Rect`, or the semantics of
`pos`/`bounds`) requires a coordinated `ara-core` + client version bump.

## Out of scope

- **Edge routing** (`Link.route`): deferred to the Stage 3 client
  (`T-EDGE-ROUTING`). Edges are drawn straight/orthogonal from node endpoints.
