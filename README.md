# HWCode — Universal Hardware-Oriented Systems Language

[![License: Dual AGPL-3.0 / Commercial](https://img.shields.io/badge/License-Dual%20AGPLv3%20%2F%20Commercial-blue.svg)](LICENSE)
[![Rust: 2021 Edition](https://img.shields.io/badge/Rust-2021%20Edition-orange.svg)](https://www.rust-lang.org/)
[![Dependencies: Zero](https://img.shields.io/badge/Dependencies-Zero%20External%20Crates-brightgreen.svg)](Cargo.toml)
[![Tests: 13/13 Passed](https://img.shields.io/badge/Tests-13%2F13%20Passing-success.svg)](src/lib.rs)
[![Commercial: Inquiries Open](https://img.shields.io/badge/Commercial%20License-shadow1q2w3e4r5%40gmail.com-blueviolet.svg)](buy-commercial-license.md)

**HWCode** is a universal hardware-oriented systems programming language built on one foundational premise: **hardware must be a first-class element of the programming language model**, rather than an afterthought buried beneath operating system drivers, vendor SDKs, and opaque runtimes.

The compiler views your machine not as a CPU with external peripherals, but as a unified **Universal Hardware Graph** of physical computing resources (CPUs, GPUs, RAM, VRAM, NVMe, NICs, FPGA fabrics, NPUs, and sensors) linked by real physical interconnects.

---

## 🕹️ Interactive Showcase: Nokia 3310 Snake Game

HWCode comes with a complete, authentic **Nokia 3310 Snake Game** written **100% in HWCode** ([`snake.hwc`](snake.hwc)), featuring real-time non-blocking terminal controls and retro LCD aesthetics:

```text
  .──────────────────────────────────────────────────────────.
 /                        NOKIA  3310                         \
|  [lll] 4G                                  BATTERY: [████]  |
+────────────────────────────────────────────────────────────+
|  SCORE: 0040    HI: 0150    SPEED: 80ms    LEN: 06         |
+────────────────────────────────────────────────────────────+
|  ┌──────────────────────────────────────────────────────┐  |
|  │██████████████████████████████████████████████████████│  |
|  │█                                                    █│  |
|  │█                                                    █│  |
|  │█               oooooooo►                            █│  |
|  │█                                                    █│  |
|  │█                                 ★                  █│  |
|  │█                                                    █│  |
|  │█                                                    █│  |
|  │██████████████████████████████████████████████████████│  |
|  └──────────────────────────────────────────────────────┘  |
+────────────────────────────────────────────────────────────+
|  [W/A/S/D or Arrows] Move       [P] Pause    [Q] Quit       |
|  HWCode Plan: /F1 CPU + /F7 LCD Controller (1.5W Envelope)  |
\____________________________________________________________/
```

### Run it immediately:
```bash
snake
```
*(or `hwc run snake.hwc`)*

---

## 🛡️ The 7 Physical Boundaries

HWCode statically enforces correctness across 7 physical boundaries at compile time:

1. **Memory & Physical Layout Boundary**: Reconciles semantic types with divergent physical layouts (`Linear` $\leftrightarrow$ `Tiled2D` $\leftrightarrow$ `Tensor4D`), automatically inserting explicit `LAYOUT_CONVERT` swizzle engines and coherency barriers (`MEMORY_BARRIER`).
2. **Device & Ownership Boundary**: Enforces affine ownership over exclusive hardware resources (`acquire(/F3::dma, Mode::Exclusive)`). Prevents use-after-move and double-release at compile time.
3. **Clock-Domain Crossing (CDC) Boundary**: Rejects raw signal crossings across asynchronous clock domains unless certified hardware synchronizers (`doubleflop`, `async_fifo`, `handshake`) are specified.
4. **State Machine / Typestate Boundary**: Statically proves that operations on hardware devices (`Sensor`, `GPU`, `FPGA`) are called only within valid declared typestates (`Idle`, `Active`, `Configuring`).
5. **Topology & Routing Boundary**: Validates physical link feasibility across the Hardware Graph; autonomously discovers multi-hop paths or routes via system RAM staging.
6. **Energy & Thermal Linear Budget Boundary**: Linearly debits power budgets against physical supply limits (`acquire_budget(50W)`), proving safety before execution.
7. **Temporal & Real-Time Boundary**: Verifies `@realtime(deadline, period)` routines against scheduled routing latencies, strictly forbidding dynamic allocations.

---

## 🚀 Quickstart Guide

### 1. Build from Source (Zero Dependencies)
```bash
git clone https://github.com/<your-username>/HWcode.git
cd HWcode
cargo build --release
```

### 2. Run the Test Suite
```bash
cargo test
# 13/13 integration tests passing across all 4 roadmap phases
```

### 3. Verify a Program's Physical Boundaries
```bash
./target/release/hwc check snake.hwc
```

### 4. Inspect Universal HIR (Universal Hardware IR)
```bash
./target/release/hwc hir examples/master_space_observatory.hwc
```

### 5. Explain Compiler Hardware Routing Decisions
```bash
./target/release/hwc explain examples/master_space_observatory.hwc
```

### 6. Simulate Cycles & Inject Hardware Faults
```bash
# Inject a PCIe link disconnect mid-flight and test dynamic route recovery
./target/release/hwc simulate examples/master_space_observatory.hwc --fault link_disconnect
```

### 7. Profile with Interactive Visual Timeline
```bash
./target/release/hwc profile examples/master_space_observatory.hwc
# Generates master_space_observatory.timeline.html and trace.json
```

---

## 🛠️ CLI Subcommands Reference (`hwc`)

| Command | Description |
| :--- | :--- |
| `hwc check <file.hwc>` | Statically verifies all 7 physical boundaries (0 errors). |
| `hwc hir <file.hwc>` | Lowers code to Universal HIR and applies Level-1 & Level-2 optimizations. |
| `hwc explain <file.hwc>` | Generates plain-English rationale for routing, layout swizzling, and power splits. |
| `hwc simulate <file.hwc> [--fault <kind>]` | Cycle-accurate hardware simulator with fault injection (`link_disconnect`, etc.). |
| `hwc profile <file.hwc>` | Exports interactive HTML5 visual timelines and Chrome Trace / Perfetto profiles. |
| `hwc build <file.hwc>` | Compiles program to portable verified C11 backend source code. |
| `hwc scaffold-driver <file.hwc>` | Auto-generates typestate C driver headers from `device` blocks. |
| `hwc run [file.hwc]` | Runs interactive HWCode programs directly in the terminal. |
| `hwc graph` | Displays active Universal Hardware Graph topology and interconnect links. |

---

## 🗺️ 4-Phase Roadmap & Milestones

- [x] **Phase 1: Core Foundation** — Lexer, Parser, Hardware Graph (`/F1`..`/F7`), 7 Boundaries, Universal HIR (16 opcodes), Two-Level Optimizer, C11 Backend.
- [x] **Phase 2: Device Expansion** — GPU Queues & Compute Kernels, Hardware Interrupts & ISR attachment, PCIe Contention Solver, HTML5 Visualizer.
- [x] **Phase 3: Clusters & Acceleration** — Distributed Cluster Nodes (`/N0/F3`, `/N1/F3`) over RDMA RoCEv2, dedicated NPU Tensor Processor (`/F8`) with automatic `Tensor4D` packing, FPGA Streams.
- [x] **Phase 4: Full Self-Hosting** — Compiler written in HWCode itself at [`selfhost/hwcc.hwc`](selfhost/hwcc.hwc).

---

## 💼 Dual-Licensing & Commercial Inquiries

HWCode is offered under a **Dual-Licensing Model**:

1. **Free / Open Source**: [GNU Affero General Public License v3.0 (AGPL-3.0)](LICENSE) for open-source, non-commercial, academic, and research use.
2. **Commercial License**: Required for companies, proprietary products, embedded silicon, and closed-source software.

> **Need a Commercial License or Enterprise Support?**  
> Contact directly: **`shadow1q2w3e4r5@gmail.com`**  
> See [buy-commercial-license.md](buy-commercial-license.md) and [commercial-license-terms.md](commercial-license-terms.md) for full terms and pricing inquiries.

---

## 📖 Documentation & Guides

- 🚀 **[Getting Started & User Guide](getting-started.md)** — Step-by-step tutorial from zero to first program, CLI workflow, and C/GPU/FPGA integration.
- 📘 [Language Reference](language-reference.md) — Comprehensive guide to endpoints, operators (`->`, `~>`, `■`), and typestates.
- ⚙️ [How the Compiler Works](how-the-compiler-works.md) — Internals of the Lexer, Parser, Universal HIR, and Optimizer.
- 💼 [Buy Commercial License](buy-commercial-license.md) — Enterprise rights, OEM tiers, and purchasing process.
- 🤝 [How to Contribute](how-to-contribute.md) — How to submit pull requests and run tests.
- 📜 [Community Rules](community-rules.md) — Contributor covenant standards.
- 🛡️ [Security Policy](SECURITY.md) — How to report vulnerabilities privately.
- 📋 [Changelog](CHANGELOG.md) — Release history and what's new.

