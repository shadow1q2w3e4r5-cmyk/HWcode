//! Multi-Pass Semantic & Physical Boundary Analyzer for HWCode.
//! Enforces the 7 Physical Boundaries (Memory/Layout, Device/Ownership, Clock/CDC,
//! State Machine, Topology/Route, Power/Thermal Budget, and Temporal/@realtime)
//! and lowers verified AST into Universal HIR.

use crate::ast::*;
use crate::diagnostics::{DiagnosticBag, HardwareErrorKind, PhysicalBoundary, Span};
use crate::hardware_graph::{
    ClockRelation, DeviceStateMachine, HardwareGraph, PlannedRoute, StateTransitionRule,
};
use crate::hir::{HirInstruction, UniversalHirModule};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipState {
    OwnedExclusive,
    Shared,
    Moved { to: String, at: Span },
    Released { at: Span },
}

#[derive(Debug, Clone)]
pub struct VarInfo {
    pub name: String,
    pub semantic_type: String,
    pub domain: Option<String>,
    pub layout: Option<String>,
    pub endpoint: Option<String>,
    pub clock_domain: Option<String>,
    pub device_class: Option<String>,
    pub device_state: Option<String>,
    pub ownership: OwnershipState,
    pub is_synchronized_signal: bool,
    pub budget_remaining_watts: Option<f64>,
    pub budget_total_watts: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct AnalysisReport {
    pub hir: UniversalHirModule,
    pub planned_routes: Vec<PlannedRoute>,
    pub vars: HashMap<String, VarInfo>,
    pub total_budget_acquired_w: f64,
    pub total_budget_allocated_w: f64,
}

pub struct SemanticAnalyzer<'a> {
    pub graph: &'a mut HardwareGraph,
    pub vars: HashMap<String, VarInfo>,
    pub layouts: HashMap<String, Vec<(String, i64)>>,
    pub hir: UniversalHirModule,
    pub planned_routes: Vec<PlannedRoute>,
    pub route_counter: usize,
    pub in_unsafe_block: bool,
    pub total_budget_acquired_w: f64,
    pub total_budget_allocated_w: f64,
}

impl<'a> SemanticAnalyzer<'a> {
    pub fn new(graph: &'a mut HardwareGraph) -> Self {
        let mut layouts = HashMap::new();
        layouts.insert("Linear".to_string(), vec![("stride".to_string(), 1)]);
        layouts.insert("RowMajor".to_string(), vec![("stride".to_string(), 1)]);
        layouts.insert("ColMajor".to_string(), vec![("stride".to_string(), 1)]);
        layouts.insert(
            "Tiled2D".to_string(),
            vec![("block_x".to_string(), 16), ("block_y".to_string(), 16)],
        );
        layouts.insert("Packed".to_string(), vec![("align".to_string(), 64)]);
        layouts.insert("Swizzled".to_string(), vec![("bank_xor".to_string(), 4)]);
        layouts.insert(
            "Tensor4D".to_string(),
            vec![
                ("n".to_string(), 1),
                ("c".to_string(), 64),
                ("h".to_string(), 16),
                ("w".to_string(), 16),
            ],
        );

        Self {
            graph,
            vars: HashMap::new(),
            layouts,
            hir: UniversalHirModule::default(),
            planned_routes: Vec::new(),
            route_counter: 0,
            in_unsafe_block: false,
            total_budget_acquired_w: 0.0,
            total_budget_allocated_w: 0.0,
        }
    }

    pub fn analyze_program(&mut self, prog: &Program, diags: &mut DiagnosticBag) -> AnalysisReport {
        // 1. Register user-defined layouts (Section 4)
        for l in &prog.layouts {
            self.layouts.insert(l.name.clone(), l.properties.clone());
        }

        // 2. Register companion Hardware Definition Language `device` declarations (Section 7 & 12)
        for d in &prog.devices {
            let mut op_reqs = HashMap::new();
            for (op, req) in &d.operations {
                op_reqs.insert(op.clone(), req.clone());
            }
            let transitions = d
                .transitions
                .iter()
                .map(|(f, t, trig)| StateTransitionRule {
                    from_state: f.clone(),
                    to_state: t.clone(),
                    trigger: trig.clone(),
                })
                .collect();
            let sm = DeviceStateMachine {
                device_name: d.name.clone(),
                states: if d.states.is_empty() {
                    vec!["Idle".to_string(), "Active".to_string()]
                } else {
                    d.states.clone()
                },
                initial_state: d
                    .initial_state
                    .clone()
                    .unwrap_or_else(|| "Idle".to_string()),
                transitions,
                operation_requirements: op_reqs,
                registers: d.registers.clone(),
            };
            // Attach to matching class node in the HardwareGraph if present
            for node in self.graph.nodes.values_mut() {
                if node.class.to_string().eq_ignore_ascii_case(&d.name)
                    || node.name.contains(&d.name)
                {
                    node.state_machine = Some(sm.clone());
                }
            }
        }

        // 3. Analyze top-level statements
        for stmt in &prog.top_level_stmts {
            self.analyze_stmt(stmt, None, diags);
        }

        // 4. Analyze functions (including @realtime constraint checking)
        for func in &prog.functions {
            self.analyze_fn(func, diags);
        }

        AnalysisReport {
            hir: self.hir.clone(),
            planned_routes: self.planned_routes.clone(),
            vars: self.vars.clone(),
            total_budget_acquired_w: self.total_budget_acquired_w,
            total_budget_allocated_w: self.total_budget_allocated_w,
        }
    }

