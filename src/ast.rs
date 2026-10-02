//! Abstract Syntax Tree (AST) for HWCode and the Companion Hardware Definition Language.
//! Covers Sections 2, 4, 5, 6, 7, 8, 9, 10, and 12 of the HWCode specification.

use crate::diagnostics::Span;

/// Semantic Type + Optional Physical Domain & Layout (Section 4 & 5).
/// Examples:
/// - `i32`, `f32`
/// - `Matrix<f32>`
/// - `RAM<Matrix<f32>>(Linear)`
/// - `VRAM<Matrix<f32>>(Tiled2D)`
/// - `Device<Sensor, Active>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSpec {
    pub domain: Option<String>,       // e.g., Some("RAM"), Some("VRAM"), Some("DMABuffer"), Some("MMIO")
    pub base_type: String,            // e.g., "i32", "f32", "Matrix<f32>", "Sensor"
    pub layout_or_state: Option<String>, // e.g., Some("Linear"), Some("Tiled2D"), Some("Active")
}

impl TypeSpec {
    pub fn simple(name: &str) -> Self {
        Self {
            domain: None,
            base_type: name.to_string(),
            layout_or_state: None,
        }
    }

    pub fn display(&self) -> String {
        match (&self.domain, &self.layout_or_state) {
            (Some(dom), Some(lay)) => format!("{}<{}>({})", dom, self.base_type, lay),
            (Some(dom), None) => format!("{}<{}>", dom, self.base_type),
            (None, Some(lay)) => format!("{}<{}>", self.base_type, lay),
            (None, None) => self.base_type.clone(),
        }
    }
}

/// Physical Layout Declaration (Section 4):
/// `layout Tiled2D { block_x: 16; block_y: 16; }`
#[derive(Debug, Clone)]
pub struct LayoutDecl {
    pub name: String,
    pub properties: Vec<(String, i64)>,
    pub span: Span,
}

/// Struct Declaration (Section 2):
/// `struct Packet { id: u32, payload: f32 }`
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<(String, TypeSpec)>,
    pub span: Span,
}

/// Companion Hardware Definition Language `device` block (Section 7 & 12).
#[derive(Debug, Clone)]
pub struct DeviceDecl {
    pub name: String,
    pub bus: Option<String>,
    pub clock_domain: Option<String>,
    pub states: Vec<String>,
    pub initial_state: Option<String>,
    pub registers: Vec<(String, String)>,
    pub transitions: Vec<(String, String, String)>, // (from_state, to_state, trigger)
    pub operations: Vec<(String, String)>,          // (op_name, required_state)
    pub span: Span,
}

