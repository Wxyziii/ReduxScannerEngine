# Architecture

## High-level

```text
redux_rpf_scanner            C++ CLI launcher
└── tools/rpf_backend_rs     Rust backend
    ├── opens RPF archives
    ├── reads keys
    ├── walks entries
    ├── scans target files
    ├── hashes file data
    ├── compares clean vs modded
    └── writes JSON
```

The C++ frontend exists mainly to provide a user-friendly CLI and stable wrapper around the Rust backend.

## C++ launcher responsibilities

`src/cpp/redux_rpf_scanner.cpp` should:

- parse command-line arguments
- locate default backend path
- validate required input paths
- create output parent folder
- forward args to backend
- report backend failure clearly
- stay cross-platform

It should not implement the deep RPF logic.

## Rust backend responsibilities

`rpf_backend_rs/src/main.rs` should:

- open `update.rpf`
- load GTA keys from a user-provided key directory
- walk entries using `rpf-archive`
- recursively inspect nested RPFs where relevant
- hash selected file data
- compare clean and modded manifests
- classify changes into Redux components
- write stable JSON reports

## Future deep analyzer architecture

The scanner core finds **what files changed**.

Format-specific analyzers later explain **what changed inside those files**:

```text
XML/timecycle analyzer
DAT/META/YMT analyzer
YTD texture analyzer
GFX/SWF analyzer
YPT particle analyzer
```

Keep analyzers separate from the core scanner where possible.