    fn analyze_fn(&mut self, func: &FnDecl, diags: &mut DiagnosticBag) {
        // Check for @realtime attribute (Section 6 & 9)
        let mut realtime_deadline_ns: Option<u64> = None;
        let mut realtime_no_alloc = false;

        for attr in &func.attributes {
            if attr.name == "realtime" {
                for (k, v) in &attr.args {
                    if k == "deadline" {
                        realtime_deadline_ns = Some(Self::parse_time_to_ns(v));
                    }
                    if k == "no_alloc" && v == "true" {
                        realtime_no_alloc = true;
                    }
                }
            }
        }

        // Bind function parameters into scope
        for (pname, pty) in &func.params {
            self.vars.insert(
                pname.clone(),
                VarInfo {
                    name: pname.clone(),
                    semantic_type: pty.base_type.clone(),
                    domain: pty.domain.clone(),
                    layout: pty.layout_or_state.clone(),
                    endpoint: None,
                    clock_domain: Some("CPU_CLK".to_string()),
                    device_class: None,
                    device_state: pty.layout_or_state.clone(),
                    ownership: OwnershipState::Shared,
                    is_synchronized_signal: false,
                    budget_remaining_watts: None,
                    budget_total_watts: None,
                },
            );
        }

        let hir_start_len = self.hir.instructions.len();

        for stmt in &func.body {
            if realtime_no_alloc {
                if let Stmt::Let {
                    value: Expr::DomainAlloc { .. },
                    span,
                    ..
                } = stmt
                {
                    diags.emit_error(
                        "E0602",
                        HardwareErrorKind::RealtimeViolation,
                        PhysicalBoundary::Temporal,
                        *span,
                        format!(
                            "Dynamic domain buffer allocation is forbidden inside `@realtime(no_alloc: true)` function `{}`",
                            func.name
                        ),
                        Some("Real-time functions with `no_alloc: true` cannot perform unbounded memory allocation.".to_string()),
                        Some("Pre-allocate the buffer outside the `@realtime` function and pass it by reference.".to_string()),
                    );
                }
            }
            self.analyze_stmt(stmt, Some(&func.name), diags);
        }

        // Verify @realtime deadline against scheduled HIR operations inside this function
        if let Some(deadline_ns) = realtime_deadline_ns {
            let mut predicted_ns = 150u64; // base function prologue/epilogue
            for inst in &self.hir.instructions[hir_start_len..] {
                match inst {
                    HirInstruction::Route { planned, .. } => {
                        predicted_ns += planned.total_latency_ns;
                    }
                    HirInstruction::LayoutConvert {
                        estimated_cost_ns, ..
                    } => {
                        predicted_ns += *estimated_cost_ns;
                    }
                    HirInstruction::Dma { bytes, .. } => {
                        predicted_ns += (*bytes as u64) / 16 + 500;
                    }
                    _ => {
                        predicted_ns += 25;
                    }
                }
            }
            if predicted_ns > deadline_ns {
                diags.emit_error(
                    "E0603",
                    HardwareErrorKind::RealtimeViolation,
                    PhysicalBoundary::Temporal,
                    func.span,
                    format!(
                        "`@realtime` function `{}` predicted execution time ({} ns) exceeds declared deadline ({} ns)",
                        func.name, predicted_ns, deadline_ns
                    ),
                    Some("The scheduled hardware route and layout conversion latencies exceed the temporal guarantee.".to_string()),
                    Some("Increase the deadline, use a direct P2P DMA route, or pre-convert buffer layouts.".to_string()),
                );
            }
        }
    }

    fn parse_time_to_ns(raw: &str) -> u64 {
        if let Some(ms) = raw.strip_suffix("ms") {
            ms.parse::<u64>().unwrap_or(1) * 1_000_000
        } else if let Some(us) = raw.strip_suffix("us") {
            us.parse::<u64>().unwrap_or(1) * 1_000
        } else if let Some(ns) = raw.strip_suffix("ns") {
            ns.parse::<u64>().unwrap_or(1)
        } else {
            raw.parse::<u64>().unwrap_or(1_000_000)
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt, _current_fn: Option<&str>, diags: &mut DiagnosticBag) {
        match stmt {
            Stmt::Let {
                name,
                mutable: _,
                ty,
                value,
                span,
            } => {
                self.analyze_let(name, ty.as_ref(), value, *span, diags);
            }
            Stmt::RegWrite {
                device_var,
                reg_name,
                value,
                span,
            } => {
                self.analyze_reg_write(device_var, reg_name, value, *span, diags);
            }
            Stmt::Transfer {
                event_var,
                mode,
                waypoints,
                payload,
                fallback_path,
                constraints,
                span,
            } => {
                self.analyze_transfer(
                    event_var.as_deref(),
                    mode,
                    waypoints,
                    payload,
                    fallback_path.as_ref(),
                    constraints,
                    *span,
                    diags,
                );
            }
            Stmt::Await { event_var, span: _ } => {
                self.hir.instructions.push(HirInstruction::EventWait {
                    event_name: event_var.clone(),
                });
            }
            Stmt::Release { var_name, span } => {
                if let Some(info) = self.vars.get_mut(var_name) {
                    match &info.ownership {
                        OwnershipState::Moved { to, at } => {
                            diags.emit_error(
                                "E0501",
                                HardwareErrorKind::OwnershipViolation,
                                PhysicalBoundary::Device,
                                *span,
                                format!(
                                    "Cannot release resource `{}` because its exclusive ownership was already moved to `{}` at line {}",
                                    var_name, to, at.line
                                ),
                                Some("HWCode enforces affine ownership for exclusive hardware resources (Section 5).".to_string()),
                                Some(format!("Call `release({})` on the new owner instead.", to)),
                            );
                        }
                        OwnershipState::Released { at } => {
                            diags.emit_error(
                                "E0502",
                                HardwareErrorKind::OwnershipViolation,
                                PhysicalBoundary::Device,
                                *span,
                                format!(
                                    "Double release of hardware resource `{}` (previously released at line {})",
                                    var_name, at.line
                                ),
                                None,
                                None,
                            );
                        }
                        _ => {
                            let ep = info.endpoint.clone().unwrap_or_else(|| "/F1".to_string());
                            info.ownership = OwnershipState::Released { at: *span };
                            self.hir.instructions.push(HirInstruction::ResourceRelease {
                                var_name: var_name.clone(),
                                endpoint: ep,
                            });
                        }
                    }
                }
            }
            Stmt::UnsafeBlock {
                capability,
                body,
                span,
            } => {
                let cap_name = capability
                    .clone()
                    .unwrap_or_else(|| "Cap::UnrestrictedHardware".to_string());
                diags.record_audit(
                    *span,
                    &cap_name,
                    "unsafe capability block entry",
                    "Programmer assumed explicit responsibility for hardware invariants (Section 10)",
                );
                let prev_unsafe = self.in_unsafe_block;
                self.in_unsafe_block = true;
                for s in body {
                    self.analyze_stmt(s, _current_fn, diags);
                }
                self.in_unsafe_block = prev_unsafe;
            }
            Stmt::Trace { label, .. } => {
                self.hir.instructions.push(HirInstruction::TracePoint {
                    label: label.clone(),
                });
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.check_expr_use(expr, "return", diags);
                }
            }
            Stmt::ExprStmt { expr, .. } => {
                self.check_expr_use(expr, "statement", diags);
            }
            Stmt::Dispatch {
                queue,
                kernel_name,
                grid,
                block,
                args,
                span,
            } => {
                let resolved_queue = self.resolve_endpoint(queue);
                if !resolved_queue.contains("queue") && !resolved_queue.contains("/F3") {
                    diags.emit_warning(
                        "W0201",
                        HardwareErrorKind::Unsupported,
                        PhysicalBoundary::Device,
                        *span,
                        format!("Dispatch target `{}` is not a dedicated accelerator compute queue", resolved_queue),
                        Some("Hardware dispatches should target compute queues such as `/F3::queue`.".to_string()),
                        None,
                    );
                }

                let mut arg_names = Vec::new();
                for arg in args {
                    if let Expr::Var(vname, vspan) = arg {
                        if let Some(info) = self.vars.get(vname) {
                            if let OwnershipState::Moved { to, at } = &info.ownership {
                                diags.emit_error(
                                    "E0501",
                                    HardwareErrorKind::OwnershipViolation,
                                    PhysicalBoundary::Device,
                                    *vspan,
                                    format!("Cannot pass moved buffer `{}` to kernel `{}` (moved to `{}` at line {})", vname, kernel_name, to, at.line),
                                    None,
                                    None,
                                );
                            }
                        }
                        arg_names.push(vname.clone());
                    } else {
                        arg_names.push("expr".to_string());
                    }
                }

                let total_threads =
                    (grid.0 * grid.1 * grid.2) as u64 * (block.0 * block.1 * block.2) as u64;
                let est_time_ns = 500 + total_threads / 8;

                self.hir.instructions.push(HirInstruction::GpuDispatch {
                    queue: resolved_queue,
                    kernel_name: kernel_name.clone(),
                    grid: *grid,
                    block: *block,
                    args: arg_names,
                    estimated_time_ns: est_time_ns,
                });
            }
            Stmt::InterruptAttach {
                endpoint,
                vector,
                handler_name,
                span: _,
            } => {
                let resolved_ep = self.resolve_endpoint(endpoint);
                self.hir.instructions.push(HirInstruction::InterruptAttach {
                    endpoint: resolved_ep,
                    vector: *vector,
                    handler_name: handler_name.clone(),
                });
            }
        }
    }

