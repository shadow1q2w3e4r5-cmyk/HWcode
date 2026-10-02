//! Universal Hardware Intermediate Representation (Universal HIR) & Two-Level Optimizer.
//! Implements Section 4 (Layout Optimization) and Section 11 (Universal HIR and Compiler Architecture).

use crate::hardware_graph::PlannedRoute;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    Fast, // Fast developer iteration mode
    Max,  // Maximum global hardware placement, layout elision, and async overlap optimization
}

/// The 12 canonical Universal HIR opcodes from Section 11 (Page 11), plus core allocation/compute ops.
#[derive(Debug, Clone, PartialEq)]
pub enum HirInstruction {
    /// `RESOURCE_ACQUIRE`: Exclusive or shared capability/endpoint acquisition
    ResourceAcquire {
        dest_var: String,
        endpoint: String,
        mode: String,
    },
    /// `RESOURCE_RELEASE`: Explicit or scope-exit release of an owned hardware resource
    ResourceRelease { var_name: String, endpoint: String },
    /// `ROUTE`: Selected multi-hop or peer-to-peer path through the Hardware Graph
    Route {
        route_id: usize,
        planned: PlannedRoute,
        payload: String,
    },
    /// `DMA`: First-class DMA engine descriptor & transfer operation
    Dma {
        engine: String,
        src_endpoint: String,
        dst_endpoint: String,
        payload: String,
        bytes: usize,
        async_event: Option<String>,
    },
    /// `LAYOUT_CONVERT`: Physical data representation transformation (e.g., `Linear -> Tiled2D`)
    LayoutConvert {
        buffer: String,
        semantic_type: String,
        from_layout: String,
        to_layout: String,
        mechanism: String, // e.g., "GPU_Swizzle_Kernel" or "DMA_ScatterGather"
        estimated_cost_ns: u64,
    },
    /// `CLOCK_SYNC`: Hardware clock-domain crossing synchronizer (`doubleflop`, `async_fifo`, `handshake`)
    ClockSync {
        signal: String,
        from_domain: String,
        to_domain: String,
        primitive: String,
        latency_cycles: u32,
    },
    /// `STATE_TRANSITION`: Verified hardware state machine transition (e.g., `Sensor: Idle -> Active`)
    StateTransition {
        device_var: String,
        device_class: String,
        from_state: String,
        to_state: String,
        trigger: String,
    },
    /// `EVENT_SIGNAL`: Hardware completion event publication
    EventSignal {
        event_name: String,
        source_op: String,
    },
    /// `EVENT_WAIT`: Dependency graph synchronization wait on an async event
    EventWait { event_name: String },
    /// `MEMORY_BARRIER`: Cache flush/invalidate or memory ordering fence across non-coherent domains
    MemoryBarrier {
        ordering: String, // e.g., "AcqRel", "Device", "DMA_Coherency_Flush"
        domain_from: String,
        domain_to: String,
    },
    /// `DEVICE_COMMAND`: State-verified device operation or MMIO register command
    DeviceCommand {
        device_var: String,
        endpoint: String,
        command: String,
        verified_state: String,
    },
    /// `NATIVE_CALL`: Standard CPU function or `extern "C"` call
    NativeCall {
        dest_var: Option<String>,
        func_name: String,
        args: Vec<String>,
    },
    /// Physical domain buffer binding (`RAM<Matrix<f32>>(Linear)`, `VRAM<Matrix<f32>>(Tiled2D)`)
    DomainAlloc {
        var_name: String,
        domain: String,
        semantic_type: String,
        layout: String,
        bytes: usize,
    },
    /// Power/Thermal budget accounting entry
    BudgetAccount {
        budget_var: String,
        allocated_watts: f64,
        remaining_watts: f64,
    },
    /// Observability trace point
    TracePoint { label: String },
    /// GPU Compute Queue Dispatch (Phase 2): `submit_queue(/F3::queue, kernel, grid, block)`
    GpuDispatch {
        queue: String,
        kernel_name: String,
        grid: (u32, u32, u32),
        block: (u32, u32, u32),
        args: Vec<String>,
        estimated_time_ns: u64,
    },
    /// Hardware Interrupt Vector Table Binding (Phase 2): `INTERRUPT_ATTACH`
    InterruptAttach {
        endpoint: String,
        vector: u32,
        handler_name: String,
    },
    /// Multi-Resource Contention Schedule Adjustment (Phase 2): `CONTENTION_SCHEDULE`
    ContentionSchedule {
        resource: String,
        concurrent_ops: usize,
        penalty_ns: u64,
    },
    /// Distributed Cluster RDMA Network Transfer (Phase 3): `RDMA_TRANSFER`
    RdmaTransfer {
        src_node: String,
        dst_node: String,
        payload: String,
        bytes: usize,
        verb: String,
    },
    /// NPU Dedicated Tensor/Matrix Acceleration (Phase 3): `NPU_TENSOR_OP`
    NpuTensorOp {
        endpoint: String,
        op_type: String,
        dims: (u32, u32, u32, u32),
        inputs: Vec<String>,
        output: String,
        latency_ns: u64,
    },
    /// FPGA Stream Pipe Channel (Phase 3): `FPGA_STREAM_PIPE`
    FpgaStreamPipe {
        endpoint: String,
        channel: String,
        depth: usize,
    },
    /// Dynamic Thermal & Frequency Adaptation (Phase 3): `DVFS_THROTTLE`
    DvfsThrottle {
        resource: String,
        old_mhz: u64,
        new_mhz: u64,
        reason: String,
    },
}

