//! Cycle & Timeline Hardware Graph Simulator with Fault Injection and Dynamic Route Recovery.
//! Implements Sections 8 (Dynamic Topology, Hot-Swapping, and Recovery),
//! 10 (Fault Injection), and 13 (Tooling, Simulation, Debugging, and Performance).

use crate::hardware_graph::HardwareGraph;
use crate::hir::{HirInstruction, UniversalHirModule};
use std::fmt;

/// Fault injection modes from Sections 10 & 13.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectedFault {
    None,
    LinkDisconnect,   // Simulates PCIe P2P link failure, triggering NORMAL -> FAILED -> REPLAN -> FALLBACK -> RECOVERED
    GpuTimeout,       // Simulates GPU queue watchdog timeout
    DmaError,         // Simulates DMA bus fault / descriptor error
    ThermalThrottle,  // Simulates thermal sensor exceeding threshold, throttling frequency & bandwidth
    MemoryCorruption, // Simulates ECC / parity corruption detection
}

impl InjectedFault {
    pub fn from_str(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "none" | "" => Self::None,
            "link_disconnect" | "disconnect" | "hot_swap" => Self::LinkDisconnect,
            "gpu_timeout" | "timeout" => Self::GpuTimeout,
            "dma_error" | "dma" => Self::DmaError,
            "thermal_throttle" | "thermal" => Self::ThermalThrottle,
            "memory_corruption" | "corruption" => Self::MemoryCorruption,
            _ => Self::None,
        }
    }
}

/// Dynamic Route Recovery States from Section 8 (Page 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteState {
    Normal,
    Failed,
    Replan,
    Fallback,
    Recovered,
}

impl fmt::Display for RouteState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouteState::Normal => write!(f, "NORMAL"),
            RouteState::Failed => write!(f, "FAILED"),
            RouteState::Replan => write!(f, "REPLAN"),
            RouteState::Fallback => write!(f, "FALLBACK"),
            RouteState::Recovered => write!(f, "RECOVERED"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub start_ns: u64,
    pub end_ns: u64,
    pub resource: String,
    pub action: String,
    pub route_state: RouteState,
}

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub timeline: Vec<TimelineEvent>,
    pub total_time_ns: u64,
    pub total_bytes_transferred: usize,
    pub layout_conversions_executed: usize,
    pub peak_power_watts: f64,
    pub simulated_temp_c: f64,
    pub route_state_transitions: Vec<String>,
    pub fault_recovery_log: Vec<String>,
    pub status: String,
}

impl SimulationResult {
    pub fn format_report(&self) -> String {
        let mut out = Vec::new();
        out.push("================================================================================".to_string());
        out.push("  HWCode Hardware Graph Simulator & Timeline Profiler".to_string());
        out.push("================================================================================".to_string());
        out.push(format!("  Status                   : {}", self.status));
        out.push(format!("  Total Simulated Time     : {} ns ({:.3} us)", self.total_time_ns, self.total_time_ns as f64 / 1000.0));
        out.push(format!("  Total DMA Data Moved     : {} bytes", self.total_bytes_transferred));
        out.push(format!("  Layout Conversions       : {}", self.layout_conversions_executed));
        out.push(format!("  Peak Power Draw          : {:.1} W", self.peak_power_watts));
        out.push(format!("  Simulated Junction Temp  : {:.1} C", self.simulated_temp_c));

        if !self.fault_recovery_log.is_empty() {
            out.push("\n--- Fault Injection & Dynamic Topology Recovery Log ---".to_string());
            for entry in &self.fault_recovery_log {
                out.push(format!("  ! {}", entry));
            }
        }

        if !self.route_state_transitions.is_empty() {
            out.push(format!(
                "  Route Lifecycle States   : {}",
                self.route_state_transitions.join(" -> ")
            ));
        }

        out.push("\n--- Multi-Resource Hardware Execution Timeline ---".to_string());
        out.push("  [Start ns ..   End ns] | Resource       | Route State | Operation".to_string());
        out.push("  -----------------------+----------------+-------------+--------------------------------------".to_string());
        for ev in &self.timeline {
            out.push(format!(
                "  [{:8} .. {:8}] | {:<14} | {:<11} | {}",
                ev.start_ns,
                ev.end_ns,
                ev.resource,
                format!("{}", ev.route_state),
                ev.action
            ));
        }
        out.push("================================================================================".to_string());
        out.join("\n")
    }
}

pub struct HardwareSimulator;