    fn analyze_let(
        &mut self,
        name: &str,
        ty: Option<&TypeSpec>,
        value: &Expr,
        span: Span,
        diags: &mut DiagnosticBag,
    ) {
        match value {
            Expr::Find { class_name, .. } => {
                if let Some(node) = self.graph.find_by_class(class_name).cloned() {
                    let init_state = node
                        .state_machine
                        .as_ref()
                        .map(|sm| sm.initial_state.clone());
                    self.vars.insert(
                        name.to_string(),
                        VarInfo {
                            name: name.to_string(),
                            semantic_type: format!("Endpoint<{}>", node.class),
                            domain: Some(node.memory_domain.clone()),
                            layout: Some(node.preferred_layout.clone()),
                            endpoint: Some(node.endpoint_id.clone()),
                            clock_domain: Some(node.clock_domain.name.clone()),
                            device_class: Some(node.class.to_string()),
                            device_state: init_state,
                            ownership: OwnershipState::Shared,
                            is_synchronized_signal: false,
                            budget_remaining_watts: None,
                            budget_total_watts: None,
                        },
                    );
                } else {
                    diags.emit_error(
                        "E0301",
                        HardwareErrorKind::Unsupported,
                        PhysicalBoundary::Topology,
                        span,
                        format!(
                            "Capability discovery `find({})` could not locate a matching resource in the Hardware Graph",
                            class_name
                        ),
                        Some("No node in the active target topology exposes this device class.".to_string()),
                        Some("Available classes: CPU, RAM, GPU, Storage, Network, FPGA, Sensor.".to_string()),
                    );
                }
            }
            Expr::DomainAlloc { type_spec, .. } => {
                let dom = type_spec
                    .domain
                    .clone()
                    .unwrap_or_else(|| "SystemRAM".to_string());
                let lay = type_spec
                    .layout_or_state
                    .clone()
                    .unwrap_or_else(|| "Linear".to_string());

                // Verify layout is known
                if dom != "Device" && !self.layouts.contains_key(&lay) {
                    diags.emit_error(
                        "E0402",
                        HardwareErrorKind::LayoutMismatch,
                        PhysicalBoundary::Memory,
                        span,
                        format!("Unknown physical layout `{}` in allocation of `{}`", lay, name),
                        Some("Physical layouts must be built-in (`Linear`, `RowMajor`, `ColMajor`, `Tiled2D`, `Packed`, `Swizzled`) or declared with `layout Name { ... }`.".to_string()),
                        Some(format!("Add `layout {} {{ block_x: 16; block_y: 16; }}` before use.", lay)),
                    );
                }

                let ep = match dom.as_str() {
                    "VRAM" => Some("/F3".to_string()),
                    "PersistentMemory" => Some("/F4".to_string()),
                    "LocalMemory" => Some("/F6".to_string()),
                    "MMIO" => Some("/F7".to_string()),
                    _ => Some("/F2".to_string()),
                };

                self.hir.instructions.push(HirInstruction::DomainAlloc {
                    var_name: name.to_string(),
                    domain: dom.clone(),
                    semantic_type: type_spec.base_type.clone(),
                    layout: lay.clone(),
                    bytes: 4096,
                });

                self.vars.insert(
                    name.to_string(),
                    VarInfo {
                        name: name.to_string(),
                        semantic_type: type_spec.base_type.clone(),
                        domain: Some(dom),
                        layout: Some(lay.clone()),
                        endpoint: ep,
                        clock_domain: Some("CPU_CLK".to_string()),
                        device_class: if type_spec.domain.as_deref() == Some("Device") {
                            Some(type_spec.base_type.clone())
                        } else {
                            None
                        },
                        device_state: if type_spec.domain.as_deref() == Some("Device") {
                            Some(lay)
                        } else {
                            None
                        },
                        ownership: OwnershipState::OwnedExclusive,
                        is_synchronized_signal: false,
                        budget_remaining_watts: None,
                        budget_total_watts: None,
                    },
                );
            }
            Expr::Acquire { endpoint, mode, .. } => {
                let resolved_ep = self.resolve_endpoint(endpoint);
                let is_exclusive = mode.contains("Exclusive");

                // Check if another active variable already holds Exclusive ownership of this exact endpoint
                if is_exclusive {
                    for existing in self.vars.values() {
                        if existing.endpoint.as_deref() == Some(&resolved_ep)
                            && existing.ownership == OwnershipState::OwnedExclusive
                        {
                            diags.emit_error(
                                "E0503",
                                HardwareErrorKind::OwnershipViolation,
                                PhysicalBoundary::Device,
                                span,
                                format!(
                                    "Cannot acquire `{}` exclusively for `{}` because `{}` already holds exclusive ownership",
                                    resolved_ep, name, existing.name
                                ),
                                Some("Two owners cannot simultaneously hold Mode::Exclusive access to the same hardware endpoint.".to_string()),
                                Some(format!("Release `{}` first via `release({});` or use `Mode::Shared` if supported.", existing.name, existing.name)),
                            );
                        }
                    }
                }

                self.hir.instructions.push(HirInstruction::ResourceAcquire {
                    dest_var: name.to_string(),
                    endpoint: resolved_ep.clone(),
                    mode: mode.clone(),
                });

                let node_opt = self.graph.get_node(&resolved_ep).cloned();
                let clk = node_opt
                    .as_ref()
                    .map(|n| n.clock_domain.name.clone())
                    .unwrap_or_else(|| "CPU_CLK".to_string());
                let dev_class = node_opt.as_ref().map(|n| n.class.to_string());
                let dev_state = node_opt
                    .as_ref()
                    .and_then(|n| n.state_machine.as_ref().map(|sm| sm.initial_state.clone()));

                self.vars.insert(
                    name.to_string(),
                    VarInfo {
                        name: name.to_string(),
                        semantic_type: format!("Capability<{}>", resolved_ep),
                        domain: node_opt.map(|n| n.memory_domain),
                        layout: Some("Linear".to_string()),
                        endpoint: Some(resolved_ep),
                        clock_domain: Some(clk),
                        device_class: dev_class,
                        device_state: dev_state,
                        ownership: if is_exclusive {
                            OwnershipState::OwnedExclusive
                        } else {
                            OwnershipState::Shared
                        },
                        is_synchronized_signal: false,
                        budget_remaining_watts: None,
                        budget_total_watts: None,
                    },
                );
            }
            Expr::AcquireBudget { watts, .. } => {
                self.total_budget_acquired_w += *watts;
                self.hir.instructions.push(HirInstruction::BudgetAccount {
                    budget_var: name.to_string(),
                    allocated_watts: 0.0,
                    remaining_watts: *watts,
                });
                self.vars.insert(
                    name.to_string(),
                    VarInfo {
                        name: name.to_string(),
                        semantic_type: "PowerBudget".to_string(),
                        domain: None,
                        layout: None,
                        endpoint: None,
                        clock_domain: None,
                        device_class: None,
                        device_state: None,
                        ownership: OwnershipState::OwnedExclusive,
                        is_synchronized_signal: false,
                        budget_remaining_watts: Some(*watts),
                        budget_total_watts: Some(*watts),
                    },
                );
            }
            Expr::SplitBudget {
                parent_var, watts, ..
            } => {
                // Linear Power/Thermal Budget Accounting (Section 9)
                if let Some(parent) = self.vars.get_mut(parent_var) {
                    let rem = parent.budget_remaining_watts.unwrap_or(0.0);
                    let total = parent.budget_total_watts.unwrap_or(0.0);
                    if *watts > rem {
                        diags.emit_error(
                            "E0901",
                            HardwareErrorKind::BudgetExceeded,
                            PhysicalBoundary::Power,
                            span,
                            format!(
                                "Budget linear accounting violation: requested `{:.1}W` for `{}` from `{}` (total {:.1}W), but only `{:.1}W` remains available",
                                watts, name, parent_var, total, rem
                            ),
                            Some("Section 9: Energy and thermal budgets use strict linear accounting; sub-budgets cannot exceed the acquired power envelope.".to_string()),
                            Some(format!("Reduce the allocation for `{}` to <= {:.1}W or acquire a larger top-level budget.", name, rem)),
                        );
                    } else {
                        let new_rem = rem - *watts;
                        parent.budget_remaining_watts = Some(new_rem);
                        self.total_budget_allocated_w += *watts;
                        self.hir.instructions.push(HirInstruction::BudgetAccount {
                            budget_var: parent_var.clone(),
                            allocated_watts: *watts,
                            remaining_watts: new_rem,
                        });
                    }
                }
                self.vars.insert(
                    name.to_string(),
                    VarInfo {
                        name: name.to_string(),
                        semantic_type: "PowerBudget".to_string(),
                        domain: None,
                        layout: None,
                        endpoint: None,
                        clock_domain: None,
                        device_class: None,
                        device_state: None,
                        ownership: OwnershipState::OwnedExclusive,
                        is_synchronized_signal: false,
                        budget_remaining_watts: Some(*watts),
                        budget_total_watts: Some(*watts),
                    },
                );
            }
            Expr::ClockSync {
                sync_kind,
                signal_expr,
                target_domain,
                ..
            } => {
                let sig_name = match &**signal_expr {
                    Expr::Var(v, _) => v.clone(),
                    Expr::Endpoint(e, _) => e.clone(),
                    _ => "signal".to_string(),
                };
                let from_clk = self
                    .vars
                    .get(&sig_name)
                    .and_then(|v| v.clock_domain.clone())
                    .unwrap_or_else(|| "FPGA_CLK".to_string());
                let to_clk = target_domain
                    .clone()
                    .unwrap_or_else(|| "CPU_CLK".to_string());

                self.hir.instructions.push(HirInstruction::ClockSync {
                    signal: sig_name,
                    from_domain: from_clk,
                    to_domain: to_clk.clone(),
                    primitive: sync_kind.clone(),
                    latency_cycles: if sync_kind == "doubleflop" { 2 } else { 4 },
                });

                self.vars.insert(
                    name.to_string(),
                    VarInfo {
                        name: name.to_string(),
                        semantic_type: "SyncedSignal".to_string(),
                        domain: Some("SystemRAM".to_string()),
                        layout: Some("Linear".to_string()),
                        endpoint: Some("/F1".to_string()),
                        clock_domain: Some(to_clk),
                        device_class: None,
                        device_state: None,
                        ownership: OwnershipState::Shared,
                        is_synchronized_signal: true,
                        budget_remaining_watts: None,
                        budget_total_watts: None,
                    },
                );
            }
            Expr::Var(src_var, var_span) => {
                // Check affine move semantics if `src_var` is an exclusively owned resource!
                if let Some(src_info) = self.vars.get_mut(src_var).cloned() {
                    match &src_info.ownership {
                        OwnershipState::Moved { to, at } => {
                            diags.emit_error(
                                "E0501",
                                HardwareErrorKind::OwnershipViolation,
                                PhysicalBoundary::Device,
                                *var_span,
                                format!(
                                    "Use of moved exclusive hardware resource `{}` (ownership moved to `{}` at line {})",
                                    src_var, to, at.line
                                ),
                                Some("Section 5: HWCode uses affine ownership for exclusive resources; after ownership moves, the previous owner cannot use it.".to_string()),
                                Some(format!("Use the new owner `{}` instead.", to)),
                            );
                        }
                        OwnershipState::Released { at } => {
                            diags.emit_error(
                                "E0502",
                                HardwareErrorKind::OwnershipViolation,
                                PhysicalBoundary::Device,
                                *var_span,
                                format!(
                                    "Use of released hardware resource `{}` (released at line {})",
                                    src_var, at.line
                                ),
                                None,
                                None,
                            );
                        }
                        OwnershipState::OwnedExclusive => {
                            // If this is a hardware capability handle, moving it invalidates `src_var`
                            if src_info.semantic_type.starts_with("Capability<") {
                                if let Some(orig) = self.vars.get_mut(src_var) {
                                    orig.ownership = OwnershipState::Moved {
                                        to: name.to_string(),
                                        at: span,
                                    };
                                }
                            }
                            let mut moved_copy = src_info.clone();
                            moved_copy.name = name.to_string();
                            self.vars.insert(name.to_string(), moved_copy);
                        }
                        OwnershipState::Shared => {
                            let mut shared_copy = src_info.clone();
                            shared_copy.name = name.to_string();
                            self.vars.insert(name.to_string(), shared_copy);
                        }
                    }
                } else {
                    self.vars.insert(
                        name.to_string(),
                        VarInfo {
                            name: name.to_string(),
                            semantic_type: ty
                                .map(|t| t.base_type.clone())
                                .unwrap_or_else(|| "i32".to_string()),
                            domain: ty.and_then(|t| t.domain.clone()),
                            layout: ty.and_then(|t| t.layout_or_state.clone()),
                            endpoint: None,
                            clock_domain: Some("CPU_CLK".to_string()),
                            device_class: None,
                            device_state: None,
                            ownership: OwnershipState::Shared,
                            is_synchronized_signal: false,
                            budget_remaining_watts: None,
                            budget_total_watts: None,
                        },
                    );
                }
            }
            Expr::Endpoint(ep, _) => {
                let resolved = self.resolve_endpoint(ep);
                let node_opt = self.graph.get_node(&resolved).cloned();
                self.vars.insert(
                    name.to_string(),
                    VarInfo {
                        name: name.to_string(),
                        semantic_type: "Endpoint".to_string(),
                        domain: node_opt.as_ref().map(|n| n.memory_domain.clone()),
                        layout: node_opt.as_ref().map(|n| n.preferred_layout.clone()),
                        endpoint: Some(resolved),
                        clock_domain: node_opt.as_ref().map(|n| n.clock_domain.name.clone()),
                        device_class: node_opt.as_ref().map(|n| n.class.to_string()),
                        device_state: node_opt.as_ref().and_then(|n| {
                            n.state_machine.as_ref().map(|sm| sm.initial_state.clone())
                        }),
                        ownership: OwnershipState::Shared,
                        is_synchronized_signal: false,
                        budget_remaining_watts: None,
                        budget_total_watts: None,
                    },
                );
            }
            other => {
                self.check_expr_use(other, name, diags);
                let sem_ty = ty
                    .map(|t| t.base_type.clone())
                    .unwrap_or_else(|| "i32".to_string());
                let dom = ty.and_then(|t| t.domain.clone());
                let lay = ty.and_then(|t| t.layout_or_state.clone());
                self.vars.insert(
                    name.to_string(),
                    VarInfo {
                        name: name.to_string(),
                        semantic_type: sem_ty,
                        domain: dom,
                        layout: lay,
                        endpoint: Some("/F2".to_string()),
                        clock_domain: Some("CPU_CLK".to_string()),
                        device_class: None,
                        device_state: None,
                        ownership: OwnershipState::Shared,
                        is_synchronized_signal: false,
                        budget_remaining_watts: None,
                        budget_total_watts: None,
                    },
                );
            }
        }
    }

