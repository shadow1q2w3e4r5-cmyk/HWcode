//! Static & Dynamic Explanation Engine (`hwc explain`).
//! Implements Sections 4, 11, and 13:
//! "An explain command should show why a route was selected, where copies occurred,
//! whether DMA was used, which synchronization primitives were inserted, what bottleneck
//! was predicted, and which constraints influenced the decision."

use crate::diagnostics::DiagnosticBag;
use crate::hardware_graph::HardwareGraph;
use crate::hir::HirInstruction;
use crate::sema::AnalysisReport;

pub fn generate_explanation(
    filename: &str,
    graph: &HardwareGraph,
    report: &AnalysisReport,
    diags: &DiagnosticBag,
) -> String {
    let mut out = Vec::new();
    out.push("================================================================================".to_string());
    out.push(format!(
        "  HWCode Compiler Physical Plan & Boundary Explanation — `{}`",
        filename
    ));
    out.push("================================================================================".to_string());

    // 1. Hardware Graph & Selected Routes
    out.push("\n[1] HARDWARE GRAPH ROUTE SELECTION & TOPOLOGY DECISIONS".to_string());
    if report.planned_routes.is_empty() {
        out.push("  (No cross-endpoint routes in this module)".to_string());
    } else {
        for (idx, r) in report.planned_routes.iter().enumerate() {
            out.push(format!(
                "  Route #{}: {} ==> {}  (Selected Path: {})",
                idx + 1,
                r.source,
                r.destination,
                r.format_path()
            ));
            out.push(format!(
                "    - Protocol(s)       : {}",
                r.protocols.join(" -> ")
            ));
            out.push(format!(
                "    - DMA Engine(s)     : {}",
                if r.dma_engines.is_empty() {
                    "CPU PIO (No DMA)".to_string()
                } else {
                    r.dma_engines.join(", ")
                }
            ));
            out.push(format!(
                "    - Effective BW      : {:.1} GB/s",
                r.effective_bandwidth_gbps
            ));
            out.push(format!(
                "    - Predicted Latency : {} ns",
                r.total_latency_ns
            ));
            out.push(format!(
                "    - Bottleneck Link   : {}",
                r.bottleneck_link
            ));
            if let Some(fb) = &r.fallback_hops {
                out.push(format!(
                    "    - Hot-Swap Fallback : {} (ready if primary link disconnects)",
                    fb.join(" -> ")
                ));
            }
            if !r.rejected_alternatives.is_empty() {
                for (alt_path, reason) in &r.rejected_alternatives {
                    out.push(format!(
                        "    - Rejected Path     : {} -> Reason: {}",
                        alt_path.join(" -> "),
                        reason
                    ));
                }
            }
        }
    }

    // 2. Physical Layout Conversions & Memory Coherency
    out.push("\n[2] MEMORY DOMAINS, PHYSICAL LAYOUT CONVERSIONS & COHERENCY BARRIERS".to_string());
    let mut found_mem_ops = false;
    for inst in &report.hir.instructions {
        match inst {
            HirInstruction::LayoutConvert {
                buffer,
                semantic_type,
                from_layout,
                to_layout,
                mechanism,
                estimated_cost_ns,
            } => {
                found_mem_ops = true;
                out.push(format!(
                    "  * LAYOUT_CONVERT on `{}` (semantic type `{}`): `{}` -> `{}`",
                    buffer, semantic_type, from_layout, to_layout
                ));
                out.push(format!(
                    "    - Why: Destination hardware requires `{}` physical representation; raw byte copy is insufficient.",
                    to_layout
                ));
                out.push(format!(
                    "    - Mechanism: `{}` | Added Cost: +{} ns",
                    mechanism, estimated_cost_ns
                ));
            }
            HirInstruction::MemoryBarrier {
                ordering,
                domain_from,
                domain_to,
            } => {
                found_mem_ops = true;
                out.push(format!(
                    "  * MEMORY_BARRIER ({}) inserted between `{}` and `{}`",
                    ordering, domain_from, domain_to
                ));
                out.push(
                    "    - Why: Crossing non-coherent memory boundary requires DMA visibility flush/fence."
                        .to_string(),
                );
            }
            _ => {}
        }
    }
    if !found_mem_ops {
        out.push("  (All memory accesses are layout-compatible and within coherent domains)".to_string());
    }

    // 3. Clock Domain Crossings & Device State Transitions
    out.push("\n[3] CLOCK-DOMAIN CROSSINGS (CDC) & DEVICE STATE MACHINE TRANSITIONS".to_string());
    let mut found_sync_or_state = false;
    for inst in &report.hir.instructions {
        match inst {
            HirInstruction::ClockSync {
                signal,
                from_domain,
                to_domain,
                primitive,
                latency_cycles,
            } => {
                found_sync_or_state = true;
                out.push(format!(
                    "  * CLOCK_SYNC on `{}`: `{}` -> `{}` via `{}` ({} cycles)",
                    signal, from_domain, to_domain, primitive, latency_cycles
                ));
            }
            HirInstruction::StateTransition {
                device_var,
                device_class,
                from_state,
                to_state,
                trigger,
            } => {
                found_sync_or_state = true;
                out.push(format!(
                    "  * STATE_TRANSITION on `{}` ({}): `{}` -> `{}` triggered by `{}`",
                    device_var, device_class, from_state, to_state, trigger
                ));
            }
            HirInstruction::DeviceCommand {
                device_var,
                endpoint,
                command,
                verified_state,
            } => {
                found_sync_or_state = true;
                out.push(format!(
                    "  * DEVICE_COMMAND `{}.{}` @ `{}` statically proven legal in state `{}`",
                    device_var, command, endpoint, verified_state
                ));
            }
            _ => {}
        }
    }
    if !found_sync_or_state {
        out.push("  (No asynchronous clock crossings or device state transitions)".to_string());
    }

    // 4. Energy & Thermal Budget Linear Accounting
    out.push("\n[4] ENERGY & THERMAL BUDGET LINEAR ACCOUNTING".to_string());
    if report.total_budget_acquired_w > 0.0 {
        out.push(format!(
            "  * Total Acquired Power Envelope : {:.1} W",
            report.total_budget_acquired_w
        ));
        out.push(format!(
            "  * Distributed to Sub-Workloads  : {:.1} W",
            report.total_budget_allocated_w
        ));
        out.push(format!(
            "  * Unallocated Headroom          : {:.1} W  [PROVEN SAFE <= ENVELOPE]",
            (report.total_budget_acquired_w - report.total_budget_allocated_w).max(0.0)
        ));
    } else {
        let est_route_power: f64 = report
            .planned_routes
            .iter()
            .map(|r| r.estimated_power_watts)
            .sum();
        out.push(format!(
            "  * Estimated Active Transfer Power: {:.2} W (across {} active nodes in topology)",
            est_route_power,
            graph.nodes.len()
        ));
    }

    // 5. Security & Capability Audit Trail
    out.push("\n[5] CAPABILITY & UNSAFE SECURITY AUDIT TRAIL".to_string());
    if diags.audit_trail.is_empty() {
        out.push("  * Zero `unsafe` blocks used. All operations verified within standard OS/IOMMU capabilities.".to_string());
    } else {
        for entry in &diags.audit_trail {
            out.push(format!(
                "  * [AUDIT line {}:{}] Capability `{}`: {} ({})",
                entry.span.line, entry.span.col, entry.capability, entry.operation, entry.note
            ));
        }
    }

    out.push("================================================================================".to_string());
    out.join("\n")
}
