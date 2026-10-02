# HWCode Compiler Architecture & Internals

The HWCode compiler (`hwc`) is written entirely in **Rust** (Standard Library only, zero external crate dependencies) to provide a single, dependency-free, high-performance compiler and hardware toolchain.

---

## Compiler Pipeline Overview

```
                      [ .hwc Source File ]
                                |
                                v
                       Lexer (lexer.rs)
                                |
                                v
                      Parser (parser.rs)
                                |
                                v
                     AST Model (ast.rs)
                                |
                                v
               Hardware Graph (hardware_graph.rs)
                                |
                                v
           Semantic Boundary Analyzer (sema.rs)
      [Enforces all 7 Physical Hardware Boundaries]
                                |
                                v
               Universal HIR (Universal HIR in hir.rs)
                                |
                                v
             Two-Level Optimizer (hir.rs: optimize)
             [Contention Solver, Link Interleaving]
                                |
     +--------------------------+--------------------------+
     |                          |                          |
     v                          v                          v
C11 Backend               Hardware Simulator         Visual Timeline
(codegen_c.rs)            (simulator.rs)             (visualizer.rs)
  - C11 Code Generator      - Cycle simulation         - HTML5 Swimlanes
  - Typestate Drivers       - Fault injection          - Perfetto / Chrome Trace
```

---

## 1. Hardware Graph (`src/hardware_graph.rs`)

The Universal Hardware Graph models computing machinery as a directed graph $G = (V, E)$:
- **Vertices ($V$)**: Hardware endpoints (`/F1` CPU, `/F2` RAM, `/F3` GPU, `/F4` NVMe, `/F5` NIC, `/F6` FPGA, `/F7` Sensor, `/F8` NPU, cluster nodes). Each node specifies memory domains, supported physical layouts, clock domains, and typestate rules.
- **Edges ($E$)**: Physical interconnects (PCIe Gen4, NVLink, DDR5 Coherent Bus, I2C, AXI4, RoCEv2 RDMA) annotated with bandwidth, latency, DMA availability, coherency, and power costs.
- **Route Planner**: Multi-constraint Dijkstra and BFS route solver with hot-swap fallback paths.

---

## 2. Universal Hardware Intermediate Representation (Universal HIR) (`src/hir.rs`)

Universal HIR is an explicit, architecture-neutral intermediate representation consisting of 16 discrete hardware opcodes:

| Opcode | Hardware Action |
| :--- | :--- |
| `RESOURCE_ACQUIRE` | Locks exclusive/shared hardware resource (`Mode::Exclusive`, `Mode::Shared`). |
| `RESOURCE_RELEASE` | Relinquishes ownership of hardware capability. |
| `ROUTE` | Selects physical interconnect path through the topology. |
| `DMA` | Initiates asynchronous or synchronous direct memory access transfer. |
| `LAYOUT_CONVERT` | Dispatches hardware tile/tensor swizzle engine (`Linear` $\leftrightarrow$ `Tiled2D` $\leftrightarrow$ `Tensor4D`). |
| `MEMORY_BARRIER` | Inserts hardware memory fence (`AcqRel_DMA_Coherency`). |
| `EVENT_SIGNAL` | Emits hardware completion event from DMA or kernel. |
| `EVENT_WAIT` | Blocks until hardware event is satisfied. |
| `GPU_DISPATCH` | Enqueues compute grid to discrete accelerator queue (`/F3::queue`). |
| `INTERRUPT_ATTACH`| Binds interrupt vector table entry to ISR function. |
| `CLOCK_SYNC` | Inserts hardware synchronizer (`doubleflop`, `async_fifo`, `handshake`). |
| `STATE_TRANSITION` | Dispatches register command to advance device state machine. |
| `BUDGET_ACCOUNT` | Linearly debits energy envelope against physical limit. |
| `RDMA_TRANSFER` | Dispatches remote direct memory access write/read over fabric. |
| `CONTENTION_SCHED`| Re-orders and interleaves concurrent transfers sharing physical links. |
| `TRACE_POINT` | Records high-resolution profiling marker. |

---

## 3. Two-Level Hardware Optimizer (`src/hir.rs`)

The optimizer executes two distinct stages:
1. **Level-1 (Logical HIR)**: Dead resource elimination, redunant memory barrier coalescing, redundant layout conversion elimination.
2. **Level-2 (Physical Contention Solver)**: Analyzes concurrent transfers sharing the same PCIe / interconnect link, calculates bandwidth contention penalties, and schedules interleaved DMA bursts to maximize throughput.

---

## 4. Hardware Simulator & Fault Injection Engine (`src/simulator.rs`)

Enables testing hardware algorithms without needing physical silicon:
- Tracks nanosecond-accurate timelines across multi-device swimlanes.
- Simulates dynamic power draw and junction temperatures.
- **Fault Injection Modes**:
  - `link_disconnect`: Simulates mid-flight PCIe link disconnection, triggering dynamic topology recovery: `NORMAL -> FAILED -> REPLAN -> FALLBACK -> RECOVERED`.
  - `dma_error`: DMA engine bus error recovery.
  - `thermal_throttle`: Clock throttling under extreme heat envelopes.
  - `gpu_timeout`: Watchdog timer timeout handling.

---

## 5. Visualizer & Timeline Profiler (`src/visualizer.rs`)

Generates two industry-standard profiling formats:
1. **Interactive HTML5 Visualizer** (`.timeline.html`): Self-contained SVG/CSS timeline with pan/zoom and multi-track hardware swimlanes.
2. **Chrome Trace / Perfetto Export** (`.trace.json`): Loadable directly into `chrome://tracing` or [perfetto.dev](https://ui.perfetto.dev).

---

## 6. C11 Backend & Typestate Scaffolder (`src/codegen_c.rs`)

Lowers Universal HIR instructions to verified portable C11 runtime source code, and generates compile-time typestate driver scaffolding from `device` blocks.
