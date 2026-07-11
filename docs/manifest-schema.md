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

## Logical model extensibility

The **logical** model (`nodes`, `links`, `bindings`, `claims`, `NodeKind`,
`NodeFields`) remains **additively extensible**:

- New node kinds via `NodeKind::Other(String)`.
- Extra fields via the existing `extra` capture at the raw layer.
- Future `schema_version` field for dialect negotiation.

Only a **geometry** change (new fields on `Point`, `Rect`, or the semantics of
`pos`/`bounds`) requires a coordinated `ara-core` + client version bump.

## Out of scope

- **Edge routing** (`Link.route`): deferred to the Stage 3 client
  (`T-EDGE-ROUTING`). Edges are drawn straight/orthogonal from node endpoints.
