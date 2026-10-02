//! HWCode CLI (`hwc`) — Compiler, Universal HIR Inspector, Physical Plan Explainer,
//! Hardware Simulator (with Fault Injection), Driver Scaffolder, and Native C/GCC Builder.

use hwcode::codegen_c::{generate_c_code, generate_driver_scaffold};
use hwcode::compile_source;
use hwcode::explainer::generate_explanation;
use hwcode::hardware_graph::HardwareGraph;
use hwcode::hir::OptLevel;
use hwcode::simulator::{HardwareSimulator, InjectedFault};
use hwcode::visualizer::{generate_chrome_trace_json, generate_interactive_html};
use std::env;
use std::fs;
use std::process::{self, Command};

fn print_usage() {
    println!("HWCode Compiler & Physical Machine Planner (hwc v0.1.0 — Rust Stage 0)");
    println!();
    println!("USAGE:");
    println!("  hwc check <file.hwc>                       Verify all 7 physical boundaries (Types, Layouts, Ownership, CDC, State, Topology, Power)");
    println!("  hwc hir <file.hwc> [--opt fast|max]        Lower program to Universal HIR and display instructions");
    println!("  hwc explain <file.hwc> [--no-p2p]          Explain route selection, layout conversions, barriers, clock syncs, and budgets");
    println!("  hwc simulate <file.hwc> [--fault <kind>]   Run cycle/timeline Hardware Simulator (faults: link_disconnect, dma_error, thermal_throttle, gpu_timeout, memory_corruption)");
    println!("  hwc profile <file.hwc> [--html <out.html>] Export interactive HTML5 visual timeline & Chrome Trace JSON");
    println!("  hwc build <file.hwc> [-o <out.exe>]        Compile .hwc to C11 and build a native executable via GCC");
    println!("  hwc scaffold-driver <file.hwc>             Generate state-safe driver scaffolding from `device` definitions");
    println!("  hwc run [file.hwc]                         Run interactive HWCode application or game in terminal (e.g. Nokia Snake)");
    println!("  hwc graph                                  Display the active Universal Hardware Graph topology & endpoints");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return;
    }

    let cmd = args[1].as_str();
    match cmd {
        "help" | "--help" | "-h" => {
            print_usage();
        }
        "run" => {
            let target = if args.len() >= 3 {
                args[2].clone()
            } else {
                "snake.hwc".to_string()
            };
            hwcode::runner::run_game(&target);
        }
        "graph" => {
            let graph = HardwareGraph::standard_machine();
            println!("=== Universal Hardware Graph Topology ===");
            for id in &graph.node_order {
                if let Some(n) = graph.nodes.get(id) {
                    println!(
                        "  {:<8} | {:<10} | {:<34} | Dom={:<16} | Layout={:<8} | Clk={} ({} MHz)",
                        n.endpoint_id,
                        format!("{}", n.class),
                        n.name,
                        n.memory_domain,
                        n.preferred_layout,
                        n.clock_domain.name,
                        n.clock_domain.frequency_mhz
                    );
                    println!(
                        "           Sub-endpoints: {}",
                        n.sub_resources
                            .iter()
                            .map(|s| format!("{}::{}", n.endpoint_id, s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
            println!("\n=== Physical & Logical Links ===");
            for e in &graph.edges {
                println!(
                    "  {} -> {} | {:<18} | BW={:5.1} GB/s | Lat={:5} ns | DMA={:<5} | Coherent={:<5} | ClkRel={}",
                    e.from,
                    e.to,
                    e.protocol,
                    e.bandwidth_gbps,
                    e.latency_ns,
                    e.dma_available,
                    e.coherent,
                    e.clock_relation
                );
            }
        }
        "check" | "hir" | "explain" | "simulate" | "profile" | "build" | "scaffold-driver" => {
            if args.len() < 3 {
                eprintln!("Error: Missing input `<file.hwc>` for `hwc {}`", cmd);
                process::exit(1);
            }
            let file_path = &args[2];
            let source = match fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading `{}`: {}", file_path, e);
                    process::exit(1);
                }
            };

            let mut opt_level = OptLevel::Max;
            let mut fault = InjectedFault::None;
            let mut disable_p2p = false;
            let mut output_bin: Option<String> = None;
            let mut html_out: Option<String> = None;
            let mut trace_out: Option<String> = None;

            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "--opt" if i + 1 < args.len() => {
                        opt_level = if args[i + 1] == "fast" {
                            OptLevel::Fast
                        } else {
                            OptLevel::Max
                        };
                        i += 2;
                    }
                    "--fault" if i + 1 < args.len() => {
                        fault = InjectedFault::from_str(&args[i + 1]);
                        i += 2;
                    }
                    "--no-p2p" => {
                        disable_p2p = true;
                        i += 1;
                    }
                    "-o" if i + 1 < args.len() => {
                        output_bin = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--html" if i + 1 < args.len() => {
                        html_out = Some(args[i + 1].clone());
                        i += 2;
                    }
                    "--trace" if i + 1 < args.len() => {
                        trace_out = Some(args[i + 1].clone());
                        i += 2;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }

            let mut graph = HardwareGraph::standard_machine();
            if disable_p2p {
                graph
                    .granted_capabilities
                    .retain(|c| c != "Cap::PeerMemory");
            }

            let out = compile_source(file_path, &source, opt_level, Some(graph));

            if !out.diagnostics.diagnostics.is_empty() {
                println!("{}\n", out.diagnostics.format_all());
            }

            if out.diagnostics.has_errors() {
                eprintln!("Compilation aborted due to physical boundary verification errors.");
                process::exit(1);
            }

            match cmd {
                "check" => {
                    println!(
                        "SUCCESS: `{}` passed all 7 physical boundary checks (0 errors).",
                        file_path
                    );
                }
                "hir" => {
                    println!("{}", out.report.hir.dump());
                }
                "explain" => {
                    println!(
                        "{}",
                        generate_explanation(file_path, &out.graph, &out.report, &out.diagnostics)
                    );
                }
                "simulate" => {
                    let sim_res = HardwareSimulator::run(&out.report.hir, &out.graph, fault);
                    println!("{}", sim_res.format_report());
                }
                "profile" => {
                    let sim_res = HardwareSimulator::run(&out.report.hir, &out.graph, fault);
                    let target_html =
                        html_out.unwrap_or_else(|| file_path.replace(".hwc", ".timeline.html"));
                    let html_content = generate_interactive_html(&sim_res, file_path);
                    fs::write(&target_html, &html_content).expect("Failed to write HTML timeline");
                    println!(
                        "[1/2] Generated interactive HTML5 timeline -> `{}`",
                        target_html
                    );

                    let target_trace =
                        trace_out.unwrap_or_else(|| file_path.replace(".hwc", ".trace.json"));
                    let trace_content = generate_chrome_trace_json(&sim_res);
                    fs::write(&target_trace, &trace_content)
                        .expect("Failed to write Chrome trace JSON");
                    println!(
                        "[2/2] Generated Chrome Trace / Perfetto profile -> `{}`",
                        target_trace
                    );
                    println!("\nOpen `{}` in your browser to inspect device tracks, or load `{}` in chrome://tracing!", target_html, target_trace);
                }
                "scaffold-driver" => {
                    println!("{}", generate_driver_scaffold(&out.program));
                }
                "build" => {
                    let c_code = generate_c_code(&out.program, &out.report.hir);
                    let c_path = file_path.replace(".hwc", ".gen.c");
                    fs::write(&c_path, &c_code).expect("Failed to write generated C file");
                    println!("[1/2] Emitted C11 backend code -> `{}`", c_path);

                    let exe_path = output_bin.unwrap_or_else(|| file_path.replace(".hwc", ".exe"));
                    let status = Command::new("gcc")
                        .args([&c_path, "-O2", "-o", &exe_path])
                        .status();

                    match status {
                        Ok(s) if s.success() => {
                            println!("[2/2] Compiled native binary via GCC -> `{}`", exe_path);
                        }
                        _ => {
                            println!(
                                "[2/2] C source generated at `{}` (run `gcc {} -o {}` to link)",
                                c_path, c_path, exe_path
                            );
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        other => {
            eprintln!("Unknown command `{}`", other);
            print_usage();
            process::exit(1);
        }
    }
}
