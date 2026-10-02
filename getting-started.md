# HWCode Getting Started & Complete User Guide

Welcome to **HWCode**! This guide walks you through everything you need to start writing, verifying, simulating, and deploying hardware-oriented programs from scratch.

---

## Table of Contents

1. [Prerequisites & System Setup](#1-prerequisites--system-setup)
2. [Building the Compiler & Tools](#2-building-the-compiler--tools)
3. [Writing Your First HWCode Program](#3-writing-your-first-hwcode-program)
4. [Mastering the 7 Physical Boundaries](#4-mastering-the-7-physical-boundaries)
5. [The Complete `hwc` CLI Workflow](#5-the-complete-hwc-cli-workflow)
6. [Interfacing with Other Languages & Systems (C/C++, GPU, FPGA, Linux)](#6-interfacing-with-other-languages--systems)
7. [Running the Interactive Nokia 3310 Snake Game](#7-running-the-interactive-nokia-3310-snake-game)
8. [Troubleshooting & Compiler Diagnostic Guide](#8-troubleshooting--compiler-diagnostic-guide)

---

## 1. Prerequisites & System Setup

HWCode is engineered with **zero external crate dependencies**. All you need is the standard Rust toolchain:

### Required:
- **Rust & Cargo** (Edition 2021, Rust 1.75+): Install via [rustup.rs](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
  *(On Windows, download and run `rustup-init.exe`)*

### Optional (for native C linking & web profiling):
- **C Compiler** (GCC, Clang, or MSVC): Required if you compile generated C11 code to native machine code (`hwc build`).
- **Modern Web Browser** (Chrome, Firefox, Edge, Safari): To view interactive HTML5 timeline profiles (`.timeline.html`) and Perfetto flamecharts (`.trace.json`).

---

## 2. Building the Compiler & Tools

Clone the repository and compile the release binaries:

```bash
git clone https://github.com/<your-username>/HWcode.git
cd HWcode
cargo build --release
```

This builds two executables in `target/release/`:
- **`hwc`**: The core compiler, verifier, HIR optimizer, physical explainer, hardware simulator, and C11 backend.
- **`snake`**: The interactive Nokia 3310 Snake game written in pure HWCode.

### Run the Automated Test Suite:
Verify that all 4 roadmap phases pass on your machine:
```bash
cargo test
# Output: test result: ok. 13 passed; 0 failed; finished in 0.02s
```

*(Optional)* Install `hwc` and `snake` to your global `PATH`:
```bash
cargo install --path .
```

---

## 3. Writing Your First HWCode Program

Create a file named `hello_hardware.hwc`:

```hwcode
// hello_hardware.hwc — Your first hardware-oriented program

// 1. Declare a custom physical layout (Section 4)
layout VectorTile {
    length: 64;
    alignment_bytes: 32;
}

// 2. Define a hardware peripheral state machine (Section 7)
device ThermalSensor {
    bus: I2C;
    clock: SENSOR_CLK;
    states: [Idle, Active, Overheat];
    initial: Idle;
    register START: u32;
    register TEMP: f32;
    transition Idle -> Active on REG.START = 1;
    transition Active -> Idle on REG.START = 0;
    op read_temperature requires Active;
}

// 3. Main program with realtime & power budget constraints (Section 8 & 9)
@realtime(deadline: 20ms, period: 20ms, max_power: 10W)
fn main() -> i32 {
    trace("system_startup");

    // Acquire an energy envelope of 10 Watts
    let system_power = acquire_budget(10W);
    let sensor_power = split_budget(system_power, 2W);
    let compute_power = split_budget(system_power, 8W);

    // Discover the sensor in the Hardware Graph & transition state
    let mut sensor = find(Sensor);
    sensor.REG.START = 1;
    let temp = sensor.read_temperature();

    // Allocate memory in Host System RAM
    let mut telemetry_buf: RAM<Matrix<f32>>(VectorTile) = RAM<Matrix<f32>>(VectorTile);

    // Route telemetry data from Sensor to Host RAM
    /F7 -> /F2 : telemetry_buf;

    trace("telemetry_acquired");
    return 0;
}
```

---

## 4. Mastering the 7 Physical Boundaries

HWCode will **refuse to compile code** that violates physical hardware invariants. Here is how each boundary protects your system:

### Boundary 1: Memory Layout Mismatch
* **The Hazard**: You try to copy linear system RAM bytes directly into a GPU swizzled tile without converting layout.
* **HWCode Solution**: The compiler detects that `RAM<Matrix<f32>>(Linear)` and `VRAM<Matrix<f32>>(Tiled2D)` have incompatible strides, and automatically inserts an explicit `LAYOUT_CONVERT` instruction via a hardware swizzle engine.

### Boundary 2: Device Affine Ownership
* **The Hazard**: Two threads attempt to issue commands to an exclusive DMA channel at the same time.
* **HWCode Solution**: Exclusive capabilities (`acquire(/F3::dma, Mode::Exclusive)`) are affine; moving ownership invalidates the original variable at compile time:
  ```hwcode
  let dma_ch = acquire(/F3::dma, Mode::Exclusive);
  dispatch_dma(dma_ch);
  // dispatch_dma(dma_ch); <-- COMPILE ERROR E0501: Use-after-move!
  ```

### Boundary 3: Asynchronous Clock-Domain Crossing (CDC)
* **The Hazard**: Reading a raw register or pulse generated by a 250 MHz FPGA clock domain inside a 3600 MHz CPU loop causes metastability.
* **HWCode Solution**: The compiler emits `E0601 ClockDomainHazard` unless synchronized with a certified primitive:
  ```hwcode
  let safe_pulse = doubleflop(raw_fpga_pulse, FPGA_CLK -> CPU_CLK);
  ```

### Boundary 4: Typestate Machine Correctness
* **The Hazard**: Reading data from an optical sensor while it is still in `Idle` or `Configuring` mode.
* **HWCode Solution**: Operation calls require statically proven typestates (`op read_temperature requires Active`). The compiler enforces register transitions (`sensor.REG.START = 1;`) before permitting the call.

### Boundary 5: Topology Route Availability
* **The Hazard**: Requesting a direct point-to-point PCIe transfer between two devices that lack a physical bridge or peer memory support.
* **HWCode Solution**: The Route Planner automatically detects link unavailability and routes through system memory staging:
  ```hwcode
  disk ■ gpu : data fallback [disk -> cpu -> gpu];
  ```

### Boundary 6: Power & Thermal Envelopes
* **The Hazard**: Attempting to run a 60W GPU workload on a 20W battery envelope.
* **HWCode Solution**: The compiler accounts for power linearly (`acquire_budget(20W)`). If the sum of sub-allocations exceeds the acquired budget, compilation aborts with `E0901 PowerBudgetExceeded`.

### Boundary 7: Hard Real-Time Constraints
* **The Hazard**: A function marked `@realtime(deadline: 5ms)` triggers an unbounded dynamic heap allocation or a high-latency link.
* **HWCode Solution**: Disallows dynamic heap allocation inside `@realtime` routines and verifies that cumulative route latencies stay below the deadline.

---

## 5. The Complete `hwc` CLI Workflow

The `hwc` toolchain provides an end-to-end pipeline for hardware-aware development:

```
[Write .hwc] -> [hwc check] -> [hwc explain] -> [hwc simulate] -> [hwc profile] -> [hwc build]
```

### 1. Static Verification
```bash
hwc check hello_hardware.hwc
```
*Validates all 7 physical boundaries without producing binaries.*

### 2. Explanation of Hardware Decisions
```bash
hwc explain hello_hardware.hwc
```
*Outputs human-readable justification for every route, layout conversion, clock synchronizer, and power budget allocation.*

### 3. Lowering to Universal HIR
```bash
hwc hir hello_hardware.hwc
```
*Displays the 16 Universal Hardware Intermediate Representation opcodes.*

### 4. Simulating Hardware Cycles with Fault Injection
Test resilience against real-world hardware failures:
```bash
# Test how the route planner recovers from a broken PCIe link mid-flight:
hwc simulate hello_hardware.hwc --fault link_disconnect
```
*Simulates lifecycle transitions: `NORMAL -> FAILED -> REPLAN -> FALLBACK -> RECOVERED`.*

### 5. Profiling with Interactive HTML5 Timelines
```bash
hwc profile hello_hardware.hwc
```
*Generates `hello_hardware.timeline.html` and `hello_hardware.trace.json`. Open `hello_hardware.timeline.html` in your browser to inspect hardware swimlanes, or drag `hello_hardware.trace.json` into [ui.perfetto.dev](https://ui.perfetto.dev).*

### 6. Compiling to Portable C11
```bash
hwc build hello_hardware.hwc -o hello_hardware.exe
```
*Produces clean, verified C11 runtime source code (`.gen.c`) and compiles it via your system C compiler.*

### 7. Scaffolding State-Safe C Drivers
```bash
hwc scaffold-driver hello_hardware.hwc
```
*Emits compile-time typestate C driver structs and transition functions directly from your `device` declarations.*

---

## 6. Interfacing with Other Languages & Systems

HWCode is engineered to integrate seamlessly into existing embedded and high-performance stacks:

### 6.1 Interfacing with C & C++
HWCode functions can link against C ABIs using `extern "C"`:
```hwcode
extern "C" fn printf(fmt: str) -> i32;
extern "C" fn custom_dma_irq_handler(vector: u32) -> i32;

fn main() {
    printf("Connecting to hardware...\n");
}
```
When compiled with `hwc build`, HWCode generates standard C11 code with portable function signatures and headers, making it trivial to embed into existing C/C++ codebases.

### 6.2 Interfacing with GPUs (CUDA / OpenCL / Vulkan)
Declare GPU kernels and dispatch them to discrete queues:
```hwcode
kernel fn vector_add_kernel(a: RAM<Matrix<f32>>, b: RAM<Matrix<f32>>) -> f32 {
    return 0.0;
}

fn launch_pipeline() {
    let queue = acquire(/F3::queue, Mode::Shared);
    dispatch(queue, vector_add_kernel, grid: (32, 32, 1), block: (16, 16, 1), args: [a, b]);
}
```

### 6.3 Interfacing with FPGAs & Verilog/VHDL
Use `/F6` to route streams directly to reconfigurable fabric, and synchronize AXI4 streaming signals across clock domains with `doubleflop` or `async_fifo`:
```hwcode
let fpga_stream = /F6::memory;
let synced_pulse = doubleflop(raw_trigger, FPGA_CLK -> CPU_CLK);
/F1 -> /F6 : fpga_stream;
```

### 6.4 Interfacing with Distributed Clusters & RDMA (RoCEv2)
Address remote cluster nodes with `/N0/...`, `/N1/...`, `/N2/...`:
```hwcode
let node0_cluster = acquire(/N0/F3, Mode::Shared);
let node1_cluster = acquire(/N1/F3, Mode::Shared);

// Remote Direct Memory Access transfer over 100G RoCEv2 fabric
/N0/F3 -> /N1/F3 : tensor_weights;
```

### 6.5 Interfacing with Embedded Linux & RTOS
- **Memory-Mapped I/O**: Use domain `MMIO` and endpoint `/F7`.
- **Interrupt Vectors**: Attach ISR routines directly with `interrupt /F7::interrupt vector 33 attach my_isr;`.
- **Direct Hardware Access**: Use `unsafe capability(Cap::MMIO)` to perform raw register blits with a formal security audit trail.

---

## 7. Running the Interactive Nokia 3310 Snake Game

HWCode includes a complete, playable **Nokia 3310 Snake Game** written in 100% pure HWCode ([`snake.hwc`](snake.hwc)).

### Launching the Game:
Simply run:
```bash
snake
```
*(or `hwc run snake.hwc`)*

### Gameplay Controls:
- **`W` / `↑`**: Move Up
- **`S` / `↓`**: Move Down
- **`A` / `←`**: Move Left
- **`D` / `→`**: Move Right
- **`P`**: Pause / Resume
- **`R` / Space**: Play Again (on Game Over)
- **`Q`**: Quit back to terminal

The game dynamically verifies its hardware plan (`/F1` CPU, `/F7` LCD Display, 1.5W battery budget envelope), enters raw console mode with ANSI graphics, and runs the realtime game loop.

---

## 8. Troubleshooting & Compiler Diagnostic Guide

When compilation fails, HWCode diagnostics explain **why** the physical boundary was violated:

| Error Code | Error Name | Cause & Solution |
| :--- | :--- | :--- |
| **`E0401`** | `LayoutMismatch` | Source and destination require different physical representations (e.g. `Linear` vs `Tiled2D`). Insert explicit layout conversion or allow compiler automatic insertion. |
| **`E0501`** | `AffineUseAfterMove` | An exclusive capability handle was already consumed. Acquire a new handle or share ownership (`Mode::Shared`). |
| **`E0601`** | `ClockDomainHazard` | An asynchronous signal crossed clock domains without a synchronizer. Wrap the signal with `doubleflop(signal, CLK_A -> CLK_B)`. |
| **`E0701`** | `InvalidDeviceState` | A device operation was invoked in an illegal state. Execute the required register write transition first (e.g. `dev.REG.START = 1;`). |
| **`E0901`** | `PowerBudgetExceeded` | The sum of sub-allocations exceeds the acquired energy envelope. Increase `acquire_budget(...)` or reduce component wattages. |
| **`E0801`** | `RealtimeDeadlineMiss`| Total route/DMA latencies exceed `@realtime(deadline: ...)` constraint. Use a faster interconnect (PCIe direct vs staged RAM). |

---

## 9. Commercial Licensing & Inquiries

For commercial projects, enterprise deployments, and closed-source proprietary hardware support:
- **Direct Contact**: [`shadow1q2w3e4r5@gmail.com`](mailto:shadow1q2w3e4r5@gmail.com)
- **Reference**: See [`buy-commercial-license.md`](buy-commercial-license.md) and [`commercial-license-terms.md`](commercial-license-terms.md) for full terms.
