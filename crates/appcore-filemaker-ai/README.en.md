# appcore-filemaker-ai

[Português](README.pt.md) | [Français](README.fr.md)

Optional bounded bridge between `appcore-ai` and `appcore-filemaker`. It keeps
model policy, tool schemas, call budgets, mutation validation, and artifact
access outside the deterministic FileMaker core.

All tool arguments use closed schemas, mutations resolve a candidate before
commit, and bridge limits can only tighten the core `ResourceLimits`.
Serialized result sizing writes into a non-retaining bounded counter and stops
at `max_result_bytes`; it does not allocate a second complete JSON buffer.

The complete create/patch/inspect/validate/preview/debug-mask/export loop is
executable and policy checked. Dataset sessions can export one selected table
as bounded in-memory CSV; graphical tools still require a resolved scene.
Capability discovery and export expose editable, flattened, and hybrid PDF;
hybrid combines vector outlines with invisible searchable Unicode text.
Schema discovery exposes `horizontal` and implemented `vertical_rl` writing;
only color emoji remains a prepared text capability.

See the [English guide](wiki/guide.en.md), [basic example](wiki/examples/basic.en.md),
and [intermediate example](wiki/examples/intermediate.en.md).

License: MIT.