    /// Analyze a register write such as `sensor.REG.START = 1;` and trigger State Machine transitions (Section 7).
    fn analyze_reg_write(
        &mut self,
        device_var: &str,
        reg_name: &str,
        value: &Expr,
        span: Span,
        diags: &mut DiagnosticBag,
    ) {
        let val_str = match value {
            Expr::Int(v, _, _) => v.to_string(),
            Expr::Var(s, _) => s.clone(),
            _ => "1".to_string(),
        };
        let trigger_str = format!("REG.{} = {}", reg_name, val_str);

        if let Some(info) = self.vars.get_mut(device_var) {
            let ep = info.endpoint.clone().unwrap_or_else(|| "/F7".to_string());
            let cur_state = info
                .device_state
                .clone()
                .unwrap_or_else(|| "Idle".to_string());

            if let Some(node) = self.graph.get_node(&ep).cloned() {
                if let Some(sm) = &node.state_machine {
                    if let Some(rule) = sm.transitions.iter().find(|r| {
                        r.from_state == cur_state
                            && r.trigger.replace(' ', "") == trigger_str.replace(' ', "")
                    }) {
                        let new_state = rule.to_state.clone();
                        info.device_state = Some(new_state.clone());
                        self.hir.instructions.push(HirInstruction::StateTransition {
                            device_var: device_var.to_string(),
                            device_class: sm.device_name.clone(),
                            from_state: cur_state,
                            to_state: new_state,
                            trigger: trigger_str,
                        });
                        return;
                    }
                }
            }
        } else {
            diags.emit_error(
                "E0702",
                HardwareErrorKind::InvalidDeviceState,
                PhysicalBoundary::State,
                span,
                format!(
                    "Unknown device handle `{}` in register write `REG.{}`",
                    device_var, reg_name
                ),
                None,
                None,
            );
        }
    }