impl HirInstruction {
    pub fn opcode_name(&self) -> &'static str {
        match self {
            HirInstruction::ResourceAcquire { .. } => "RESOURCE_ACQUIRE",
            HirInstruction::ResourceRelease { .. } => "RESOURCE_RELEASE",
            HirInstruction::Route { .. } => "ROUTE",
            HirInstruction::Dma { .. } => "DMA",
            HirInstruction::LayoutConvert { .. } => "LAYOUT_CONVERT",
            HirInstruction::ClockSync { .. } => "CLOCK_SYNC",
            HirInstruction::StateTransition { .. } => "STATE_TRANSITION",
            HirInstruction::EventSignal { .. } => "EVENT_SIGNAL",
            HirInstruction::EventWait { .. } => "EVENT_WAIT",
            HirInstruction::MemoryBarrier { .. } => "MEMORY_BARRIER",
            HirInstruction::DeviceCommand { .. } => "DEVICE_COMMAND",
            HirInstruction::NativeCall { .. } => "NATIVE_CALL",
            HirInstruction::DomainAlloc { .. } => "DOMAIN_ALLOC",
            HirInstruction::BudgetAccount { .. } => "BUDGET_ACCOUNT",
            HirInstruction::TracePoint { .. } => "TRACE_POINT",
            HirInstruction::GpuDispatch { .. } => "GPU_DISPATCH",
            HirInstruction::InterruptAttach { .. } => "INTERRUPT_ATTACH",
            HirInstruction::ContentionSchedule { .. } => "CONTENTION_SCHEDULE",
            HirInstruction::RdmaTransfer { .. } => "RDMA_TRANSFER",
            HirInstruction::NpuTensorOp { .. } => "NPU_TENSOR_OP",
            HirInstruction::FpgaStreamPipe { .. } => "FPGA_STREAM_PIPE",
            HirInstruction::DvfsThrottle { .. } => "DVFS_THROTTLE",
        }
    }
}

