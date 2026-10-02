//! Recursive-descent Parser for HWCode (.hwc) and Companion Hardware Definition Language (.hwd).

use crate::ast::*;
use crate::diagnostics::{DiagnosticBag, HardwareErrorKind, PhysicalBoundary, Span};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn peek_n(&self, offset: usize) -> &TokenKind {
        &self
            .tokens
            .get(self.pos + offset)
            .unwrap_or_else(|| self.tokens.last().unwrap())
            .kind
    }

    fn span(&self) -> Span {
        self.peek().span
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn eat(&mut self, expected: &TokenKind) -> bool {
        if self.peek_kind() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_ident(&mut self, diags: &mut DiagnosticBag, context: &str) -> String {
        let span = self.span();
        match self.peek_kind().clone() {
            TokenKind::Ident(s) => {
                self.advance();
                s
            }
            TokenKind::Dma => {
                self.advance();
                "dma".to_string()
            }
            TokenKind::Register => {
                self.advance();
                "register".to_string()
            }
            TokenKind::Op => {
                self.advance();
                "op".to_string()
            }
            other => {
                diags.emit_error(
                    "E0001",
                    HardwareErrorKind::SyntaxError,
                    PhysicalBoundary::Syntax,
                    span,
                    format!("Expected identifier in {}, found {:?}", context, other),
                    None,
                    None,
                );
                self.advance();
                "_error_".to_string()
            }
        }
    }

    pub fn parse_program(&mut self, diags: &mut DiagnosticBag) -> Program {
        let mut prog = Program::default();
        let mut pending_attrs: Vec<Attribute> = Vec::new();

        while *self.peek_kind() != TokenKind::Eof {
            match self.peek_kind() {
                TokenKind::At => {
                    let attr = self.parse_attribute(diags);
                    pending_attrs.push(attr);
                }
                TokenKind::Layout => {
                    prog.layouts.push(self.parse_layout(diags));
                }
                TokenKind::Struct => {
                    prog.structs.push(self.parse_struct(diags));
                }
                TokenKind::Device => {
                    prog.devices.push(self.parse_device(diags));
                }
                TokenKind::Extern => {
                    prog.extern_fns.push(self.parse_extern_fn(diags));
                }
                TokenKind::Kernel => {
                    self.advance();
                    let attrs = std::mem::take(&mut pending_attrs);
                    prog.functions.push(self.parse_fn(attrs, true, diags));
                }
                TokenKind::Fn => {
                    let attrs = std::mem::take(&mut pending_attrs);
                    prog.functions.push(self.parse_fn(attrs, false, diags));
                }
                _ => {
                    let stmt = self.parse_stmt(diags);
                    prog.top_level_stmts.push(stmt);
                }
            }
        }
        prog
    }

    fn parse_attribute(&mut self, diags: &mut DiagnosticBag) -> Attribute {
        let span = self.span();
        self.eat(&TokenKind::At);
        let name = self.expect_ident(diags, "attribute");
        let mut args = Vec::new();
        if self.eat(&TokenKind::LParen) {
            while *self.peek_kind() != TokenKind::RParen && *self.peek_kind() != TokenKind::Eof {
                let key = self.expect_ident(diags, "attribute key");
                self.eat(&TokenKind::Colon);
                let val = self.parse_raw_token_string();
                args.push((key, val));
                self.eat(&TokenKind::Comma);
            }
            self.eat(&TokenKind::RParen);
        }
        Attribute { name, args, span }
    }

    fn parse_raw_token_string(&mut self) -> String {
        match self.advance().kind {
            TokenKind::Ident(s) => s,
            TokenKind::IntLit(v, Some(u)) => format!("{}{}", v, u),
            TokenKind::IntLit(v, None) => v.to_string(),
            TokenKind::FloatLit(v, Some(u)) => format!("{}{}", v, u),
            TokenKind::FloatLit(v, None) => v.to_string(),
            TokenKind::StringLit(s) => s,
            TokenKind::EndpointLit(e) => e,
            other => format!("{:?}", other),
        }
    }

    /// Parse `layout Tiled2D { block_x: 16; block_y: 16; }`
    fn parse_layout(&mut self, diags: &mut DiagnosticBag) -> LayoutDecl {
        let span = self.span();
        self.eat(&TokenKind::Layout);
        let name = self.expect_ident(diags, "layout declaration");
        let mut properties = Vec::new();
        self.eat(&TokenKind::LBrace);
        while *self.peek_kind() != TokenKind::RBrace && *self.peek_kind() != TokenKind::Eof {
            let prop_name = self.expect_ident(diags, "layout property");
            self.eat(&TokenKind::Colon);
            let val = match self.advance().kind {
                TokenKind::IntLit(v, _) => v,
                _ => 0,
            };
            properties.push((prop_name, val));
            if !self.eat(&TokenKind::Semicolon) {
                self.eat(&TokenKind::Comma);
            }
        }
        self.eat(&TokenKind::RBrace);
        LayoutDecl {
            name,
            properties,
            span,
        }
    }

    /// Parse `struct Name { field: Type, ... }`
    fn parse_struct(&mut self, diags: &mut DiagnosticBag) -> StructDecl {
        let span = self.span();
        self.eat(&TokenKind::Struct);
        let name = self.expect_ident(diags, "struct declaration");
        let mut fields = Vec::new();
        self.eat(&TokenKind::LBrace);
        while *self.peek_kind() != TokenKind::RBrace && *self.peek_kind() != TokenKind::Eof {
            let fname = self.expect_ident(diags, "struct field");
            self.eat(&TokenKind::Colon);
            let ftype = self.parse_type_spec(diags);
            fields.push((fname, ftype));
            if !self.eat(&TokenKind::Comma) {
                self.eat(&TokenKind::Semicolon);
            }
        }
        self.eat(&TokenKind::RBrace);
        StructDecl { name, fields, span }
    }

    /// Parse Companion Hardware Definition Language `device Name { ... }` (Section 7 & 12)
    fn parse_device(&mut self, diags: &mut DiagnosticBag) -> DeviceDecl {
        let span = self.span();
        self.eat(&TokenKind::Device);
        let name = self.expect_ident(diags, "device declaration");
        let mut bus = None;
        let mut clock_domain = None;
        let mut states = Vec::new();
        let mut initial_state = None;
        let mut registers = Vec::new();
        let mut transitions = Vec::new();
        let mut operations = Vec::new();

        self.eat(&TokenKind::LBrace);
        while *self.peek_kind() != TokenKind::RBrace && *self.peek_kind() != TokenKind::Eof {
            match self.peek_kind().clone() {
                TokenKind::States => {
                    self.advance();
                    self.eat(&TokenKind::Colon);
                    self.eat(&TokenKind::LBracket);
                    while *self.peek_kind() != TokenKind::RBracket
                        && *self.peek_kind() != TokenKind::Eof
                    {
                        let st = self.expect_ident(diags, "state name");
                        if states.is_empty() && initial_state.is_none() {
                            initial_state = Some(st.clone());
                        }
                        states.push(st);
                        self.eat(&TokenKind::Comma);
                    }
                    self.eat(&TokenKind::RBracket);
                    self.eat(&TokenKind::Semicolon);
                }
                TokenKind::Register => {
                    self.advance();
                    let reg_name = self.expect_ident(diags, "register name");
                    self.eat(&TokenKind::Colon);
                    let reg_ty = self.expect_ident(diags, "register type");
                    registers.push((reg_name, reg_ty));
                    self.eat(&TokenKind::Semicolon);
                }
                TokenKind::Transition => {
                    // `transition Idle -> Active on REG.START = 1;`
                    self.advance();
                    let from_st = self.expect_ident(diags, "transition source state");
                    self.eat(&TokenKind::Arrow);
                    let to_st = self.expect_ident(diags, "transition target state");
                    // optional `on ...` up to `;`
                    let mut trigger = String::new();
                    if let TokenKind::Ident(w) = self.peek_kind() {
                        if w == "on" || w == "when" {
                            self.advance();
                        }
                    }
                    while *self.peek_kind() != TokenKind::Semicolon
                        && *self.peek_kind() != TokenKind::RBrace
                        && *self.peek_kind() != TokenKind::Eof
                    {
                        let tok = self.advance();
                        match tok.kind {
                            TokenKind::Ident(s) => trigger.push_str(&s),
                            TokenKind::Dot => trigger.push('.'),
                            TokenKind::Assign => trigger.push_str(" = "),
                            TokenKind::IntLit(v, _) => trigger.push_str(&v.to_string()),
                            _ => {}
                        }
                    }
                    self.eat(&TokenKind::Semicolon);
                    transitions.push((from_st, to_st, trigger.trim().to_string()));
                }
                TokenKind::Op => {
                    // `op read_temperature requires Active;`
                    self.advance();
                    let op_name = self.expect_ident(diags, "device operation name");
                    self.eat(&TokenKind::Requires);
                    let req_state = self.expect_ident(diags, "required device state");
                    operations.push((op_name, req_state));
                    self.eat(&TokenKind::Semicolon);
                }
                TokenKind::Ident(key) => {
                    self.advance();
                    self.eat(&TokenKind::Colon);
                    let val = self.expect_ident(diags, "device property value");
                    match key.as_str() {
                        "bus" => bus = Some(val),
                        "clock" => clock_domain = Some(val),
                        "initial" => initial_state = Some(val),
                        _ => {}
                    }
                    self.eat(&TokenKind::Semicolon);
                }
                _ => {
                    self.advance();
                }
            }
        }
        self.eat(&TokenKind::RBrace);

        DeviceDecl {
            name,
            bus,
            clock_domain,
            states,
            initial_state,
            registers,
            transitions,
            operations,
            span,
        }
    }

    fn parse_extern_fn(&mut self, diags: &mut DiagnosticBag) -> ExternFnDecl {
        let span = self.span();
        self.eat(&TokenKind::Extern);
        let abi = match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                self.advance();
                s
            }
            _ => "C".to_string(),
        };
        self.eat(&TokenKind::Fn);
        let name = self.expect_ident(diags, "extern fn name");
        let params = self.parse_param_list(diags);
        let return_type = if self.eat(&TokenKind::Arrow) {
            Some(self.parse_type_spec(diags))
        } else {
            None
        };
        self.eat(&TokenKind::Semicolon);
        ExternFnDecl {
            abi,
            name,
            params,
            return_type,
            span,
        }
    }

    fn parse_fn(&mut self, attributes: Vec<Attribute>, is_kernel: bool, diags: &mut DiagnosticBag) -> FnDecl {
        let span = self.span();
        self.eat(&TokenKind::Fn);
        let name = self.expect_ident(diags, "function name");
        let params = self.parse_param_list(diags);
        let return_type = if self.eat(&TokenKind::Arrow) {
            Some(self.parse_type_spec(diags))
        } else {
            None
        };
        let body = self.parse_block(diags);
        FnDecl {
            name,
            is_kernel,
            attributes,
            params,
            return_type,
            body,
            span,
        }
    }

    fn parse_param_list(&mut self, diags: &mut DiagnosticBag) -> Vec<(String, TypeSpec)> {
        let mut params = Vec::new();
        self.eat(&TokenKind::LParen);
        while *self.peek_kind() != TokenKind::RParen && *self.peek_kind() != TokenKind::Eof {
            let pname = self.expect_ident(diags, "parameter name");
            self.eat(&TokenKind::Colon);
            let ptype = self.parse_type_spec(diags);
            params.push((pname, ptype));
            self.eat(&TokenKind::Comma);
        }
        self.eat(&TokenKind::RParen);
        params
    }

    /// Parses types such as:
    /// - `i32`, `f32`, `&mut Buffer`
    /// - `Matrix<f32>`
    /// - `RAM<Matrix<f32>>(Linear)`
    /// - `VRAM<Matrix<f32>>(Tiled2D)`
    /// - `Device<Sensor, Active>`
    pub fn parse_type_spec(&mut self, diags: &mut DiagnosticBag) -> TypeSpec {
        let mut prefix_ref = String::new();
        if self.eat(&TokenKind::Ampersand) {
            prefix_ref.push('&');
            if self.eat(&TokenKind::Mut) {
                prefix_ref.push_str("mut ");
            }
        } else if self.eat(&TokenKind::Star) {
            prefix_ref.push('*');
        }

        let outer = self.expect_ident(diags, "type name");
        let is_memory_domain = matches!(
            outer.as_str(),
            "RAM"
                | "SystemRAM"
                | "VRAM"
                | "DeviceRAM"
                | "PersistentMemory"
                | "SharedMemory"
                | "LocalMemory"
                | "MMIO"
                | "Cache"
                | "PinnedRAM"
                | "DMABuffer"
                | "Device"
        );

        if self.eat(&TokenKind::Lt) {
            let inner_first = self.expect_ident(diags, "generic type parameter");
            let mut inner_str = inner_first;
            let mut second_param = None;

            if self.eat(&TokenKind::Lt) {
                let nested = self.expect_ident(diags, "nested generic parameter");
                self.eat(&TokenKind::Gt);
                inner_str = format!("{}<{}>", inner_str, nested);
            }
            if self.eat(&TokenKind::Comma) {
                second_param = Some(self.expect_ident(diags, "state or layout parameter"));
            }
            self.eat(&TokenKind::Gt);

            let mut layout = second_param;
            if self.eat(&TokenKind::LParen) {
                layout = Some(self.expect_ident(diags, "layout name"));
                self.eat(&TokenKind::RParen);
            }

            if is_memory_domain {
                TypeSpec {
                    domain: Some(format!("{}{}", prefix_ref, outer)),
                    base_type: inner_str,
                    layout_or_state: layout,
                }
            } else {
                TypeSpec {
                    domain: None,
                    base_type: format!("{}{}<{}>", prefix_ref, outer, inner_str),
                    layout_or_state: layout,
                }
            }
        } else {
            let mut layout = None;
            if self.eat(&TokenKind::LParen) {
                layout = Some(self.expect_ident(diags, "layout name"));
                self.eat(&TokenKind::RParen);
            }
            TypeSpec {
                domain: None,
                base_type: format!("{}{}", prefix_ref, outer),
                layout_or_state: layout,
            }
        }
    }

    fn parse_block(&mut self, diags: &mut DiagnosticBag) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        self.eat(&TokenKind::LBrace);
        while *self.peek_kind() != TokenKind::RBrace && *self.peek_kind() != TokenKind::Eof {
            stmts.push(self.parse_stmt(diags));
        }
        self.eat(&TokenKind::RBrace);
        stmts
    }

    fn parse_stmt(&mut self, diags: &mut DiagnosticBag) -> Stmt {
        let span = self.span();

        match self.peek_kind().clone() {
            TokenKind::Let => {
                self.advance();
                let mutable = self.eat(&TokenKind::Mut);
                let name = self.expect_ident(diags, "let binding");
                let ty = if self.eat(&TokenKind::Colon) {
                    Some(self.parse_type_spec(diags))
                } else {
                    None
                };
                self.eat(&TokenKind::Assign);

                // Check if right-hand side is an async `dma ...` transfer producing an event:
                // `let ev = dma /F4 -> /F3 : buffer;`
                if *self.peek_kind() == TokenKind::Dma {
                    return self.parse_transfer_stmt(Some(name), diags);
                }

                let value = self.parse_expr(diags);
                self.eat(&TokenKind::Semicolon);
                Stmt::Let {
                    name,
                    mutable,
                    ty,
                    value,
                    span,
                }
            }
            TokenKind::Dma | TokenKind::Route => self.parse_transfer_stmt(None, diags),
            TokenKind::EndpointLit(_) => self.parse_transfer_stmt(None, diags),
            TokenKind::Await => {
                self.advance();
                let event_var = self.expect_ident(diags, "await event");
                self.eat(&TokenKind::Semicolon);
                Stmt::Await { event_var, span }
            }
            TokenKind::Dispatch => {
                self.advance();
                self.eat(&TokenKind::LParen);
                let queue = self.parse_endpoint_or_var(diags);
                self.eat(&TokenKind::Comma);
                let kernel_name = self.expect_ident(diags, "kernel name");
                let mut grid = (1u32, 1u32, 1u32);
                let mut block = (1u32, 1u32, 1u32);
                let mut args = Vec::new();

                while self.eat(&TokenKind::Comma) {
                    if *self.peek_kind() == TokenKind::RParen {
                        break;
                    }
                    let key = self.expect_ident(diags, "dispatch parameter");
                    self.eat(&TokenKind::Colon);
                    match key.as_str() {
                        "grid" => {
                            grid = self.parse_dim3();
                        }
                        "block" => {
                            block = self.parse_dim3();
                        }
                        "args" => {
                            self.eat(&TokenKind::LBracket);
                            while *self.peek_kind() != TokenKind::RBracket && *self.peek_kind() != TokenKind::Eof {
                                args.push(self.parse_expr(diags));
                                self.eat(&TokenKind::Comma);
                            }
                            self.eat(&TokenKind::RBracket);
                        }
                        _ => {}
                    }
                }
                self.eat(&TokenKind::RParen);
                self.eat(&TokenKind::Semicolon);
                Stmt::Dispatch {
                    queue,
                    kernel_name,
                    grid,
                    block,
                    args,
                    span,
                }
            }
            TokenKind::Interrupt => {
                self.advance();
                let endpoint = self.parse_endpoint_or_var(diags);
                let mut vector = 0u32;
                if let TokenKind::Ident(w) = self.peek_kind().clone() {
                    if w == "vector" || w == "vec" {
                        self.advance();
                        if let TokenKind::IntLit(v, _) = self.advance().kind {
                            vector = v as u32;
                        }
                    }
                }
                let mut handler_name = String::new();
                if let TokenKind::Ident(w) = self.peek_kind().clone() {
                    if w == "attach" || w == "handle" {
                        self.advance();
                        handler_name = self.expect_ident(diags, "interrupt handler function name");
                    }
                }
                self.eat(&TokenKind::Semicolon);
                Stmt::InterruptAttach {
                    endpoint,
                    vector,
                    handler_name,
                    span,
                }
            }
            TokenKind::Release => {
                self.advance();
                self.eat(&TokenKind::LParen);
                let var_name = self.expect_ident(diags, "release resource");
                self.eat(&TokenKind::RParen);
                self.eat(&TokenKind::Semicolon);
                Stmt::Release { var_name, span }
            }
            TokenKind::Unsafe => {
                self.advance();
                let mut capability = None;
                if let TokenKind::Ident(kw) = self.peek_kind().clone() {
                    if kw == "capability" {
                        self.advance();
                        self.eat(&TokenKind::LParen);
                        capability = Some(self.parse_scoped_ident(diags));
                        self.eat(&TokenKind::RParen);
                    }
                } else if self.eat(&TokenKind::LParen) {
                    capability = Some(self.parse_scoped_ident(diags));
                    self.eat(&TokenKind::RParen);
                }
                let body = self.parse_block(diags);
                Stmt::UnsafeBlock {
                    capability,
                    body,
                    span,
                }
            }
            TokenKind::Trace => {
                self.advance();
                self.eat(&TokenKind::LParen);
                let label = match self.peek_kind().clone() {
                    TokenKind::StringLit(s) => {
                        self.advance();
                        s
                    }
                    TokenKind::Ident(s) => {
                        self.advance();
                        s
                    }
                    _ => "trace_point".to_string(),
                };
                self.eat(&TokenKind::RParen);
                self.eat(&TokenKind::Semicolon);
                Stmt::Trace { label, span }
            }
            TokenKind::Return => {
                self.advance();
                if self.eat(&TokenKind::Semicolon) {
                    Stmt::Return { value: None, span }
                } else {
                    let expr = self.parse_expr(diags);
                    self.eat(&TokenKind::Semicolon);
                    Stmt::Return {
                        value: Some(expr),
                        span,
                    }
                }
            }
            TokenKind::Ident(_) => {
                // Disambiguate:
                // 1) `cpu = find(CPU);` (bare assignment / declaration as on Page 3)
                // 2) `sensor.REG.START = 1;` (register write as on Page 7)
                // 3) `disk ~> gpu : data;` or `disk ■ gpu : data;` or `disk -> ram -> gpu : data;`
                // 4) Expression statement or implicit return `a + b`
                if *self.peek_n(1) == TokenKind::Assign {
                    let name = self.expect_ident(diags, "assignment target");
                    self.eat(&TokenKind::Assign);
                    if *self.peek_kind() == TokenKind::Dma {
                        return self.parse_transfer_stmt(Some(name), diags);
                    }
                    let value = self.parse_expr(diags);
                    self.eat(&TokenKind::Semicolon);
                    return Stmt::Let {
                        name,
                        mutable: false,
                        ty: None,
                        value,
                        span,
                    };
                }

                if *self.peek_n(1) == TokenKind::AutoRoute || *self.peek_n(1) == TokenKind::Arrow {
                    return self.parse_transfer_stmt(None, diags);
                }

                if *self.peek_n(1) == TokenKind::Dot && *self.peek_n(3) == TokenKind::Dot {
                    // `sensor.REG.START = 1;`
                    let device_var = self.expect_ident(diags, "device variable");
                    self.eat(&TokenKind::Dot);
                    let _reg_group = self.expect_ident(diags, "REG");
                    self.eat(&TokenKind::Dot);
                    let reg_name = self.expect_ident(diags, "register name");
                    self.eat(&TokenKind::Assign);
                    let value = self.parse_expr(diags);
                    self.eat(&TokenKind::Semicolon);
                    return Stmt::RegWrite {
                        device_var,
                        reg_name,
                        value,
                        span,
                    };
                }

                let expr = self.parse_expr(diags);
                if self.eat(&TokenKind::Semicolon) {
                    Stmt::ExprStmt { expr, span }
                } else {
                    // Implicit tail return expression (e.g. `fn add(a: i32, b: i32) -> i32 { a + b }`)
                    Stmt::Return {
                        value: Some(expr),
                        span,
                    }
                }
            }
            _ => {
                let expr = self.parse_expr(diags);
                self.eat(&TokenKind::Semicolon);
                Stmt::ExprStmt { expr, span }
            }
        }
    }

    /// Parses hardware communication / route / DMA statements:
    /// - `disk ~> gpu : data;` (`disk ■ gpu : data;`)
    /// - `/F4 ~> /F3 : data;`
    /// - `/F1 -> /F2 -> /F3 : data;`
    /// - `dma /F4 -> /F3 : buffer;`
    /// - Optional `fallback [/F4 -> /F2 -> /F3]` and `with { deadline: 5ms, power: 20W }`
    fn parse_transfer_stmt(
        &mut self,
        event_var: Option<String>,
        diags: &mut DiagnosticBag,
    ) -> Stmt {
        let span = self.span();
        let mut is_dma = false;
        if self.eat(&TokenKind::Dma) {
            is_dma = true;
        } else {
            self.eat(&TokenKind::Route);
        }

        let mut waypoints = Vec::new();
        waypoints.push(self.parse_endpoint_or_var(diags));

        let mut mode = if is_dma {
            TransferMode::DmaExplicit
        } else {
            TransferMode::ExplicitRoute
        };

        while *self.peek_kind() == TokenKind::Arrow || *self.peek_kind() == TokenKind::AutoRoute {
            if self.eat(&TokenKind::AutoRoute) {
                if !is_dma {
                    mode = TransferMode::AutoDiscovery;
                }
            } else {
                self.eat(&TokenKind::Arrow);
            }
            waypoints.push(self.parse_endpoint_or_var(diags));
        }

        let payload = if self.eat(&TokenKind::Colon) {
            self.expect_ident(diags, "transfer payload variable")
        } else {
            "unit_signal".to_string()
        };

        let mut fallback_path = None;
        if self.eat(&TokenKind::Fallback) {
            self.eat(&TokenKind::LBracket);
            let mut fb = Vec::new();
            while *self.peek_kind() != TokenKind::RBracket && *self.peek_kind() != TokenKind::Eof {
                fb.push(self.parse_endpoint_or_var(diags));
                if !self.eat(&TokenKind::Arrow) {
                    self.eat(&TokenKind::AutoRoute);
                }
            }
            self.eat(&TokenKind::RBracket);
            fallback_path = Some(fb);
        }

        let mut constraints = Vec::new();
        if self.eat(&TokenKind::With) {
            self.eat(&TokenKind::LBrace);
            while *self.peek_kind() != TokenKind::RBrace && *self.peek_kind() != TokenKind::Eof {
                let k = self.expect_ident(diags, "constraint key");
                self.eat(&TokenKind::Colon);
                let v = self.parse_raw_token_string();
                constraints.push((k, v));
                if !self.eat(&TokenKind::Comma) {
                    self.eat(&TokenKind::Semicolon);
                }
            }
            self.eat(&TokenKind::RBrace);
        }

        self.eat(&TokenKind::Semicolon);

        Stmt::Transfer {
            event_var,
            mode,
            waypoints,
            payload,
            fallback_path,
            constraints,
            span,
        }
    }

    fn parse_endpoint_or_var(&mut self, diags: &mut DiagnosticBag) -> String {
        match self.peek_kind().clone() {
            TokenKind::EndpointLit(ep) => {
                self.advance();
                ep
            }
            TokenKind::Ident(id) => {
                self.advance();
                if self.eat(&TokenKind::DoubleColon) {
                    let sub = self.expect_ident(diags, "sub-resource");
                    format!("{}::{}", id, sub)
                } else {
                    id
                }
            }
            _ => {
                self.advance();
                "/F1".to_string()
            }
        }
    }

    fn parse_scoped_ident(&mut self, diags: &mut DiagnosticBag) -> String {
        let first = self.expect_ident(diags, "identifier");
        if self.eat(&TokenKind::DoubleColon) {
            let second = self.expect_ident(diags, "scoped member");
            format!("{}::{}", first, second)
        } else {
            first
        }
    }

    pub fn parse_expr(&mut self, diags: &mut DiagnosticBag) -> Expr {
        let mut left = self.parse_primary(diags);

        while matches!(
            self.peek_kind(),
            TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::EqEq
                | TokenKind::NotEq
                | TokenKind::Lt
                | TokenKind::Gt
                | TokenKind::Le
                | TokenKind::Ge
        ) {
            let span = self.span();
            let op = match self.advance().kind {
                TokenKind::Plus => "+",
                TokenKind::Minus => "-",
                TokenKind::Star => "*",
                TokenKind::Slash => "/",
                TokenKind::EqEq => "==",
                TokenKind::NotEq => "!=",
                TokenKind::Lt => "<",
                TokenKind::Gt => ">",
                TokenKind::Le => "<=",
                TokenKind::Ge => ">=",
                _ => "+",
            }
            .to_string();
            let right = self.parse_primary(diags);
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }

        left
    }

    fn parse_primary(&mut self, diags: &mut DiagnosticBag) -> Expr {
        let span = self.span();
        match self.peek_kind().clone() {
            TokenKind::IntLit(v, unit) => {
                self.advance();
                Expr::Int(v, unit, span)
            }
            TokenKind::FloatLit(v, unit) => {
                self.advance();
                Expr::Float(v, unit, span)
            }
            TokenKind::StringLit(s) => {
                self.advance();
                Expr::Str(s, span)
            }
            TokenKind::EndpointLit(ep) => {
                self.advance();
                Expr::Endpoint(ep, span)
            }
            TokenKind::Find => {
                self.advance();
                self.eat(&TokenKind::LParen);
                let class_name = self.expect_ident(diags, "find device class");
                self.eat(&TokenKind::RParen);
                Expr::Find { class_name, span }
            }
            TokenKind::Acquire => {
                self.advance();
                self.eat(&TokenKind::LParen);
                let endpoint = self.parse_endpoint_or_var(diags);
                let mut mode = "Mode::Exclusive".to_string();
                if self.eat(&TokenKind::Comma) {
                    mode = self.parse_scoped_ident(diags);
                }
                self.eat(&TokenKind::RParen);
                Expr::Acquire {
                    endpoint,
                    mode,
                    span,
                }
            }
            TokenKind::AcquireBudget => {
                self.advance();
                self.eat(&TokenKind::LParen);
                let watts = self.parse_numeric_value();
                self.eat(&TokenKind::RParen);
                Expr::AcquireBudget { watts, span }
            }
            TokenKind::SplitBudget => {
                self.advance();
                self.eat(&TokenKind::LParen);
                let parent_var = self.expect_ident(diags, "parent budget variable");
                self.eat(&TokenKind::Comma);
                let watts = self.parse_numeric_value();
                self.eat(&TokenKind::RParen);
                Expr::SplitBudget {
                    parent_var,
                    watts,
                    span,
                }
            }
            TokenKind::Ident(name) => {
                // Check for DomainAlloc: `RAM<Matrix<f32>>(Linear)` or `VRAM<f32>` or `Device<Sensor, Idle>`
                let is_domain_ctor = matches!(
                    name.as_str(),
                    "RAM"
                        | "SystemRAM"
                        | "VRAM"
                        | "DeviceRAM"
                        | "PersistentMemory"
                        | "SharedMemory"
                        | "LocalMemory"
                        | "MMIO"
                        | "Cache"
                        | "PinnedRAM"
                        | "DMABuffer"
                        | "Device"
                ) && *self.peek_n(1) == TokenKind::Lt;

                if is_domain_ctor {
                    let type_spec = self.parse_type_spec(diags);
                    return Expr::DomainAlloc { type_spec, span };
                }

                // Check for Clock Synchronizers (Section 6): `doubleflop(...)`, `async_fifo(...)`, `handshake(...)`
                if matches!(name.as_str(), "doubleflop" | "async_fifo" | "handshake")
                    && *self.peek_n(1) == TokenKind::LParen
                {
                    self.advance();
                    self.eat(&TokenKind::LParen);
                    let signal_expr = Box::new(self.parse_expr(diags));
                    let mut target_domain = None;
                    if self.eat(&TokenKind::Comma) {
                        target_domain = Some(self.expect_ident(diags, "target clock domain"));
                    }
                    self.eat(&TokenKind::RParen);
                    return Expr::ClockSync {
                        sync_kind: name,
                        signal_expr,
                        target_domain,
                        span,
                    };
                }

                self.advance();

                // Method call: `sensor.read_temperature()`
                if self.eat(&TokenKind::Dot) {
                    let method = self.expect_ident(diags, "method or operation name");
                    let mut args = Vec::new();
                    if self.eat(&TokenKind::LParen) {
                        while *self.peek_kind() != TokenKind::RParen
                            && *self.peek_kind() != TokenKind::Eof
                        {
                            args.push(self.parse_expr(diags));
                            self.eat(&TokenKind::Comma);
                        }
                        self.eat(&TokenKind::RParen);
                    }
                    return Expr::Call {
                        receiver: Some(name),
                        func: method,
                        args,
                        span,
                    };
                }

                // Function call: `add(1, 2)`
                if self.eat(&TokenKind::LParen) {
                    let mut args = Vec::new();
                    while *self.peek_kind() != TokenKind::RParen
                        && *self.peek_kind() != TokenKind::Eof
                    {
                        args.push(self.parse_expr(diags));
                        self.eat(&TokenKind::Comma);
                    }
                    self.eat(&TokenKind::RParen);
                    return Expr::Call {
                        receiver: None,
                        func: name,
                        args,
                        span,
                    };
                }

                Expr::Var(name, span)
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr(diags);
                self.eat(&TokenKind::RParen);
                inner
            }
            _ => {
                self.advance();
                Expr::Int(0, None, span)
            }
        }
    }

    fn parse_numeric_value(&mut self) -> f64 {
        match self.advance().kind {
            TokenKind::IntLit(v, _) => v as f64,
            TokenKind::FloatLit(v, _) => v,
            _ => 0.0,
        }
    }

    fn parse_dim3(&mut self) -> (u32, u32, u32) {
        if self.eat(&TokenKind::LParen) {
            let x = if let TokenKind::IntLit(v, _) = self.advance().kind { v as u32 } else { 1 };
            let mut y = 1;
            let mut z = 1;
            if self.eat(&TokenKind::Comma) {
                if let TokenKind::IntLit(v, _) = self.advance().kind { y = v as u32; }
            }
            if self.eat(&TokenKind::Comma) {
                if let TokenKind::IntLit(v, _) = self.advance().kind { z = v as u32; }
            }
            self.eat(&TokenKind::RParen);
            (x, y, z)
        } else if let TokenKind::IntLit(v, _) = self.advance().kind {
            (v as u32, 1, 1)
        } else {
            (1, 1, 1)
        }
    }
}