/// Function attribute such as `@realtime(deadline: 5ms, period: 10ms, max_power: 15W)`
/// or `@idempotent`, `@replayable`, `@non_replayable`.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<(String, String)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExternFnDecl {
    pub abi: String,
    pub name: String,
    pub params: Vec<(String, TypeSpec)>,
    pub return_type: Option<TypeSpec>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name: String,
    pub is_kernel: bool,
    pub attributes: Vec<Attribute>,
    pub params: Vec<(String, TypeSpec)>,
    pub return_type: Option<TypeSpec>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferMode {
    ExplicitRoute, // `/F1 -> /F2 : data` or `/F1 -> /F2 -> /F3 : data`
    AutoDiscovery, // `/F4 ~> /F3 : data` or `disk ■ gpu : data`
    DmaExplicit,   // `dma /F4 -> /F3 : buffer`
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64, Option<String>, Span),
    Float(f64, Option<String>, Span),
    Str(String, Span),
    Var(String, Span),
    Endpoint(String, Span),
    /// Capability discovery: `find(CPU)`, `find(GPU)`, `find(Storage)`
    Find {
        class_name: String,
        span: Span,
    },
    /// Resource allocation with domain/layout: `RAM<Matrix<f32>>(Linear)`
    DomainAlloc {
        type_spec: TypeSpec,
        span: Span,
    },
    /// Exclusive/Shared resource acquisition: `acquire(/F3::dma, Mode::Exclusive)`
    Acquire {
        endpoint: String,
        mode: String,
        span: Span,
    },
    /// Power/Thermal budget acquisition: `acquire_budget(50W)`
    AcquireBudget {
        watts: f64,
        span: Span,
    },
    /// Linear budget sub-allocation: `split_budget(budget, 20W)`
    SplitBudget {
        parent_var: String,
        watts: f64,
        span: Span,
    },
    /// Clock-domain crossing synchronizer: `doubleflop(sig, FPGA_CLK -> CPU_CLK)`
    ClockSync {
        sync_kind: String, // "doubleflop", "async_fifo", "handshake"
        signal_expr: Box<Expr>,
        target_domain: Option<String>,
        span: Span,
    },
    /// Function or device operation call: `add(1, 2)` or `sensor.read_temperature()`
    Call {
        receiver: Option<String>,
        func: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// Binary expression: `a + b`
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, _, s)
            | Expr::Float(_, _, s)
            | Expr::Str(_, s)
            | Expr::Var(_, s)
            | Expr::Endpoint(_, s)
            | Expr::Find { span: s, .. }
            | Expr::DomainAlloc { span: s, .. }
            | Expr::Acquire { span: s, .. }
            | Expr::AcquireBudget { span: s, .. }
            | Expr::SplitBudget { span: s, .. }
            | Expr::ClockSync { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::Binary { span: s, .. } => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let x: Type = expr;` or `cpu = find(CPU);`
    Let {
        name: String,
        mutable: bool,
        ty: Option<TypeSpec>,
        value: Expr,
        span: Span,
    },
    /// Register write or state trigger: `sensor.REG.START = 1;`
    RegWrite {
        device_var: String,
        reg_name: String,
        value: Expr,
        span: Span,
    },
    /// Hardware transfer / route / DMA statement:
    /// - `disk ~> gpu : data;` (`disk ■ gpu : data;`)
    /// - `/F1 -> /F2 -> /F3 : data;`
    /// - `dma /F4 -> /F3 : buffer;`
    /// - `let ev = dma /F4 -> /F3 : buffer;`
    Transfer {
        event_var: Option<String>,
        mode: TransferMode,
        waypoints: Vec<String>, // endpoints or variable names
        payload: String,
        fallback_path: Option<Vec<String>>,
        constraints: Vec<(String, String)>,
        span: Span,
    },
    /// `await ev;`
    Await {
        event_var: String,
        span: Span,
    },
    /// `release(handle);`
    Release {
        var_name: String,
        span: Span,
    },
    /// `unsafe capability(Cap::MMIO) { ... }` or `unsafe { ... }`
    UnsafeBlock {
        capability: Option<String>,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `trace(target);`
    Trace {
        label: String,
        span: Span,
    },
    /// `return expr;` or implicit tail return expression
    Return {
        value: Option<Expr>,
        span: Span,
    },
    /// Standalone expression statement
    ExprStmt {
        expr: Expr,
        span: Span,
    },
    /// GPU queue dispatch: `dispatch(queue, kernel, grid: (16, 16, 1), block: (16, 16, 1), args: [a, b, c]);`
    Dispatch {
        queue: String,
        kernel_name: String,
        grid: (u32, u32, u32),
        block: (u32, u32, u32),
        args: Vec<Expr>,
        span: Span,
    },
    /// Hardware interrupt vector binding: `interrupt /F3::interrupt vector 42 attach handler_fn;`
    InterruptAttach {
        endpoint: String,
        vector: u32,
        handler_name: String,
        span: Span,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub layouts: Vec<LayoutDecl>,
    pub structs: Vec<StructDecl>,
    pub devices: Vec<DeviceDecl>,
    pub extern_fns: Vec<ExternFnDecl>,
    pub functions: Vec<FnDecl>,
    pub top_level_stmts: Vec<Stmt>,
}