    /// Analyze a transfer/route/DMA operation across all Physical Boundaries (Sections 3, 4, 5, 6, 8, 11).
    #[allow(clippy::too_many_arguments)]
    fn analyze_transfer(
        &mut self,
        event_var: Option<&str>,
        mode: &TransferMode,
        waypoints: &[String],
        payload: &str,
        fallback_path: Option<&Vec<String>>,
        _constraints: &[(String, String)],
        span: Span,
        diags: &mut DiagnosticBag,
    ) {
        if waypoints.len() < 2 {
            return;
        }

        // Check if any waypoint or payload is a moved/released exclusive resource
        for wp in waypoints {
            let base_var = wp.split("::").next().unwrap_or(wp);
            if let Some(vinfo) = self.vars.get(base_var) {
                if let OwnershipState::Moved { to, at } = &vinfo.ownership {
                    diags.emit_error(
                        "E0501",
                        HardwareErrorKind::OwnershipViolation,
                        PhysicalBoundary::Device,
                        span,
                        format!(
                            "Cannot use endpoint handle `{}` in transfer because its exclusive ownership moved to `{}` at line {}",
                            base_var, to, at.line
                        ),
                        Some("Exclusive hardware resources follow affine ownership semantics.".to_string()),
                        Some(format!("Use `{}` instead.", to)),
                    );
                    return;
                }
            }
        }

        let src_raw = &waypoints[0];
        let dst_raw = waypoints.last().unwrap();
        let src_ep = self.resolve_endpoint(src_raw);
        let dst_ep = self.resolve_endpoint(dst_raw);

        // 1. Clock Boundary Check (Section 6):
        // Crossing an asynchronous clock boundary with a raw signal (not DMA and not synchronized) is a ClockDomainHazard!
        let clk_rel = self.graph.clock_relation(&src_ep, &dst_ep);
        let payload_info = self.vars.get(payload).cloned();
        let is_bulk_buffer = payload_info
            .as_ref()
            .map(|p| {
                (p.domain.is_some()
                    && (p.semantic_type.contains("Matrix")
                        || p.semantic_type.contains("Buffer")
                        || p.semantic_type.starts_with('[')))
                    || p.name == "data"
                    || p.name == "buffer"
            })
            .unwrap_or(payload == "data" || payload == "buffer");
        let is_synced = payload_info
            .as_ref()
            .map(|p| p.is_synchronized_signal)
            .unwrap_or(false);

        if clk_rel == ClockRelation::Asynchronous
            && *mode != TransferMode::DmaExplicit
            && !is_bulk_buffer
            && !is_synced
            && !self.in_unsafe_block
        {
            let src_clk = self
                .graph
                .get_node(&src_ep)
                .map(|n| {
                    format!(
                        "{} ({} MHz)",
                        n.clock_domain.name, n.clock_domain.frequency_mhz
                    )
                })
                .unwrap_or_else(|| "ASYNC_CLK_A".to_string());
            let dst_clk = self
                .graph
                .get_node(&dst_ep)
                .map(|n| {
                    format!(
                        "{} ({} MHz)",
                        n.clock_domain.name, n.clock_domain.frequency_mhz
                    )
                })
                .unwrap_or_else(|| "ASYNC_CLK_B".to_string());

            diags.emit_error(
                "E0601",
                HardwareErrorKind::ClockDomainHazard,
                PhysicalBoundary::Clock,
                span,
                format!(
                    "Unsynchronized raw signal `{}` crosses asynchronous clock boundary from `{}` [{}] to `{}` [{}]",
                    payload, src_ep, src_clk, dst_ep, dst_clk
                ),
                Some("Section 6: A raw signal crossing from one asynchronous clock domain to another without a declared synchronizer can cause metastability and physical sampling failure.".to_string()),
                Some(format!(
                    "Wrap the signal in a synchronizer first: `let synced = doubleflop({}, {});` (or `async_fifo` / `handshake`).",
                    payload,
                    self.graph
                        .get_node(&dst_ep)
                        .map(|n| n.clock_domain.name.as_str())
                        .unwrap_or("CPU_CLK")
                )),
            );
            return;
        }

        // 2. Topology & Route Planning Boundary (Section 3 & 8)
        let planned_result = if waypoints.len() > 2 && *mode == TransferMode::ExplicitRoute {
            // Explicit multi-hop route specified by programmer: e.g. `/F1 -> /F2 -> /F3`
            let resolved_hops: Vec<String> =
                waypoints.iter().map(|w| self.resolve_endpoint(w)).collect();
            self.graph.plan_route(&src_ep, &dst_ep).map(|mut r| {
                r.hops = resolved_hops;
                r
            })
        } else {
            self.graph.plan_route(&src_ep, &dst_ep)
        };

        let mut planned = match planned_result {
            Ok(r) => r,
            Err(reason) => {
                diags.emit_error(
                    "E0302",
                    HardwareErrorKind::RouteUnavailable,
                    PhysicalBoundary::Topology,
                    span,
                    format!(
                        "No legal hardware route available from `{}` to `{}`: {}",
                        src_ep, dst_ep, reason
                    ),
                    Some("The Hardware Graph route planner evaluated all physical and logical links and found no valid path.".to_string()),
                    Some("Grant the required capability or route through `/F2` (SystemRAM).".to_string()),
                );
                return;
            }
        };

        if let Some(fb) = fallback_path {
            planned.fallback_hops = Some(fb.iter().map(|w| self.resolve_endpoint(w)).collect());
        }

        // 3. Memory & Physical Layout Boundary (Section 4):
        // Check if source and destination have a Semantic Type match vs Physical Layout mismatch!
        let src_var_info = self
            .vars
            .get(src_raw)
            .cloned()
            .or_else(|| payload_info.clone());
        let dst_var_info = self.vars.get(dst_raw).cloned();

        // Check semantic type mismatch if both source and destination variables are typed buffers
        if let (Some(s_var), Some(d_var)) = (&src_var_info, &dst_var_info) {
            if !s_var.semantic_type.starts_with("Endpoint")
                && !d_var.semantic_type.starts_with("Endpoint")
                && !s_var.semantic_type.starts_with("Capability")
                && !d_var.semantic_type.starts_with("Capability")
                && s_var.semantic_type != d_var.semantic_type
            {
                diags.emit_error(
                    "E0401",
                    HardwareErrorKind::TypeError,
                    PhysicalBoundary::Memory,
                    span,
                    format!(
                        "Semantic type mismatch in hardware transfer: source `{}` has type `{}`, but destination `{}` expects `{}`",
                        src_raw, s_var.semantic_type, dst_raw, d_var.semantic_type
                    ),
                    Some("Section 4: Type correctness protects semantic meaning; transfers require identical semantic types.".to_string()),
                    None,
                );
                return;
            }
        }

        let src_layout = src_var_info
            .as_ref()
            .and_then(|v| v.layout.clone())
            .or_else(|| {
                self.graph
                    .get_node(&src_ep)
                    .map(|n| n.preferred_layout.clone())
            })
            .unwrap_or_else(|| "Linear".to_string());

        let dst_layout = dst_var_info
            .as_ref()
            .and_then(|v| v.layout.clone())
            .or_else(|| {
                self.graph
                    .get_node(&dst_ep)
                    .map(|n| n.preferred_layout.clone())
            })
            .unwrap_or_else(|| "Linear".to_string());

        let semantic_ty = src_var_info
            .as_ref()
            .map(|v| v.semantic_type.clone())
            .unwrap_or_else(|| "Matrix<f32>".to_string());

        // Emit ROUTE in Universal HIR
        self.route_counter += 1;
        let route_id = self.route_counter;
        self.hir.instructions.push(HirInstruction::Route {
            route_id,
            planned: planned.clone(),
            payload: payload.to_string(),
        });

        // Emit MEMORY_BARRIER if crossing non-coherent domains (Section 5)
        if planned.requires_memory_barrier {
            let dom_from = self
                .graph
                .get_node(&src_ep)
                .map(|n| n.memory_domain.clone())
                .unwrap_or_else(|| "SystemRAM".to_string());
            let dom_to = self
                .graph
                .get_node(&dst_ep)
                .map(|n| n.memory_domain.clone())
                .unwrap_or_else(|| "VRAM".to_string());
            if dom_from != dom_to {
                self.hir.instructions.push(HirInstruction::MemoryBarrier {
                    ordering: "AcqRel_DMA_Coherency".to_string(),
                    domain_from: dom_from,
                    domain_to: dom_to,
                });
            }
        }

        // Emit DMA instruction
        let dma_engine = planned
            .dma_engines
            .first()
            .cloned()
            .unwrap_or_else(|| format!("{}::dma", dst_ep));
        self.hir.instructions.push(HirInstruction::Dma {
            engine: dma_engine,
            src_endpoint: src_ep.clone(),
            dst_endpoint: dst_ep.clone(),
            payload: payload.to_string(),
            bytes: 65536,
            async_event: event_var.map(|s| s.to_string()),
        });

        if let Some(ev) = event_var {
            self.hir.instructions.push(HirInstruction::EventSignal {
                event_name: ev.to_string(),
                source_op: format!("DMA({} -> {})", src_ep, dst_ep),
            });
        }

        // Distributed Cluster RDMA Network Transfer (Phase 3)
        if planned.protocols.iter().any(|p| p.contains("RDMA"))
            || src_ep.contains("/N")
            || dst_ep.contains("/N")
        {
            self.hir.instructions.push(HirInstruction::RdmaTransfer {
                src_node: src_ep.clone(),
                dst_node: dst_ep.clone(),
                payload: payload.to_string(),
                bytes: 65536,
                verb: "IBV_WR_RDMA_WRITE".to_string(),
            });
        }

        // Emit explicit LAYOUT_CONVERT if physical layouts differ (Section 4 & Phase 3 NPU)
        if src_layout != dst_layout {
            let mechanism = if dst_ep == "/F8" || dst_layout == "Tensor4D" {
                "NPU_TensorCore_Packer".to_string()
            } else if dst_ep == "/F3" || dst_ep.ends_with("/F3") {
                "GPU_TileSwizzle_Engine".to_string()
            } else {
                "DMA_StrideTransform".to_string()
            };
            self.hir.instructions.push(HirInstruction::LayoutConvert {
                buffer: payload.to_string(),
                semantic_type: semantic_ty.clone(),
                from_layout: src_layout.clone(),
                to_layout: dst_layout.clone(),
                mechanism: mechanism.clone(),
                estimated_cost_ns: 420,
            });
            diags.emit_info(
                "I0401",
                HardwareErrorKind::LayoutMismatch,
                PhysicalBoundary::Memory,
                span,
                format!(
                    "Inserted explicit `LAYOUT_CONVERT` ({} -> {}) via `{}` (+420 ns) for `{}`",
                    src_layout, dst_layout, mechanism, payload
                ),
                Some("Section 4: Semantic types match, but source and destination require different physical representations.".to_string()),
                None,
            );
        }

        self.planned_routes.push(planned);
    }

