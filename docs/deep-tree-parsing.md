# Bounded deep exploration-tree parsing

`parse_sources` and the native `parse_dir` loader accept a real `children:`
hierarchy of up to 10,000 nodes along a branch. Input order remains preorder,
and normalization still produces ordinary parent-to-child links. The source
is neither flattened nor rewritten; native loads retain the exact UTF-8 input.

## Stack and resource limits

The pinned serde-saphyr backend defaults to structural depth 64 and flow depth
255. These limits are not sufficient for long research branches. A preliminary
streaming budget scan now permits structural and flow depth 20,002: each branch
node contributes a mapping and a children sequence, plus document overhead.
All other serde-saphyr budget defaults remain enabled, and input is explicitly
capped at 1 GiB. This byte cap accommodates the quadratic indentation overhead
of a 10,000-deep block-style tree without disabling input limits altogether.
Compact nested flow YAML avoids that indentation cost while representing the
same hierarchy.

Documents whose total structural depth is at most 64 retain the existing
serde-saphyr deserialization path. Deeper documents use the backend's publicly
re-exported granit-parser event API, storing scalar spans and collection edges
in a flat arena. Individual nodes are deserialized through shallow serde views
which exclude only `children`; an explicit frame stack reconstructs their
hierarchy. Normalization and raw-node destruction are also iterative.

Non-tree metadata retains a 64-level bound. Expanded tree nodes are capped at
250,000 and metadata deserialization visits at 1,000,000, including alias replay.
These additional bounds prevent a short aliased source from expanding without
limit. Invalid scalar/sequence/mapping shapes and over-limit inputs produce
`ARA100` diagnostics; node failures include the original mapping line/column.
Unknown node keys remain warnings with the original node ID, not an allowlist.

The deep parser currently rejects tagged collections rather than discarding
their meaning. YAML merge keys are expanded with explicit-key precedence and
a bounded merge chain. Ordinary bounded documents retain their existing full
tag/merge semantics. Scalar tags, scalar styles, anchors, and aliases are
retained by the arena; literal/folded strings use decoded backend values rather
than reparsing an indentation-dependent block fragment.

## Native and pure builds

The algorithm uses no threads, filesystem, native stack manipulation, or new
dependency. It is compiled in the pure `parse_sources` path as well as native
loads, including `--no-default-features` and wasm. `serde_stacker` is deliberately
not needed: growing native stacks would not solve pure wasm deserialization.

`source_node_fields(source, requested)` is the public pure projection helper
for full-show consumers. It decodes only selected source node mappings into
`BTreeMap<String, SourceValue>`, retaining unknown fields while omitting each
selected node's `children`. It walks the actual tree/root hierarchy iteratively,
checks the same standalone input budget once, matches trimmed IDs, and rejects
missing or duplicate identities. It does not construct a whole-tree dynamic
value. Shallow opaque tags, aliases, merges, Unicode, and CRLF values are
preserved.

The writer's source-position parser is a separate ownership boundary. It must
also use iterative collection construction and destruction and permit the same
bounded structural depth. Raw-show consumers should use that parser's original
UTF-8 byte spans, not reconstruct source from the normalized manifest or parse
an entire deep tree into recursively owned JSON values.

Permanent regressions exercise a genuine 10,000-deep flow hierarchy on a small
stack, preorder and all parent links, the first over-limit depth, malformed and
unknown deepest-node fields, bounded generic metadata, decoded block strings,
and exact native CRLF source retention. Integration verification is performed
by the parent implementation after concurrent work lands.
