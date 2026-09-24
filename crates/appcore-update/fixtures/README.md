# Update evidence fixtures

These bounded fixtures are test inputs only. They contain no secret, signing
key, application payload or production path.

- `cache/` contains a resumable partial and a deliberately digest-mismatched
  object.
- `catalog/ambiguous.json` contains two releases with the same application,
  channel, target and version, which must be rejected before selection.
- `receipts/incomplete-v2.json` omits the required artifact fields and must hit
  the decode/upgrade wall.

The focused fixture tests consume these files with `include_bytes!` and
`include_str!`, so they remain portable and do not depend on a developer's
filesystem layout.
