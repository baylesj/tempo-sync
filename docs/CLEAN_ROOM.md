# Clean Room Implementation Process

This document was written by reading the GPLv2+ Ableton Link C++ sources.
The associated Rust implementation is written solely from these protocol documents, ensuring a clean-room implementation free of GPL contamination.
Interop is verified against Ableton's compiled binaries.

## Source Revisions

The documentation was produced by examining the following upstream repositories and commits:

- **Ableton Link (C++)**: `5ba907ae027a8c2ec8824463e37296dcefc5e9be`
- **Ableton Link RS (Rust)**: `6fa2263149f19132bd87d8880839f0cfa894a009` (Used only as a second opinion)
