//! HWCode — Universal Hardware-Oriented Systems Language
//! Core compiler library exposing Lexer, Parser, Hardware Graph, Semantic & Physical Boundary
//! Analyzer, Universal HIR Planner & Optimizer, Explainer, C Backend, and Hardware Simulator.

pub mod ast;
pub mod codegen_c;
pub mod diagnostics;
pub mod explainer;
pub mod hardware_graph;
pub mod hir;
pub mod lexer;
pub mod parser;
pub mod runner;
pub mod sema;
pub mod simulator;
pub mod visualizer;

use ast::Program;
use diagnostics::DiagnosticBag;
use hardware_graph::HardwareGraph;
use hir::OptLevel;
use lexer::Lexer;
use parser::Parser;
use sema::{AnalysisReport, SemanticAnalyzer};

pub struct CompilePipelineOutput {
    pub program: Program,
    pub graph: HardwareGraph,
    pub report: AnalysisReport,
    pub diagnostics: DiagnosticBag,
}

/// Runs the complete HWCode front-end and middle-end pipeline on `source`.
pub fn compile_source(
    filename: &str,
    source: &str,
    opt_level: OptLevel,
    custom_graph: Option<HardwareGraph>,
) -> CompilePipelineOutput {
    let mut diags = DiagnosticBag::new(filename, source);
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();

    let mut parser = Parser::new(tokens);
    let program = parser.parse_program(&mut diags);

    let mut graph = custom_graph.unwrap_or_else(HardwareGraph::standard_machine);
    let mut analyzer = SemanticAnalyzer::new(&mut graph);
    let mut report = analyzer.analyze_program(&program, &mut diags);

    if !diags.has_errors() {
        report.hir.optimize(opt_level);
    }

    CompilePipelineOutput {
        program,
        graph,
        report,
        diagnostics: diags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::HirInstruction;
    use crate::simulator::{HardwareSimulator, InjectedFault};

    #[test]
    fn test_section2_and_3_discovery_and_pdf_route_syntax() {
        // Tests exact syntax from Page 2, 3, and 14 of the PDF:
        // `cpu = find(CPU); gpu = find(GPU); disk = find(Storage); disk ■ gpu : data;`
        let src = r#"
            fn add(a: i32, b: i32) -> i32 { a + b }

            cpu = find(CPU);
            gpu = find(GPU);
            disk = find(Storage);
            let data = RAM<Matrix<f32>>(Linear);
            disk ■ gpu : data;
        "#;

        let out = compile_source("test_route.hwc", src, OptLevel::Max, None);
        assert!(
            !out.diagnostics.has_errors(),
            "Unexpected errors:\n{}",
            out.diagnostics.format_all()
        );
        assert_eq!(out.report.planned_routes.len(), 1);
        let route = &out.report.planned_routes[0];
        assert_eq!(route.source, "/F4");
        assert_eq!(route.destination, "/F3");
        // Direct P2P path /F4 -> /F3 should be chosen when Cap::PeerMemory is available
        assert_eq!(route.hops, vec!["/F4", "/F3"]);
        // Also verify LAYOUT_CONVERT was automatically inserted (Linear -> Tiled2D on GPU)
        let has_layout_conv = out
            .report
            .hir
            .instructions
            .iter()
            .any(|i| matches!(i, HirInstruction::LayoutConvert { from_layout, to_layout, .. } if from_layout == "Linear" && to_layout == "Tiled2D"));
        assert!(
            has_layout_conv,
            "Expected automatic LAYOUT_CONVERT Linear -> Tiled2D"
        );
    }

    #[test]
    fn test_section3_capability_blocking_falls_back_to_system_ram() {
        // When Cap::PeerMemory is revoked, `/F4 ~> /F3 : data` explains that direct P2P is blocked
        // and automatically selects the staged route `/F4 -> /F2 -> /F3` through SystemRAM!
        let src = r#"
            let data = RAM<Matrix<f32>>(Linear);
            /F4 ~> /F3 : data;
        "#;
        let mut graph = HardwareGraph::standard_machine();
        graph
            .granted_capabilities
            .retain(|c| c != "Cap::PeerMemory");

        let out = compile_source("test_fallback.hwc", src, OptLevel::Fast, Some(graph));
        assert!(!out.diagnostics.has_errors());
        let route = &out.report.planned_routes[0];
        assert_eq!(route.hops, vec!["/F4", "/F2", "/F3"]);
        assert!(!route.rejected_alternatives.is_empty());
    }

    #[test]
    fn test_section5_affine_ownership_use_after_move_error() {
        // Page 5: `acquire(/F3::dma, Mode::Exclusive)` produces an owned capability with affine move semantics.
        // Using the previous owner after move must fail at compile time!
        let src = r#"
            let dma1 = acquire(/F3::dma, Mode::Exclusive);
            let dma2 = dma1;
            release(dma1);
        "#;
        let out = compile_source("test_ownership.hwc", src, OptLevel::Fast, None);
        assert!(out.diagnostics.has_errors());
        let msg = out.diagnostics.format_all();
        assert!(
            msg.contains("E0501"),
            "Expected E0501 OwnershipViolation, got:\n{}",
            msg
        );
    }

    #[test]
    fn test_section6_clock_domain_crossing_hazard_and_synchronizer() {
        // Page 6: Crossing an asynchronous clock boundary (/F6 FPGA_CLK -> /F1 CPU_CLK)
        // with a raw signal without `doubleflop`, `async_fifo`, or `handshake` must fail!
        let bad_src = r#"
            let irq_sig = 1;
            /F6 -> /F1 : irq_sig;
        "#;
        let bad_out = compile_source("test_cdc_bad.hwc", bad_src, OptLevel::Fast, None);
        assert!(bad_out.diagnostics.has_errors());
        assert!(bad_out.diagnostics.format_all().contains("E0601"));

        // Wrapping in `doubleflop` resolves the hazard and emits `CLOCK_SYNC` in HIR
        let good_src = r#"
            let irq_sig = 1;
            let safe_sig = doubleflop(irq_sig, CPU_CLK);
            /F6 -> /F1 : safe_sig;
        "#;
        let good_out = compile_source("test_cdc_good.hwc", good_src, OptLevel::Fast, None);
        assert!(
            !good_out.diagnostics.has_errors(),
            "Unexpected errors:\n{}",
            good_out.diagnostics.format_all()
        );
        let has_clock_sync = good_out
            .report
            .hir
            .instructions
            .iter()
            .any(|i| matches!(i, HirInstruction::ClockSync { primitive, .. } if primitive == "doubleflop"));
        assert!(has_clock_sync);
    }

    #[test]
    fn test_section7_device_state_machine_verification() {
        // Page 7: Calling `sensor.read_temperature()` while in `Idle` state must fail;
        // transitioning via `sensor.REG.START = 1;` transitions `Idle -> Active` and succeeds.
        let invalid_src = r#"
            let sensor = find(Sensor);
            sensor.read_temperature();
        "#;
        let bad_out = compile_source("test_state_bad.hwc", invalid_src, OptLevel::Fast, None);
        assert!(bad_out.diagnostics.has_errors());
        assert!(bad_out.diagnostics.format_all().contains("E0701"));

        let valid_src = r#"
            let sensor = find(Sensor);
            sensor.REG.START = 1;
            sensor.read_temperature();
        "#;
        let good_out = compile_source("test_state_good.hwc", valid_src, OptLevel::Fast, None);
        assert!(
            !good_out.diagnostics.has_errors(),
            "Unexpected errors:\n{}",
            good_out.diagnostics.format_all()
        );
    }

    #[test]
    fn test_section9_budget_linear_accounting() {
        // Page 9: Acquiring a 50W budget and splitting 35W + 25W = 60W > 50W must fail statically.
        let over_budget_src = r#"
            let total_budget = acquire_budget(50W);
            let gpu_budget = split_budget(total_budget, 35W);
            let cpu_budget = split_budget(total_budget, 25W);
        "#;
        let bad_out = compile_source("test_budget_bad.hwc", over_budget_src, OptLevel::Fast, None);
        assert!(bad_out.diagnostics.has_errors());
        assert!(bad_out.diagnostics.format_all().contains("E0901"));
    }

    #[test]
    fn test_section8_and_13_simulator_fault_injection_recovery() {
        // Page 8 & 13: Injecting LinkDisconnect triggers NORMAL -> FAILED -> REPLAN -> FALLBACK -> RECOVERED
        let src = r#"
            let data = RAM<Matrix<f32>>(Linear);
            route /F4 ~> /F3 : data fallback [/F4 -> /F2 -> /F3];
        "#;
        let out = compile_source("test_sim.hwc", src, OptLevel::Max, None);
        assert!(!out.diagnostics.has_errors());

        let sim =
            HardwareSimulator::run(&out.report.hir, &out.graph, InjectedFault::LinkDisconnect);
        assert_eq!(
            sim.route_state_transitions,
            vec!["NORMAL", "FAILED", "REPLAN", "FALLBACK", "RECOVERED"]
        );
    }

    #[test]
    fn test_phase2_gpu_kernel_and_queue_dispatch() {
        let src = r#"
            kernel fn vector_add(a: i32, b: i32) -> i32 {
                let tid = a + b;
                return tid;
            }

            fn main() {
                let q = acquire(/F3::queue, Mode::Shared);
                let a = 1;
                let b = 2;
                dispatch(q, vector_add, grid: (16, 16, 1), block: (8, 8, 1), args: [a, b]);
            }
        "#;
        let out = compile_source("test_gpu.hwc", src, OptLevel::Max, None);
        assert!(
            !out.diagnostics.has_errors(),
            "Unexpected errors:\n{}",
            out.diagnostics.format_all()
        );
        let has_dispatch = out.report.hir.instructions.iter().any(|i| {
            matches!(
                i,
                HirInstruction::GpuDispatch {
                    kernel_name,
                    grid,
                    block,
                    ..
                } if kernel_name == "vector_add" && grid.0 == 16 && block.0 == 8
            )
        });
        assert!(has_dispatch, "Expected GPU_DISPATCH in Universal HIR");
    }

    #[test]
    fn test_phase2_interrupt_vector_attachment() {
        let src = r#"
            fn isr_handler() -> i32 {
                return 1;
            }

            fn main() {
                interrupt /F3::interrupt vector 42 attach isr_handler;
            }
        "#;
        let out = compile_source("test_irq.hwc", src, OptLevel::Fast, None);
        assert!(
            !out.diagnostics.has_errors(),
            "Unexpected errors:\n{}",
            out.diagnostics.format_all()
        );
        let has_irq = out.report.hir.instructions.iter().any(|i| {
            matches!(
                i,
                HirInstruction::InterruptAttach {
                    vector: 42,
                    handler_name,
                    ..
                } if handler_name == "isr_handler"
            )
        });
        assert!(has_irq, "Expected INTERRUPT_ATTACH in Universal HIR");
    }

    #[test]
    fn test_phase2_contention_solver_and_visualizer() {
        let src = r#"
            let buf1 = RAM<Matrix<f32>>(Linear);
            let buf2 = RAM<Matrix<f32>>(Linear);
            dma /F4 -> /F3 : buf1;
            dma /F4 -> /F3 : buf2;
        "#;
        let out = compile_source("test_contention.hwc", src, OptLevel::Max, None);
        assert!(!out.diagnostics.has_errors());
        let has_contention = out.report.hir.instructions.iter().any(|i| {
            matches!(
                i,
                HirInstruction::ContentionSchedule {
                    concurrent_ops: 2,
                    ..
                }
            )
        });
        assert!(
            has_contention,
            "Expected CONTENTION_SCHEDULE in Universal HIR"
        );

        let sim = HardwareSimulator::run(&out.report.hir, &out.graph, InjectedFault::None);
        let html = crate::visualizer::generate_interactive_html(&sim, "Contention Test");
        assert!(html.contains("HWCode Physical Hardware Timeline"));
        let json = crate::visualizer::generate_chrome_trace_json(&sim);
        assert!(json.starts_with('[') && json.contains("\"ph\":\"X\""));
    }

    #[test]
    fn test_phase3_distributed_cluster_rdma_route() {
        let src = r#"
            let tensor_data = RAM<Matrix<f32>>(Linear);
            /N0/F3 ~> /N1/F3 : tensor_data;
        "#;
        let out = compile_source("test_cluster.hwc", src, OptLevel::Max, None);
        assert!(!out.diagnostics.has_errors());
        let has_rdma = out.report.hir.instructions.iter().any(|i| {
            matches!(i, HirInstruction::RdmaTransfer { verb, .. } if verb == "IBV_WR_RDMA_WRITE")
        });
        assert!(has_rdma, "Expected RDMA_TRANSFER in Universal HIR");
    }

    #[test]
    fn test_phase3_npu_tensor_packing_layout() {
        let src = r#"
            let host_mat = RAM<Matrix<f32>>(Linear);
            /F1 -> /F8 : host_mat;
        "#;
        let out = compile_source("test_npu.hwc", src, OptLevel::Max, None);
        assert!(!out.diagnostics.has_errors());
        let has_npu_packer = out.report.hir.instructions.iter().any(|i| {
            matches!(
                i,
                HirInstruction::LayoutConvert {
                    to_layout,
                    mechanism,
                    ..
                } if to_layout == "Tensor4D" && mechanism == "NPU_TensorCore_Packer"
            )
        });
        assert!(
            has_npu_packer,
            "Expected LAYOUT_CONVERT via NPU_TensorCore_Packer"
        );
    }

    #[test]
    fn test_phase4_self_hosted_compiler_compilation() {
        let selfhost_src =
            std::fs::read_to_string("selfhost/hwcc.hwc").expect("selfhost/hwcc.hwc must exist");
        let out = compile_source("selfhost/hwcc.hwc", &selfhost_src, OptLevel::Max, None);
        assert!(
            !out.diagnostics.has_errors(),
            "Self-hosted compiler failed physical boundary verification:\n{}",
            out.diagnostics.format_all()
        );
        let c_code = crate::codegen_c::generate_c_code(&out.program, &out.report.hir);
        assert!(c_code.contains("hwcode_execute_physical_plan"));
        assert!(c_code.contains("verify_physical_boundaries"));
        assert!(c_code.contains("emit_universal_hir"));
    }
}
