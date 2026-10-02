# Changelog

All notable changes to HWCode will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [0.1.0] — 2026-10-02

### 🎉 Initial Public Release

**HWCode** — Universal Hardware-Oriented Systems Programming Language.

### Added

- **Core Compiler** (6,760+ lines of pure Rust, zero external dependencies)
  - Lexer with hardware endpoint tokenization (`/F3`, `/N0/F3`)
  - Recursive-descent parser for device blocks, typestates, and transfers
  - Semantic analyzer enforcing all 7 physical boundaries
  - Universal Hardware Graph with 8 device classes and cluster topology
  - 16-opcode Universal HIR with two-level optimizer
  - C11 code generation backend
  - Hardware simulator with 5 fault injection modes
  - Interactive HTML5 visual timeline profiler
  - Driver scaffold generator from `device` block definitions
  - Plain-English route explainer

- **The 7 Physical Boundaries** (all statically enforced at compile time)
  1. Memory & Physical Layout Boundary
  2. Device & Ownership Boundary (affine types)
  3. Clock-Domain Crossing (CDC) Boundary
  4. State Machine / Typestate Boundary
  5. Topology & Routing Boundary
  6. Energy & Thermal Linear Budget Boundary
  7. Temporal & Real-Time Boundary

- **CLI Commands**: `check`, `hir`, `explain`, `simulate`, `profile`, `build`, `scaffold-driver`, `run`, `graph`

- **Nokia 3310 Snake Game** — fully written in HWCode (`snake.hwc`) with interactive CLI runner

- **Self-Hosted Compiler** — `selfhost/hwcc.hwc` demonstrates the compiler compiling itself

- **13 Integration Tests** — covering all 4 development phases (100% passing)

- **6 Example Programs** including distributed cluster RDMA routes, GPU pipelines, and NPU tensor packing

- **Dual-License Model** — AGPLv3 (open source) + Commercial (enterprise)

- **Full Documentation Suite**
  - `README.md` — project overview with badges and quickstart
  - `getting-started.md` — step-by-step tutorial
  - `language-reference.md` — operators, endpoints, typestates
  - `how-the-compiler-works.md` — compiler architecture internals
  - `how-to-contribute.md` — PR guidelines and coding standards
  - `community-rules.md` — Contributor Covenant 2.1
  - `buy-commercial-license.md` — enterprise inquiry guide
  - `commercial-license-terms.md` — formal license terms
  - `SECURITY.md` — vulnerability reporting policy
