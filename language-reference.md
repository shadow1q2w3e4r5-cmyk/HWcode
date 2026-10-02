# HWCode Language Specification Reference (v0.1.0)

HWCode is a Universal Hardware-Oriented Systems Programming Language based on the architectural principle that **hardware is a first-class citizen of the programming language model**.

---

## 1. Hardware Endpoints & Addressing (Section 2 & 3)

Hardware devices are identified by structured physical endpoint identifiers:
- `/F1`: Host Central Processing Unit (CPU)
- `/F2`: Main System Memory (RAM)
- `/F3`: Discrete Graphics Processing Unit (GPU)
- `/F4`: Non-Volatile Storage Controller (NVMe / SSD)
- `/F5`: Network Interface Controller (NIC)
- `/F6`: Field Programmable Gate Array (FPGA Fabric)
- `/F7`: Peripheral Sensor / I2C / MMIO Bus
- `/F8`: Neural / Tensor Processing Unit (NPU)
- `/N0/...`, `/N1/...`: Distributed Cluster Compute Nodes

Sub-resources are addressed via `::`, such as `/F3::dma`, `/F3::queue`, `/F7::interrupt`.

---

## 2. Transfer Operators & Route Planning (Section 3 & 8)

HWCode provides first-class syntax for physical data movement across buses:

### 2.1 Direct DMA & Explicit Route (`->`)
```hwcode
// Explicit point-to-point DMA transfer
/F4 -> /F3 : data_buffer;

// Multi-hop staged transfer via system memory
/F4 -> /F2 -> /F3 : data_buffer;

// Asynchronous DMA event handle
let dma_event = dma /F4 -> /F3 : data_buffer;
await dma_event;
```

### 2.2 Autonomous Topology Discovery & Waypoints (`■` or `~>`)
```hwcode
// Auto-route between endpoints with constraint solver
disk ■ gpu : data_buffer with {
    deadline: 5ms,
    power: 20W,
};

// Auto-route with hot-swap dynamic fallback path
disk ■ gpu : data_buffer fallback [disk -> cpu -> gpu];
```

---

## 3. The 7 Physical Boundaries (Section 4–10)

The HWCode compiler guarantees physical correctness across 7 fundamental boundaries:

### 3.1 Memory Domain & Physical Layout Boundary
Data types specify their physical memory domain and layout:
```hwcode
let host_data: RAM<Matrix<f32>>(Linear) = RAM<Matrix<f32>>(Linear);
let gpu_tile: VRAM<Matrix<f32>>(Tiled2D) = VRAM<Matrix<f32>>(Tiled2D);
let npu_tensor: DeviceRAM<Matrix<f32>>(Tensor4D) = DeviceRAM<Matrix<f32>>(Tensor4D);
```
When moving data between incompatible layouts, the compiler inserts an explicit `LAYOUT_CONVERT` instruction and hardware engine swizzler.

### 3.2 Device & Affine Ownership Boundary
Exclusive hardware handles follow affine move semantics:
```hwcode
let dma_ch = acquire(/F3::dma, Mode::Exclusive);
// Passing dma_ch moves ownership; use-after-move is rejected at compile time.
release(dma_ch);
```

### 3.3 Clock-Domain Crossing (CDC) Boundary
Signals crossing asynchronous clock domains must use certified synchronizers:
```hwcode
// Synchronize raw FPGA clock pulse to CPU clock domain
let safe_signal = doubleflop(raw_fpga_pulse, FPGA_CLK -> CPU_CLK);
```
Unsynchronized raw crossings generate compile error `E0601 ClockDomainHazard`.

### 3.4 Hardware State Machine & Typestate Boundary
Devices declare explicit operational states and transitions:
```hwcode
device CryoSensor {
    bus: I2C;
    clock: SENSOR_CLK;
    states: [Idle, Configuring, Active, Error];
    initial: Idle;
    register START: u32;
    transition Idle -> Active on REG.START = 1;
    transition Active -> Idle on REG.START = 0;
    op read_temperature requires Active;
}
```
Calling `read_temperature()` when the device is not proven to be in `Active` is rejected at compile time.

### 3.5 Topology & Link Routing Boundary
Validates physical link availability, bandwidth limits, and route feasibility in the active Hardware Graph.

### 3.6 Energy & Thermal Linear Budget Boundary
Energy envelopes are acquired and linearly accounted for:
```hwcode
let battery = acquire_budget(100W);
let gpu_power = split_budget(battery, 60W);
let cpu_power = split_budget(battery, 30W);
// Compiler statically verifies sum <= acquired envelope.
```

### 3.7 Temporal & Real-Time Boundary
Annotate hard real-time subroutines:
```hwcode
@realtime(deadline: 10ms, period: 10ms, max_power: 25W)
fn control_loop() -> i32 {
    // Dynamic heap allocations are forbidden inside @realtime blocks.
    return 0;
}
```

---

## 4. Unsafe Security Audit Trail (Section 10)

Hardware operations that bypass compiler proofs must be enclosed in explicit capability blocks:
```hwcode
unsafe capability(Cap::MMIO) {
    // Direct MMIO pointer access
}
```
All unsafe capability blocks are logged by the compiler into an immutable audit report.