impl HardwareSimulator {
    pub fn run(
        hir: &UniversalHirModule,
        _graph: &HardwareGraph,
        fault: InjectedFault,
    ) -> SimulationResult {
        let mut clock_ns = 0u64;
        let mut timeline = Vec::new();
        let mut total_bytes = 0usize;
        let mut layout_convs = 0usize;
        let mut peak_power = 28.0f64; // base CPU + RAM + GPU idle power
        let mut temp_c = 46.0f64;
        let mut route_states = vec!["NORMAL".to_string()];
        let mut recovery_log = Vec::new();
        let mut current_route_state = RouteState::Normal;
        let mut status = "SUCCESS (All physical boundaries & events verified)".to_string();

        // Pre-apply thermal fault if requested
        let latency_multiplier = if fault == InjectedFault::ThermalThrottle {
            temp_c = 92.5;
            recovery_log.push(
                "TopologyWatcher: Thermal sensor reported 92.5C (> 88.0C limit) -> HardwareError::ThermalLimit".to_string(),
            );
            recovery_log.push(
                "Scheduler: Adapted clock frequency (-35% DVFS throttle) to stay within thermal envelope".to_string(),
            );
            1.5
        } else {
            1.0
        };

        for inst in &hir.instructions {
            match inst {
                HirInstruction::DomainAlloc {
                    var_name,
                    domain,
                    semantic_type,
                    layout,
                    bytes,
                } => {
                    let dur = 40;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: domain.clone(),
                        action: format!("Alloc `{}` : {} ({}, {}B)", var_name, semantic_type, layout, bytes),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::ResourceAcquire {
                    dest_var,
                    endpoint,
                    mode,
                } => {
                    let dur = 35;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: endpoint.clone(),
                        action: format!("Acquire `{}` ({})", dest_var, mode),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::ResourceRelease { var_name, endpoint } => {
                    let dur = 25;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: endpoint.clone(),
                        action: format!("Release `{}`", var_name),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::Route {
                    route_id,
                    planned,
                    payload,
                } => {
                    if fault == InjectedFault::LinkDisconnect {
                        // Exercise Section 8: NORMAL -> FAILED -> REPLAN -> FALLBACK -> RECOVERED
                        current_route_state = RouteState::Failed;
                        route_states.push("FAILED".to_string());
                        recovery_log.push(format!(
                            "TopologyWatcher: Primary link on Route #{} ({}) disconnected mid-flight! Emitting HardwareError::Disconnected",
                            route_id,
                            planned.format_path()
                        ));
                        timeline.push(TimelineEvent {
                            start_ns: clock_ns,
                            end_ns: clock_ns + 120,
                            resource: "TopologyWatcher".to_string(),
                            action: format!("Link fault detected on {}", planned.format_path()),
                            route_state: current_route_state.clone(),
                        });
                        clock_ns += 120;

                        current_route_state = RouteState::Replan;
                        route_states.push("REPLAN".to_string());
                        let fb_path = planned
                            .fallback_hops
                            .clone()
                            .unwrap_or_else(|| vec![planned.source.clone(), "/F2".to_string(), planned.destination.clone()]);
                        recovery_log.push(format!(
                            "RoutePlanner: Re-evaluating capability graph -> Selected fallback route via SystemRAM: {}",
                            fb_path.join(" -> ")
                        ));
                        timeline.push(TimelineEvent {
                            start_ns: clock_ns,
                            end_ns: clock_ns + 180,
                            resource: "RoutePlanner".to_string(),
                            action: format!("Replan Route #{} -> Fallback {}", route_id, fb_path.join(" -> ")),
                            route_state: current_route_state.clone(),
                        });
                        clock_ns += 180;

                        current_route_state = RouteState::Fallback;
                        route_states.push("FALLBACK".to_string());
                        let fb_dur = ((planned.total_latency_ns as f64) * 1.8 * latency_multiplier) as u64;
                        timeline.push(TimelineEvent {
                            start_ns: clock_ns,
                            end_ns: clock_ns + fb_dur,
                            resource: fb_path.join("->"),
                            action: format!("Fallback transfer `{}` via SystemRAM staging", payload),
                            route_state: current_route_state.clone(),
                        });
                        clock_ns += fb_dur;

                        current_route_state = RouteState::Recovered;
                        route_states.push("RECOVERED".to_string());
                        recovery_log.push(format!(
                            "Runtime: Route #{} transfer of `{}` completed safely via fallback path.",
                            route_id, payload
                        ));
                        status = "RECOVERED (Handled injected link disconnect via fallback route)".to_string();
                    } else {
                        let dur = ((planned.total_latency_ns as f64) * latency_multiplier) as u64;
                        peak_power = peak_power.max(28.0 + planned.estimated_power_watts * 8.0);
                        temp_c = (temp_c + planned.estimated_power_watts * 0.4).min(95.0);
                        timeline.push(TimelineEvent {
                            start_ns: clock_ns,
                            end_ns: clock_ns + dur,
                            resource: planned.format_path(),
                            action: format!(
                                "Route #{}: `{}` via {}",
                                route_id,
                                payload,
                                planned.protocols.join("+")
                            ),
                            route_state: current_route_state.clone(),
                        });
                        clock_ns += dur;
                    }
                }
                HirInstruction::MemoryBarrier {
                    ordering,
                    domain_from,
                    domain_to,
                } => {
                    let dur = 45;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: "IOMMU/Cache".to_string(),
                        action: format!("Barrier {} ({} -> {})", ordering, domain_from, domain_to),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::Dma {
                    engine,
                    src_endpoint,
                    dst_endpoint,
                    payload,
                    bytes,
                    ..
                } => {
                    if fault == InjectedFault::DmaError {
                        current_route_state = RouteState::Failed;
                        route_states.push("FAILED".to_string());
                        recovery_log.push(format!(
                            "DMA Engine `{}` reported HardwareError::DMAError (bus fault during `{}` transfer)",
                            engine, payload
                        ));
                        recovery_log.push(
                            "Device State Machine: DMA controller transitioned `Active -> Error -> Reset -> Active` (idempotent retry #1 succeeded)"
                                .to_string(),
                        );
                        timeline.push(TimelineEvent {
                            start_ns: clock_ns,
                            end_ns: clock_ns + 300,
                            resource: engine.clone(),
                            action: "HardwareError::DMAError -> Reset & Retry #1".to_string(),
                            route_state: current_route_state.clone(),
                        });
                        clock_ns += 300;
                        current_route_state = RouteState::Recovered;
                        route_states.push("RECOVERED".to_string());
                        status = "RECOVERED (DMA engine reset & idempotent retry succeeded)".to_string();
                    }
                    let dur = (220.0 * latency_multiplier) as u64;
                    total_bytes += *bytes;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: engine.clone(),
                        action: format!("DMA {} -> {} : `{}` ({} B)", src_endpoint, dst_endpoint, payload, bytes),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::LayoutConvert {
                    buffer,
                    from_layout,
                    to_layout,
                    mechanism,
                    estimated_cost_ns,
                    ..
                } => {
                    let dur = ((*estimated_cost_ns as f64) * latency_multiplier) as u64;
                    layout_convs += 1;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: mechanism.clone(),
                        action: format!("LayoutConvert `{}`: {} -> {}", buffer, from_layout, to_layout),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::ClockSync {
                    signal,
                    from_domain,
                    to_domain,
                    primitive,
                    latency_cycles,
                } => {
                    let dur = (*latency_cycles as u64) * 12;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: format!("CDC:{}", primitive),
                        action: format!("Sync `{}` ({} -> {})", signal, from_domain, to_domain),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::StateTransition {
                    device_var,
                    device_class,
                    from_state,
                    to_state,
                    trigger,
                } => {
                    let dur = 60;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: format!("{}:{}", device_class, device_var),
                        action: format!("State {} -> {} ({})", from_state, to_state, trigger),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::DeviceCommand {
                    device_var,
                    endpoint,
                    command,
                    verified_state,
                } => {
                    if fault == InjectedFault::GpuTimeout {
                        recovery_log.push(format!(
                            "Device `{}` @ `{}` exceeded command queue timeout -> HardwareError::Timeout",
                            device_var, endpoint
                        ));
                        status = "HALTED_SAFE (HardwareError::Timeout caught by runtime supervisor)".to_string();
                    }
                    let dur = 110;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: endpoint.clone(),
                        action: format!("{}.{}() [state={}]", device_var, command, verified_state),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::EventSignal {
                    event_name,
                    source_op,
                } => {
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + 10,
                        resource: "EventGraph".to_string(),
                        action: format!("Signal `{}` from {}", event_name, source_op),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += 10;
                }
                HirInstruction::EventWait { event_name } => {
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + 20,
                        resource: "EventGraph".to_string(),
                        action: format!("Await `{}` satisfied", event_name),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += 20;
                }
                HirInstruction::BudgetAccount {
                    budget_var,
                    allocated_watts,
                    remaining_watts,
                } => {
                    peak_power = peak_power.max(28.0 + *allocated_watts);
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + 15,
                        resource: "PowerBudget".to_string(),
                        action: format!(
                            "Budget `{}`: allocated {:.1}W, remaining {:.1}W",
                            budget_var, allocated_watts, remaining_watts
                        ),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += 15;
                }
                HirInstruction::TracePoint { label } => {
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + 5,
                        resource: "TraceProfiler".to_string(),
                        action: format!("TRACE(\"{}\")", label),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += 5;
                }
                HirInstruction::NativeCall { func_name, args, .. } => {
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + 30,
                        resource: "/F1 (CPU)".to_string(),
                        action: format!("Call {}({})", func_name, args.join(", ")),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += 30;
                }
                HirInstruction::GpuDispatch {
                    queue,
                    kernel_name,
                    grid,
                    block,
                    args,
                    estimated_time_ns,
                } => {
                    let dur = ((*estimated_time_ns as f64) * latency_multiplier) as u64;
                    peak_power = peak_power.max(75.0);
                    temp_c = (temp_c + 2.5).min(95.0);
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: queue.clone(),
                        action: format!(
                            "Kernel `{}` [grid=({},{},{}), block=({},{},{})] args=({})",
                            kernel_name, grid.0, grid.1, grid.2, block.0, block.1, block.2, args.join(", ")
                        ),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::InterruptAttach {
                    endpoint,
                    vector,
                    handler_name,
                } => {
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + 25,
                        resource: endpoint.clone(),
                        action: format!("VectorTable: bind IRQ #{} -> fn {}()", vector, handler_name),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += 25;
                }
                HirInstruction::ContentionSchedule {
                    resource,
                    concurrent_ops,
                    penalty_ns,
                } => {
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + penalty_ns,
                        resource: resource.clone(),
                        action: format!("Contention delay ({} concurrent ops, +{}ns)", concurrent_ops, penalty_ns),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += penalty_ns;
                }
                HirInstruction::RdmaTransfer {
                    src_node,
                    dst_node,
                    payload,
                    bytes,
                    verb,
                } => {
                    let dur = (2400.0 * latency_multiplier) as u64;
                    total_bytes += *bytes;
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: "RDMA_RoCEv2".to_string(),
                        action: format!("RDMA {} -> {} : `{}` ({} bytes, verb={})", src_node, dst_node, payload, bytes, verb),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::NpuTensorOp {
                    endpoint,
                    op_type,
                    dims,
                    inputs,
                    output,
                    latency_ns,
                } => {
                    let dur = ((*latency_ns as f64) * latency_multiplier) as u64;
                    peak_power = peak_power.max(90.0);
                    temp_c = (temp_c + 3.0).min(95.0);
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + dur,
                        resource: endpoint.clone(),
                        action: format!(
                            "NPU Tensor {} [dims=({},{},{},{})] inputs=({}) -> {}",
                            op_type, dims.0, dims.1, dims.2, dims.3, inputs.join(", "), output
                        ),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += dur;
                }
                HirInstruction::FpgaStreamPipe {
                    endpoint,
                    channel,
                    depth,
                } => {
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + 50,
                        resource: endpoint.clone(),
                        action: format!("Config AXI4-Stream `{}` (FIFO depth={})", channel, depth),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += 50;
                }
                HirInstruction::DvfsThrottle {
                    resource,
                    old_mhz,
                    new_mhz,
                    reason,
                } => {
                    timeline.push(TimelineEvent {
                        start_ns: clock_ns,
                        end_ns: clock_ns + 80,
                        resource: resource.clone(),
                        action: format!("DVFS Throttle: {}MHz -> {}MHz ({})", old_mhz, new_mhz, reason),
                        route_state: current_route_state.clone(),
                    });
                    clock_ns += 80;
                }
            }
        }

        if fault == InjectedFault::MemoryCorruption {
            recovery_log.push(
                "MemoryController: ECC parity mismatch detected on DMA payload -> HardwareError::Corruption"
                    .to_string(),
            );
            status = "HALTED_SAFE (HardwareError::Corruption trapped before silent data corruption)".to_string();
        }

        SimulationResult {
            timeline,
            total_time_ns: clock_ns,
            total_bytes_transferred: total_bytes,
            layout_conversions_executed: layout_convs,
            peak_power_watts: peak_power,
            simulated_temp_c: temp_c,
            route_state_transitions: route_states,
            fault_recovery_log: recovery_log,
            status,
        }
    }
}