    fn check_expr_use(&mut self, expr: &Expr, _ctx: &str, diags: &mut DiagnosticBag) {
        match expr {
            Expr::Var(vname, span) => {
                if let Some(info) = self.vars.get(vname) {
                    if let OwnershipState::Moved { to, at } = &info.ownership {
                        diags.emit_error(
                            "E0501",
                            HardwareErrorKind::OwnershipViolation,
                            PhysicalBoundary::Device,
                            *span,
                            format!(
                                "Use of moved exclusive hardware resource `{}` (moved to `{}` at line {})",
                                vname, to, at.line
                            ),
                            Some("Section 5: After exclusive ownership moves, the previous owner cannot use it.".to_string()),
                            Some(format!("Use `{}` instead.", to)),
                        );
                    }
                }
            }
            Expr::Call {
                receiver,
                func,
                args,
                span,
            } => {
                // Check arguments for moved exclusive handles
                for (idx, arg) in args.iter().enumerate() {
                    if let Expr::Var(arg_var, arg_span) = arg {
                        if let Some(info) = self.vars.get_mut(arg_var) {
                            match &info.ownership {
                                OwnershipState::Moved { to, at } => {
                                    diags.emit_error(
                                        "E0501",
                                        HardwareErrorKind::OwnershipViolation,
                                        PhysicalBoundary::Device,
                                        *arg_span,
                                        format!(
                                            "Cannot pass `{}` to `{}` because ownership was already moved to `{}` at line {}",
                                            arg_var, func, to, at.line
                                        ),
                                        Some("Exclusive hardware capabilities have affine move semantics.".to_string()),
                                        None,
                                    );
                                }
                                OwnershipState::OwnedExclusive => {
                                    if info.semantic_type.starts_with("Capability<") {
                                        info.ownership = OwnershipState::Moved {
                                            to: format!("{} arg #{}", func, idx + 1),
                                            at: *arg_span,
                                        };
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else {
                        self.check_expr_use(arg, func, diags);
                    }
                }

                // Device State Machine Verification on method calls like `sensor.read_temperature()` (Section 7)
                if let Some(recv_var) = receiver {
                    if let Some(info) = self.vars.get(recv_var).cloned() {
                        let ep = info.endpoint.clone().unwrap_or_else(|| "/F7".to_string());
                        let actual_state = info
                            .device_state
                            .clone()
                            .unwrap_or_else(|| "Idle".to_string());

                        if let Some(node) = self.graph.get_node(&ep) {
                            if let Some(sm) = &node.state_machine {
                                if let Some(req_state) = sm.operation_requirements.get(func) {
                                    if &actual_state != req_state {
                                        // Find transition hint
                                        let hint = sm
                                            .transitions
                                            .iter()
                                            .find(|t| {
                                                t.from_state == actual_state
                                                    && &t.to_state == req_state
                                            })
                                            .map(|t| {
                                                format!(
                                                    "Transition `{}` to `{}` first via `{}.{};`.",
                                                    recv_var, req_state, recv_var, t.trigger
                                                )
                                            })
                                            .unwrap_or_else(|| {
                                                format!(
                                                    "Transition `{}` to `{}` state before calling `{}`.",
                                                    recv_var, req_state, func
                                                )
                                            });

                                        diags.emit_error(
                                            "E0701",
                                            HardwareErrorKind::InvalidDeviceState,
                                            PhysicalBoundary::State,
                                            *span,
                                            format!(
                                                "Operation `{}.{}` requires device state `{}`, but `{}` is currently in state `{}`",
                                                recv_var, func, req_state, recv_var, actual_state
                                            ),
                                            Some("Section 7: Many low-level hardware failures arise from performing the right operation in the wrong device state.".to_string()),
                                            Some(hint),
                                        );
                                        return;
                                    } else {
                                        self.hir.instructions.push(HirInstruction::DeviceCommand {
                                            device_var: recv_var.clone(),
                                            endpoint: ep,
                                            command: func.clone(),
                                            verified_state: actual_state,
                                        });
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }

                // Standard Native Call
                let arg_strs = args
                    .iter()
                    .map(|a| match a {
                        Expr::Var(v, _) => v.clone(),
                        Expr::Int(i, _, _) => i.to_string(),
                        Expr::Float(fl, _, _) => fl.to_string(),
                        Expr::Str(s, _) => format!("\"{}\"", s),
                        _ => "expr".to_string(),
                    })
                    .collect();
                self.hir.instructions.push(HirInstruction::NativeCall {
                    dest_var: None,
                    func_name: func.clone(),
                    args: arg_strs,
                });
            }
            Expr::Binary { left, right, .. } => {
                self.check_expr_use(left, _ctx, diags);
                self.check_expr_use(right, _ctx, diags);
            }
            _ => {}
        }
    }

    fn resolve_endpoint(&self, raw: &str) -> String {
        if raw.starts_with('/') {
            return raw.to_string();
        }
        let base = raw.split("::").next().unwrap_or(raw);
        if let Some(info) = self.vars.get(base) {
            if let Some(ep) = &info.endpoint {
                return ep.clone();
            }
        }
        match base.to_lowercase().as_str() {
            "cpu" => "/F1".to_string(),
            "ram" | "mem" => "/F2".to_string(),
            "gpu" => "/F3".to_string(),
            "disk" | "nvme" | "storage" => "/F4".to_string(),
            "nic" | "net" => "/F5".to_string(),
            "fpga" => "/F6".to_string(),
            "sensor" => "/F7".to_string(),
            _ => "/F1".to_string(),
        }
    }
}
