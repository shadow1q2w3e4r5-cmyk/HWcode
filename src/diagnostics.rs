//! Structured diagnostics, `HardwareError` taxonomy, and Security Audit Trail.
//! Implements Section 10 (Capabilities, Security, Errors, and Reliability)
//! and Section 11/14 Physical Boundary diagnostics.

use std::fmt;

/// Unified HardwareError categories from Section 10, plus compile-time physical boundary violations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareErrorKind {
    // Section 10 Runtime & Hardware Error Categories
    Unsupported,
    Timeout,
    DeviceLost,
    PermissionDenied,
    OutOfMemory,
    BusError,
    DMAError,
    ThermalLimit,
    PowerLimit,
    Corruption,
    Disconnected,
    // Compile-time Physical Boundary Violation Categories (Section 14)
    OwnershipViolation,
    LayoutMismatch,
    ClockDomainHazard,
    InvalidDeviceState,
    BudgetExceeded,
    RealtimeViolation,
    RouteUnavailable,
    SyntaxError,
    TypeError,
}

impl fmt::Display for HardwareErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// The 7 Physical Boundaries from Section 14.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalBoundary {
    Memory,     // Layout and coherency reasoning
    Device,     // Capability and affine ownership reasoning
    Clock,      // Synchronization / CDC reasoning
    Topology,   // Route and recovery reasoning
    State,      // Valid device state transition reasoning
    Power,      // Energy and thermal budget linear accounting
    Temporal,   // Event, deadline, and scheduling reasoning
    Syntax,     // Lexical / grammar level
}

impl fmt::Display for PhysicalBoundary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhysicalBoundary::Memory => write!(f, "Memory Boundary"),
            PhysicalBoundary::Device => write!(f, "Device/Ownership Boundary"),
            PhysicalBoundary::Clock => write!(f, "Clock Boundary"),
            PhysicalBoundary::Topology => write!(f, "Topology Boundary"),
            PhysicalBoundary::State => write!(f, "State Boundary"),
            PhysicalBoundary::Power => write!(f, "Power/Thermal Boundary"),
            PhysicalBoundary::Temporal => write!(f, "Temporal Boundary"),
            PhysicalBoundary::Syntax => write!(f, "Syntax"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "ERROR"),
            Severity::Warning => write!(f, "WARN"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub kind: HardwareErrorKind,
    pub boundary: PhysicalBoundary,
    pub message: String,
    pub span: Span,
    pub explanation: Option<String>,
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn format(&self, filename: &str, source_lines: &[String]) -> String {
        let mut out = format!(
            "[{} {}] ({} @ {}) {}:{}:{}: {}",
            self.severity,
            self.code,
            self.kind,
            self.boundary,
            filename,
            self.span.line,
            self.span.col,
            self.message
        );
        if self.span.line >= 1 && self.span.line <= source_lines.len() {
            let line_str = &source_lines[self.span.line - 1];
            out.push_str(&format!("\n  {:4} | {}", self.span.line, line_str));
            let caret_pad = " ".repeat(self.span.col.saturating_sub(1));
            out.push_str(&format!("\n       | {}^", caret_pad));
        }
        if let Some(why) = &self.explanation {
            out.push_str(&format!("\n  = why: {}", why));
        }
        if let Some(help) = &self.suggestion {
            out.push_str(&format!("\n  = help: {}", help));
        }
        out
    }
}

/// Audit record for `unsafe` capability blocks (Section 10).
#[derive(Debug, Clone)]
pub struct SecurityAuditEntry {
    pub span: Span,
    pub capability: String,
    pub operation: String,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticBag {
    pub filename: String,
    pub source_lines: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub audit_trail: Vec<SecurityAuditEntry>,
}

impl DiagnosticBag {
    pub fn new(filename: &str, source: &str) -> Self {
        Self {
            filename: filename.to_string(),
            source_lines: source.lines().map(|s| s.to_string()).collect(),
            diagnostics: Vec::new(),
            audit_trail: Vec::new(),
        }
    }

    pub fn emit_error(
        &mut self,
        code: &str,
        kind: HardwareErrorKind,
        boundary: PhysicalBoundary,
        span: Span,
        message: impl Into<String>,
        explanation: Option<String>,
        suggestion: Option<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            code: code.to_string(),
            kind,
            boundary,
            message: message.into(),
            span,
            explanation,
            suggestion,
        });
    }

    pub fn emit_warning(
        &mut self,
        code: &str,
        kind: HardwareErrorKind,
        boundary: PhysicalBoundary,
        span: Span,
        message: impl Into<String>,
        explanation: Option<String>,
        suggestion: Option<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            code: code.to_string(),
            kind,
            boundary,
            message: message.into(),
            span,
            explanation,
            suggestion,
        });
    }

    pub fn emit_info(
        &mut self,
        code: &str,
        kind: HardwareErrorKind,
        boundary: PhysicalBoundary,
        span: Span,
        message: impl Into<String>,
        explanation: Option<String>,
        suggestion: Option<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Info,
            code: code.to_string(),
            kind,
            boundary,
            message: message.into(),
            span,
            explanation,
            suggestion,
        });
    }

    pub fn record_audit(&mut self, span: Span, capability: &str, operation: &str, note: &str) {
        self.audit_trail.push(SecurityAuditEntry {
            span,
            capability: capability.to_string(),
            operation: operation.to_string(),
            note: note.to_string(),
        });
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn format_all(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.format(&self.filename, &self.source_lines))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
