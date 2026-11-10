# Local dependencies

These five crates are the Aptos dependencies used by the ceremony. They
are included locally so that building the artifact does not require
following a contributor-specific development branch.

Cargo generated the standalone manifests from the original workspace.
Source-control metadata and personal contributor annotations are omitted
from this review snapshot. Copyright and license notices are retained.
The root workspace selects these crates using path dependencies; other
dependencies are pinned in the root `Cargo.lock`.