impl fmt::Display for HirInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HirInstruction::ResourceAcquire {
                dest_var,
                endpoint,
                mode,
            } => write!(
                f,
                "RESOURCE_ACQUIRE  {} = acquire({}, {})",
                dest_var, endpoint, mode
            ),
            HirInstruction::ResourceRelease { var_name, endpoint } => {
                write!(f, "RESOURCE_RELEASE  release({} @ {})", var_name, endpoint)
            }
            HirInstruction::Route {
                route_id,
                planned,
                payload,
            } => write!(
                f,
                "ROUTE #{:<2}         {} [{}] payload=`{}` (bw={:.1}GB/s, lat={}ns)",
                route_id,
                planned.format_path(),
                planned.protocols.join(" + "),
                payload,
                planned.effective_bandwidth_gbps,
                planned.total_latency_ns
            ),
            HirInstruction::Dma {
                engine,
                src_endpoint,
                dst_endpoint,
                payload,
                bytes,
                async_event,
            } => {
                let ev_str = async_event
                    .as_ref()
                    .map(|e| format!(" -> signal({})", e))
                    .unwrap_or_default();
                write!(
                    f,
                    "DMA               engine={} {} -> {} : {} ({} bytes){}",
                    engine, src_endpoint, dst_endpoint, payload, bytes, ev_str
                )
            }
            HirInstruction::LayoutConvert {
                buffer,
                semantic_type,
                from_layout,
                to_layout,
                mechanism,
                estimated_cost_ns,
            } => write!(
                f,
                "LAYOUT_CONVERT    `{}` ({}) : {} ==> {} via {} (cost={}ns)",
                buffer, semantic_type, from_layout, to_layout, mechanism, estimated_cost_ns
            ),
            HirInstruction::ClockSync {
                signal,
                from_domain,
                to_domain,
                primitive,
                latency_cycles,
            } => write!(
                f,
                "CLOCK_SYNC        `{}` ({} -> {}) via {} ({} cycles)",
                signal, from_domain, to_domain, primitive, latency_cycles
            ),
            HirInstruction::StateTransition {
                device_var,
                device_class,
                from_state,
                to_state,
                trigger,
            } => write!(
                f,
                "STATE_TRANSITION  {} ({}) : {} -> {} [on {}]",
                device_var, device_class, from_state, to_state, trigger
            ),
            HirInstruction::EventSignal {
                event_name,
                source_op,
            } => write!(f, "EVENT_SIGNAL      {} (from {})", event_name, source_op),
            HirInstruction::EventWait { event_name } => {
                write!(f, "EVENT_WAIT        await {}", event_name)
            }
            HirInstruction::MemoryBarrier {
                ordering,
                domain_from,
                domain_to,
            } => write!(
                f,
                "MEMORY_BARRIER    order={} ({} -> {})",
                ordering, domain_from, domain_to
            ),
            HirInstruction::DeviceCommand {
                device_var,
                endpoint,
                command,
                verified_state,
            } => write!(
                f,
                "DEVICE_COMMAND    {}.{} @ {} [verified_state={}]",
                device_var, command, endpoint, verified_state
            ),
            HirInstruction::NativeCall {
                dest_var,
                func_name,
                args,
            } => {
                if let Some(d) = dest_var {
                    write!(
                        f,
                        "NATIVE_CALL       {} = {}({})",
                        d,
                        func_name,
                        args.join(", ")
                    )
                } else {
                    write!(f, "NATIVE_CALL       {}({})", func_name, args.join(", "))
                }
            }
            HirInstruction::DomainAlloc {
                var_name,
                domain,
                semantic_type,
                layout,
                bytes,
            } => write!(
                f,
                "DOMAIN_ALLOC      {} : {}<{}>({}) [{} bytes]",
                var_name, domain, semantic_type, layout, bytes
            ),
            HirInstruction::BudgetAccount {
                budget_var,
                allocated_watts,
                remaining_watts,
            } => write!(
                f,
                "BUDGET_ACCOUNT    {} -= {:.1}W (remaining={:.1}W)",
                budget_var, allocated_watts, remaining_watts
            ),
            HirInstruction::TracePoint { label } => {
                write!(f, "TRACE_POINT       \"{}\"", label)
            }
            HirInstruction::GpuDispatch {
                queue,
                kernel_name,
                grid,
                block,
                args,
                estimated_time_ns,
            } => write!(
                f,
                "GPU_DISPATCH      {} -> {}({}) [grid=({},{}), block=({},{}), est={}ns]",
                queue,
                kernel_name,
                args.join(", "),
                grid.0,
                grid.1,
                block.0,
                block.1,
                estimated_time_ns
            ),
            HirInstruction::InterruptAttach {
                endpoint,
                vector,
                handler_name,
            } => write!(
                f,
                "INTERRUPT_ATTACH  vec#{} @ {} -> fn {}()",
                vector, endpoint, handler_name
            ),
            HirInstruction::ContentionSchedule {
                resource,
                concurrent_ops,
                penalty_ns,
            } => write!(
                f,
                "CONTENTION_SCHED  {} : {} concurrent ops (+{}ns penalty)",
                resource, concurrent_ops, penalty_ns
            ),
            HirInstruction::RdmaTransfer {
                src_node,
                dst_node,
                payload,
                bytes,
                verb,
            } => write!(
                f,
                "RDMA_TRANSFER     {} -> {} : {} ({} bytes, verb={})",
                src_node, dst_node, payload, bytes, verb
            ),
            HirInstruction::NpuTensorOp {
                endpoint,
                op_type,
                dims,
                inputs,
                output,
                latency_ns,
            } => write!(
                f,
                "NPU_TENSOR_OP     {}.{}({}) -> {} [dims=({},{},{},{}), lat={}ns]",
                endpoint,
                op_type,
                inputs.join(", "),
                output,
                dims.0,
                dims.1,
                dims.2,
                dims.3,
                latency_ns
            ),
            HirInstruction::FpgaStreamPipe {
                endpoint,
                channel,
                depth,
            } => write!(
                f,
                "FPGA_STREAM_PIPE  {}::{} [fifo_depth={}]",
                endpoint, channel, depth
            ),
            HirInstruction::DvfsThrottle {
                resource,
                old_mhz,
                new_mhz,
                reason,
            } => write!(
                f,
                "DVFS_THROTTLE     {} : {}MHz -> {}MHz [{}]",
                resource, old_mhz, new_mhz, reason
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UniversalHirModule {
    pub instructions: Vec<HirInstruction>,
    pub optimizations_applied: Vec<String>,
}

impl UniversalHirModule {
    pub fn dump(&self) -> String {
        let mut lines = Vec::new();
        lines.push(
            "=== Universal Hardware Intermediate Representation (Universal HIR) ===".to_string(),
        );
        for (idx, inst) in self.instructions.iter().enumerate() {
            lines.push(format!("  {:03}: {}", idx, inst));
        }
        if !self.optimizations_applied.is_empty() {
            lines.push("\n--- Hardware Optimizations Applied ---".to_string());
            for opt in &self.optimizations_applied {
                lines.push(format!("  * {}", opt));
            }
        }
        lines.join("\n")
    }

    /// Two-level Hardware HIR Optimizer (Section 4 & 11):
    /// In `OptLevel::Max`, eliminates redundant back-and-forth `LAYOUT_CONVERT` operations
    /// (e.g. `Tiled2D -> Linear` immediately followed by `Linear -> Tiled2D` on the same buffer)
    /// and coalesces consecutive memory barriers on identical domains.
    pub fn optimize(&mut self, level: OptLevel) {
        if level == OptLevel::Fast {
            return;
        }

        // Pass 1: Eliminate redundant back-and-forth LAYOUT_CONVERT pairs (Section 4)
        let mut i = 0;
        while i + 1 < self.instructions.len() {
            let cancel_pair = match (&self.instructions[i], &self.instructions[i + 1]) {
                (
                    HirInstruction::LayoutConvert {
                        buffer: b1,
                        from_layout: f1,
                        to_layout: t1,
                        ..
                    },
                    HirInstruction::LayoutConvert {
                        buffer: b2,
                        from_layout: f2,
                        to_layout: t2,
                        ..
                    },
                ) if b1 == b2 && t1 == f2 && f1 == t2 => Some((b1.clone(), f1.clone(), t1.clone())),
                _ => None,
            };

            if let Some((buf, l_keep, l_temp)) = cancel_pair {
                self.optimizations_applied.push(format!(
                    "Elided redundant round-trip LAYOUT_CONVERT on `{}` ({} -> {} -> {}); kept in `{}` layout",
                    buf, l_keep, l_temp, l_keep, l_keep
                ));
                self.instructions.remove(i + 1);
                self.instructions.remove(i);
            } else {
                i += 1;
            }
        }

        // Pass 2: Coalesce back-to-back identical MEMORY_BARRIER instructions
        let mut j = 0;
        while j + 1 < self.instructions.len() {
            let duplicate_barrier = match (&self.instructions[j], &self.instructions[j + 1]) {
                (
                    HirInstruction::MemoryBarrier {
                        ordering: o1,
                        domain_from: df1,
                        domain_to: dt1,
                    },
                    HirInstruction::MemoryBarrier {
                        ordering: o2,
                        domain_from: df2,
                        domain_to: dt2,
                    },
                ) if o1 == o2 && df1 == df2 && dt1 == dt2 => Some(format!("{} -> {}", df1, dt1)),
                _ => None,
            };
            if let Some(dom_pair) = duplicate_barrier {
                self.optimizations_applied.push(format!(
                    "Coalesced adjacent MEMORY_BARRIER instructions across ({})",
                    dom_pair
                ));
                self.instructions.remove(j + 1);
            } else {
                j += 1;
            }
        }

        // Pass 3: Multi-Resource Contention Solver (Section 9 & 11)
        // Detects concurrent requests contending for the same DMA engine, PCIe bus, or GPU queue
        let mut dma_count = 0usize;
        let mut gpu_count = 0usize;
        for inst in &self.instructions {
            match inst {
                HirInstruction::Dma { .. } => dma_count += 1,
                HirInstruction::GpuDispatch { .. } => gpu_count += 1,
                _ => {}
            }
        }

        if dma_count >= 2 {
            self.optimizations_applied.push(format!(
                "Contention Solver: Detected {} concurrent DMA transfers sharing PCIe fabric; scheduled interleaving (+{}ns serialization)",
                dma_count, dma_count * 220
            ));
            self.instructions.push(HirInstruction::ContentionSchedule {
                resource: "PCIe_DMA_Fabric".to_string(),
                concurrent_ops: dma_count,
                penalty_ns: (dma_count as u64) * 220,
            });
        }

        if gpu_count >= 2 {
            self.optimizations_applied.push(format!(
                "Contention Solver: Detected {} concurrent GPU queue dispatches; serialized queue submission (+{}ns)",
                gpu_count, gpu_count * 150
            ));
            self.instructions.push(HirInstruction::ContentionSchedule {
                resource: "/F3::queue".to_string(),
                concurrent_ops: gpu_count,
                penalty_ns: (gpu_count as u64) * 150,
            });
        }
    }
}
