# Contributing to HWCode

Thank you for your interest in contributing to **HWCode**!

HWCode is an open-source, hardware-first programming language designed to unite low-level systems programming, hardware topology, and formal physical boundary verification.

---

## Code of Conduct

All contributors and maintainers are expected to follow our [Community Rules](community-rules.md).

---

## Getting Started

### Prerequisites
- [Rust Toolchain](https://rustup.rs/) (edition 2021, rustc 1.75+ recommended)
- Standard terminal with ANSI support

### Building from Source
```bash
git clone https://github.com/<your-username>/HWcode.git
cd HWcode
cargo build --release
```

### Running Tests
The compiler includes comprehensive automated unit and integration tests covering all 14 specification chapters and 4 roadmap phases:
```bash
cargo test
```

---

## Submitting Pull Requests

1. **Fork the Repository** and create your branch from `main`:
   ```bash
   git checkout -b feature/awesome-hardware-feature
   ```
2. **Adhere to Code Standards**:
   - Zero external crate dependencies for the core compiler and runtime. Keep the toolchain lightweight, fast, and self-contained.
   - Maintain the 7 Physical Boundary invariants in `src/sema.rs`.
   - Add unit tests in `src/lib.rs` for any new language features, syntax, or error diagnostics.
3. **Run the Test Suite**:
   ```bash
   cargo test
   ```
4. **Open a Pull Request** with a clear description of your changes and motivation.

---

## Reporting Issues & Security Vulnerabilities

- **Bug Reports**: Open a GitHub issue detailing the `.hwc` source code, expected behavior, and compiler diagnostic output.
- **Commercial Licensing & Enterprise Queries**: Please contact `shadow1q2w3e4r5@gmail.com`.
