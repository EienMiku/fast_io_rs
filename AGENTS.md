# Working on fast_io_rs

- Use stable Rust for implementation. Nightly is a compatibility check, not a dependency.
- Follow the Presence-based optional customization protocol established in this conversation.
- Keep implementation commits small and independently buildable. Commit each coherent feature.
- Write focused tests for boundary conditions and I/O correctness; avoid excessive or redundant tests.
- Maintain no_std support and gate allocation/std explicitly.
- Do not claim a speedup without a reproducible benchmark.
- No upstream C++ source is vendored in this repository. Preserve attribution for any future imported code.
- Keep docs/DEVELOPMENT.md accurate enough to resume after a context reset.
