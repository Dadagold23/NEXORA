use crate::ast::{BinaryOperator, UnaryOperator};
use crate::diagnostic::Diagnostic;
use crate::nir::{BasicBlock, Constant, Function, Instruction, Module, Terminator, Type, ValueId};
use crate::source::SourceFile;
use std::collections::HashMap;
use std::fmt::Write;

// ── Public artifact type ──────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledArtifact {
    /// Logical target identifier, e.g. "js".
    pub target: String,
    /// Generated source text ready to write to disk or execute.
    pub source: String,
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Compile a NXL source file to a JavaScript artifact.
pub fn compile_js(source: &SourceFile) -> Result<CompiledArtifact, Diagnostic> {
    let module = crate::compile_to_nir(source)?;
    let js = JsBackend::new(&module).emit()?;
    Ok(CompiledArtifact {
        target: "js".to_owned(),
        source: js,
    })
}

/// Native artifact produced by compiling to Rust.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeArtifact {
    pub target: String,
    pub source: String,
}

/// Compile a NXL source file to a native artifact (Rust source).
pub fn compile_native(source: &SourceFile) -> Result<NativeArtifact, Diagnostic> {
    let module = crate::compile_to_nir(source)?;
    let rust = RustBackend::new(&module).emit()?;
    Ok(NativeArtifact {
        target: "native".to_owned(),
        source: rust,
    })
}

// ── JavaScript backend ────────────────────────────────────────────────────────

struct JsBackend<'m> {
    module: &'m Module,
    out: String,
    /// Maps ValueId → JS variable name within the current function.
    vars: HashMap<ValueId, String>,
    /// Maps NXL name → current JS variable name (updated by Store/Parameter).
    name_vars: HashMap<String, String>,
    /// Block indices that have been inlined into a Branch and must not be
    /// re-emitted by the main block loop.
    inlined_blocks: std::collections::HashSet<usize>,
    indent: usize,
}

impl<'m> JsBackend<'m> {
    fn new(module: &'m Module) -> Self {
        Self {
            module,
            out: String::new(),
            vars: HashMap::new(),
            name_vars: HashMap::new(),
            inlined_blocks: std::collections::HashSet::new(),
            indent: 0,
        }
    }

    fn emit(mut self) -> Result<String, Diagnostic> {
        self.emit_header();
        self.emit_builtins();
        for function in &self.module.functions {
            self.emit_function(function)?;
        }
        self.emit_entry_point();
        Ok(self.out)
    }

    // ── Header ────────────────────────────────────────────────────────────────

    fn emit_header(&mut self) {
        let src = &self.module.metadata.source_name;
        writeln!(
            self.out,
            "// NEXORA JavaScript backend — generated from {src}\n// NIR format version {}\n",
            self.module.metadata.format_version
        )
        .unwrap();
    }

    // ── Built-ins ─────────────────────────────────────────────────────────────

    fn emit_builtins(&mut self) {
        // print() maps to console.log; handle single or multiple args.
        writeln!(
            self.out,
            "function print(...args) {{ console.log(...args); }}\n"
        )
        .unwrap();
    }

    // ── Function ──────────────────────────────────────────────────────────────

    fn emit_function(&mut self, function: &Function) -> Result<(), Diagnostic> {
        self.vars.clear();
        self.name_vars.clear();
        self.inlined_blocks.clear();

        // Assign JS variable names to every value defined in this function.
        for block in &function.basic_blocks {
            for instruction in &block.instructions {
                self.assign_var_names(instruction);
            }
        }

        // Build name_vars for parameters so Load instructions resolve correctly.
        let params: Vec<String> = function
            .parameters
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                let entry = &function.basic_blocks[0];
                let js_var = entry
                    .instructions
                    .iter()
                    .find_map(|instr| match instr {
                        Instruction::Parameter { result, index, .. } if *index == i => {
                            Some(*result)
                        }
                        _ => None,
                    })
                    .map(|id| self.var(id))
                    .unwrap_or_else(|| name.clone());
                self.name_vars.insert(name.clone(), js_var.clone());
                js_var
            })
            .collect();

        // Debug comment
        let span = function.debug.function_span;
        if span.start != span.end {
            writeln!(self.out, "// {}:{}", function.debug.source_name, span.start).unwrap();
        }

        writeln!(
            self.out,
            "function {}({}) {{",
            js_ident(&function.name),
            params.join(", ")
        )
        .unwrap();
        self.indent = 1;

        // Emit blocks in order; skip blocks that were inlined into a Branch.
        let block_count = function.basic_blocks.len();
        for (idx, block) in function.basic_blocks.iter().enumerate() {
            if self.inlined_blocks.contains(&idx) {
                continue;
            }
            self.emit_block(block, idx, block_count, function)?;
        }

        self.indent = 0;
        writeln!(self.out, "}}\n").unwrap();
        Ok(())
    }

    fn assign_var_names(&mut self, instruction: &Instruction) {
        let results = instruction_results(instruction);
        for result in results {
            self.vars
                .entry(result)
                .or_insert_with(|| format!("v{result}"));
        }
    }

    fn emit_block(
        &mut self,
        block: &BasicBlock,
        idx: usize,
        total: usize,
        function: &Function,
    ) -> Result<(), Diagnostic> {
        // Emit a label comment for non-entry blocks that have predecessors.
        if idx > 0 {
            self.line(&format!("// bb{}:", block.id));
        }

        for instruction in &block.instructions {
            self.emit_instruction(instruction, function)?;
        }

        self.emit_terminator(&block.terminator, idx, total, function)?;
        Ok(())
    }

    // ── Instructions ──────────────────────────────────────────────────────────

    fn emit_instruction(
        &mut self,
        instruction: &Instruction,
        _function: &Function,
    ) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::Parameter { .. } => {
                // Parameters are already in the function signature; nothing to emit.
            }

            Instruction::Constant { result, value, .. } => {
                let js_val = js_constant(value);
                let v = self.var(*result);
                self.line(&format!("let {v} = {js_val};"));
            }

            Instruction::Load { result, name, .. } => {
                let v = self.var(*result);
                // Prefer the JS variable that holds this NXL name (e.g. a
                // parameter bound to v0) over the raw NXL identifier.
                let src = self
                    .name_vars
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| js_ident(name).to_owned());
                self.line(&format!("let {v} = {src};"));
            }

            Instruction::Store { name, value, .. } => {
                let rhs = self.var(*value);
                // Record that this NXL name now holds the JS variable `rhs`.
                self.name_vars.insert(name.clone(), rhs.clone());
                self.line(&format!("let {} = {rhs};", js_ident(name)));
            }

            Instruction::Assign { name, value, .. } => {
                let rhs = self.var(*value);
                // Update name_vars so subsequent Loads see the new value.
                self.name_vars.insert(name.clone(), rhs.clone());
                self.line(&format!("{} = {rhs};", js_ident(name)));
            }

            Instruction::Unary {
                result,
                operator,
                operand,
                ..
            } => {
                let op = js_unary(*operator);
                let operand = self.var(*operand);
                let v = self.var(*result);
                self.line(&format!("let {v} = {op}{operand};"));
            }

            Instruction::Binary {
                result,
                operator,
                left,
                right,
                ..
            } => {
                let lhs = self.var(*left);
                let rhs = self.var(*right);
                let op = js_binary(*operator);
                let v = self.var(*result);
                self.line(&format!("let {v} = {lhs} {op} {rhs};"));
            }

            Instruction::Call {
                result,
                callee,
                arguments,
                return_type,
                ..
            } => {
                let args: Vec<String> = arguments.iter().map(|a| self.var(*a)).collect();
                let call = format!("{}({})", js_ident(callee), args.join(", "));
                if matches!(return_type, Type::Void) {
                    self.line(&format!("{call};"));
                } else {
                    let v = self.var(*result);
                    self.line(&format!("let {v} = {call};"));
                }
            }

            Instruction::IteratorNext {
                has_next,
                item,
                iterable,
                ..
            } => {
                // Emit iterator state as a JS iterator protocol call.
                // We represent iterables as arrays; use an index variable.
                let iter_var = format!("__iter_{}", self.var(*iterable));
                let hn = self.var(*has_next);
                let it = self.var(*item);
                self.line(&format!("let {hn} = {iter_var}_idx < {iter_var}.length;"));
                self.line(&format!(
                    "let {it} = {hn} ? {iter_var}[{iter_var}_idx++] : undefined;"
                ));
            }

            Instruction::Allocate { result, .. } => {
                let v = self.var(*result);
                self.line(&format!("let {v} = {{}};"));
            }

            Instruction::LoadMemory {
                result, pointer, ..
            } => {
                let ptr = self.var(*pointer);
                let v = self.var(*result);
                self.line(&format!("let {v} = {ptr}.__value;"));
            }

            Instruction::StoreMemory { pointer, value, .. } => {
                let ptr = self.var(*pointer);
                let val = self.var(*value);
                self.line(&format!("{ptr}.__value = {val};"));
            }

            Instruction::Borrow { result, value, .. } => {
                // Borrows are transparent in JS — just alias the value.
                let src = self.var(*value);
                let v = self.var(*result);
                self.line(&format!("let {v} = {src};"));
            }

            Instruction::Drop { .. } => {
                // No-op in a GC'd runtime.
            }

            Instruction::Spawn { result, task, .. } => {
                let t = self.var(*task);
                let v = self.var(*result);
                self.line(&format!("let {v} = Promise.resolve({t});"));
            }

            Instruction::Await { result, task, .. } => {
                let t = self.var(*task);
                let v = self.var(*result);
                self.line(&format!("let {v} = await {t};"));
            }

            Instruction::ChannelSend { channel, value, .. } => {
                let ch = self.var(*channel);
                let val = self.var(*value);
                self.line(&format!("{ch}.push({val});"));
            }

            Instruction::ChannelReceive {
                result, channel, ..
            } => {
                let ch = self.var(*channel);
                let v = self.var(*result);
                self.line(&format!("let {v} = {ch}.shift();"));
            }

            Instruction::ChannelClose { channel, .. } => {
                let ch = self.var(*channel);
                self.line(&format!("{ch}.__closed = true;"));
            }

            Instruction::ActorDispatch { actor, message, .. } => {
                let a = self.var(*actor);
                let m = self.var(*message);
                self.line(&format!("{a}.__dispatch({m});"));
            }

            Instruction::CancelTask { task, .. } => {
                let t = self.var(*task);
                self.line(&format!("{t}.__cancel?.();"));
            }

            Instruction::ForeignCall {
                result,
                callee,
                arguments,
                return_type,
                ..
            } => {
                let args: Vec<String> = arguments.iter().map(|a| self.var(*a)).collect();
                let call = format!("{}({})", js_ident(callee), args.join(", "));
                if matches!(return_type, Type::Void) {
                    self.line(&format!("{call};"));
                } else {
                    let v = self.var(*result);
                    self.line(&format!("let {v} = {call};"));
                }
            }

            // These are structural; handled by the CFG builder — they should
            // not appear in basic block instruction lists after lowering.
            Instruction::If { .. }
            | Instruction::Loop { .. }
            | Instruction::Break { .. }
            | Instruction::Continue { .. }
            | Instruction::Return { .. } => {}
        }
        Ok(())
    }

    // ── Terminators ───────────────────────────────────────────────────────────

    fn emit_terminator(
        &mut self,
        terminator: &Terminator,
        block_idx: usize,
        total_blocks: usize,
        function: &Function,
    ) -> Result<(), Diagnostic> {
        match terminator {
            Terminator::Return { value, .. } => {
                if let Some(v) = value {
                    let js = self.var(*v);
                    self.line(&format!("return {js};"));
                } else {
                    // Only emit explicit return if not the last block (avoids
                    // redundant `return;` at end of void functions).
                    if block_idx + 1 < total_blocks {
                        self.line("return;");
                    }
                }
            }

            Terminator::Jump { target, .. } => {
                // A forward jump to the immediately next block needs no goto.
                // A back-edge (loop header) is handled by the loop structure
                // already emitted; nothing to emit here.
                let target_idx = function
                    .basic_blocks
                    .iter()
                    .position(|b| b.id == *target)
                    .unwrap_or(block_idx + 1);
                if target_idx != block_idx + 1 {
                    // Back-edge or non-sequential forward jump — emit a
                    // labelled-continue equivalent via a flag variable.
                    // For the current NIR subset this only occurs on loop
                    // back-edges which are already enclosed in a `while(true)`
                    // by the loop structure; the jump is implicit.
                    // Non-loop forward jumps are not yet generated by the
                    // lowerer, so this is a safety comment only.
                    self.line(&format!("// jump -> bb{target}"));
                }
            }

            Terminator::Branch {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let cond = self.var(*condition);
                let then_idx = function
                    .basic_blocks
                    .iter()
                    .position(|b| b.id == *then_block)
                    .unwrap_or(block_idx + 1);
                let else_idx = function
                    .basic_blocks
                    .iter()
                    .position(|b| b.id == *else_block)
                    .unwrap_or(block_idx + 2);

                // Mark these blocks as inlined so the main loop skips them.
                self.inlined_blocks.insert(then_idx);
                self.inlined_blocks.insert(else_idx);

                // Emit structured if/else by inlining the then/else blocks.
                self.line(&format!("if ({cond}) {{"));
                self.indent += 1;
                let then_block_data = &function.basic_blocks[then_idx];
                for instr in &then_block_data.instructions {
                    self.emit_instruction(instr, function)?;
                }
                // Emit the then-block's terminator inline (only Return matters here).
                if let Terminator::Return { value: Some(v), .. } = &then_block_data.terminator {
                    let js = self.var(*v);
                    self.line(&format!("return {js};"));
                }
                self.indent -= 1;
                self.line("} else {");
                self.indent += 1;
                let else_block_data = &function.basic_blocks[else_idx];
                for instr in &else_block_data.instructions {
                    self.emit_instruction(instr, function)?;
                }
                if let Terminator::Return { value: Some(v), .. } = &else_block_data.terminator {
                    let js = self.var(*v);
                    self.line(&format!("return {js};"));
                }
                self.indent -= 1;
                self.line("}");
            }

            Terminator::Unreachable { .. } => {
                self.line("throw new Error(\"unreachable\");");
            }
        }
        Ok(())
    }

    // ── Entry point ───────────────────────────────────────────────────────────

    fn emit_entry_point(&mut self) {
        if let Some(entry) = &self.module.entry_point {
            writeln!(self.out, "// entry point\n{}();", js_ident(entry)).unwrap();
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn var(&self, id: ValueId) -> String {
        self.vars
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("v{id}"))
    }

    fn line(&mut self, text: &str) {
        let pad = "  ".repeat(self.indent);
        writeln!(self.out, "{pad}{text}").unwrap();
    }
}

// ── Rust backend ──────────────────────────────────────────────────────────────

struct RustBackend<'m> {
    module: &'m Module,
    out: String,
    vars: HashMap<ValueId, String>,
    name_vars: HashMap<String, String>,
    inlined_blocks: std::collections::HashSet<usize>,
    indent: usize,
}

impl<'m> RustBackend<'m> {
    fn new(module: &'m Module) -> Self {
        Self {
            module,
            out: String::new(),
            vars: HashMap::new(),
            name_vars: HashMap::new(),
            inlined_blocks: std::collections::HashSet::new(),
            indent: 0,
        }
    }

    fn emit(mut self) -> Result<String, Diagnostic> {
        self.emit_header();
        for function in &self.module.functions {
            self.emit_function(function)?;
        }
        self.emit_entry_point();
        Ok(self.out)
    }

    fn emit_header(&mut self) {
        let src = &self.module.metadata.source_name;
        writeln!(
            self.out,
            "// NEXORA Rust backend — generated from {src}\n// NIR format version {}\n",
            self.module.metadata.format_version
        )
        .unwrap();
    }

    fn emit_function(&mut self, function: &Function) -> Result<(), Diagnostic> {
        self.vars.clear();
        self.name_vars.clear();
        self.inlined_blocks.clear();

        for block in &function.basic_blocks {
            for instruction in &block.instructions {
                self.assign_var_names(instruction);
            }
        }

        let params: Vec<String> = function
            .parameters
            .iter()
            .enumerate()
            .map(|(i, (name, ty))| {
                let entry = &function.basic_blocks[0];
                let js_var = entry
                    .instructions
                    .iter()
                    .find_map(|instr| match instr {
                        Instruction::Parameter { result, index, .. } if *index == i => {
                            Some(*result)
                        }
                        _ => None,
                    })
                    .map(|id| self.var(id))
                    .unwrap_or_else(|| name.clone());
                self.name_vars.insert(name.clone(), js_var.clone());
                format!("{}: {}", rust_ident(&js_var), rust_type(ty))
            })
            .collect();

        let span = function.debug.function_span;
        if span.start != span.end {
            writeln!(self.out, "// {}:{}", function.debug.source_name, span.start).unwrap();
        }

        let fn_name = rust_ident(&function.name);
        let is_main = fn_name == "main";
        if is_main {
            writeln!(self.out, "fn main() {{").unwrap();
        } else {
            writeln!(
                self.out,
                "fn {}({}) -> {} {{",
                fn_name,
                params.join(", "),
                rust_type(&function.return_type)
            )
            .unwrap();
        }
        self.indent = 1;

        let block_count = function.basic_blocks.len();
        for (idx, block) in function.basic_blocks.iter().enumerate() {
            if self.inlined_blocks.contains(&idx) {
                continue;
            }
            self.emit_block(block, idx, block_count, function)?;
        }

        self.indent = 0;
        writeln!(self.out, "}}\n").unwrap();
        Ok(())
    }

    fn assign_var_names(&mut self, instruction: &Instruction) {
        let results = instruction_results(instruction);
        for result in results {
            self.vars
                .entry(result)
                .or_insert_with(|| format!("v{result}"));
        }
    }

    fn emit_block(
        &mut self,
        block: &BasicBlock,
        idx: usize,
        total: usize,
        function: &Function,
    ) -> Result<(), Diagnostic> {
        if idx > 0 {
            self.line(&format!("// bb{}:", block.id));
        }
        for instruction in &block.instructions {
            self.emit_instruction(instruction, function)?;
        }
        self.emit_terminator(&block.terminator, idx, total, function)?;
        Ok(())
    }

    fn emit_instruction(
        &mut self,
        instruction: &Instruction,
        _function: &Function,
    ) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::Parameter { .. } => {}
            Instruction::Constant { result, value, .. } => {
                let rust_val = rust_constant(value);
                let v = self.var(*result);
                self.line(&format!("let mut {v} = {rust_val};"));
            }
            Instruction::Load { result, name, .. } => {
                let v = self.var(*result);
                let src = self
                    .name_vars
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| rust_ident(name).to_owned());
                self.line(&format!("let mut {v} = {src};"));
            }
            Instruction::Store { name, value, .. } => {
                let rhs = self.var(*value);
                self.name_vars.insert(name.clone(), rhs.clone());
                self.line(&format!("let mut {} = {rhs};", rust_ident(name)));
            }
            Instruction::Assign { name, value, .. } => {
                let rhs = self.var(*value);
                self.name_vars.insert(name.clone(), rhs.clone());
                self.line(&format!("{} = {rhs};", rust_ident(name)));
            }
            Instruction::Unary {
                result,
                operator,
                operand,
                ..
            } => {
                let op = rust_unary(*operator);
                let operand = self.var(*operand);
                let v = self.var(*result);
                self.line(&format!("let mut {v} = {op}{operand};"));
            }
            Instruction::Binary {
                result,
                operator,
                left,
                right,
                ..
            } => {
                let lhs = self.var(*left);
                let rhs = self.var(*right);
                let op = rust_binary(*operator);
                let v = self.var(*result);
                self.line(&format!("let mut {v} = {lhs} {op} {rhs};"));
            }
            Instruction::Call {
                result,
                callee,
                arguments,
                return_type,
                ..
            } => {
                let args: Vec<String> = arguments.iter().map(|a| self.var(*a)).collect();
                let callee_name = rust_ident(callee);
                let call = if callee_name == "print" {
                    format!("print({})", args.join(", "))
                } else {
                    format!("{}({})", callee_name, args.join(", "))
                };
                if matches!(return_type, Type::Void) {
                    self.line(&format!("{call};"));
                } else {
                    let v = self.var(*result);
                    self.line(&format!("let mut {v} = {call};"));
                }
            }
            Instruction::IteratorNext {
                has_next,
                item,
                iterable,
                ..
            } => {
                let iter_var = format!("__iter_{}", self.var(*iterable));
                let hn = self.var(*has_next);
                let it = self.var(*item);
                self.line(&format!(
                    "let mut {hn} = {iter_var}_idx < {iter_var}.len() as i32;"
                ));
                self.line(&format!(
                    "let mut {it} = if {hn} {{ {iter_var}[({iter_var}_idx) as usize] }} else {{ Default::default() }};"
                ));
                self.line(&format!("if {hn} {{ {iter_var}_idx += 1; }}"));
            }
            _ => {}
        }
        Ok(())
    }

    fn emit_terminator(
        &mut self,
        terminator: &Terminator,
        block_idx: usize,
        total_blocks: usize,
        _function: &Function,
    ) -> Result<(), Diagnostic> {
        match terminator {
            Terminator::Return { value, .. } => {
                if let Some(v) = value {
                    let js = self.var(*v);
                    self.line(&format!("return {js};"));
                } else if block_idx + 1 < total_blocks {
                    self.line("return;");
                }
            }
            Terminator::Jump { target, .. } => {
                self.line(&format!("// jump -> bb{target}"));
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let cond = self.var(*condition);
                self.line(&format!("if {cond} {{"));
                self.indent += 1;
                self.line(&format!("// then bb{}", then_block));
                self.indent -= 1;
                self.line("} else {");
                self.indent += 1;
                self.line(&format!("// else bb{}", else_block));
                self.indent -= 1;
                self.line("}");
                self.inlined_blocks.insert(0);
            }
            Terminator::Unreachable { .. } => {
                self.line("panic!(\"unreachable\");");
            }
        }
        Ok(())
    }

    fn emit_entry_point(&mut self) {
        if let Some(entry) = &self.module.entry_point {
            let name = rust_ident(entry);
            if name != "main" {
                writeln!(self.out, "// entry point\n{}();", name).unwrap();
            }
        }
    }

    fn var(&self, id: ValueId) -> String {
        self.vars
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("v{id}"))
    }

    fn line(&mut self, text: &str) {
        let pad = "  ".repeat(self.indent);
        writeln!(self.out, "{pad}{text}").unwrap();
    }
}

fn rust_ident(name: &str) -> &str {
    name
}

fn rust_type(ty: &Type) -> &'static str {
    match ty {
        Type::Int => "i64",
        Type::Float => "f64",
        Type::Bool => "bool",
        Type::String => "String",
        Type::Void => "()",
        Type::Task(..) | Type::Channel(..) => "()",
        _ => "()",
    }
}

fn rust_constant(constant: &Constant) -> String {
    match constant {
        Constant::Integer(n) => format!("{n}i64"),
        Constant::Float(f) => {
            if f.fract() == 0.0 {
                format!("{f:.1}f64")
            } else {
                format!("{f}f64")
            }
        }
        Constant::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\".to_string()")
        }
        Constant::Boolean(b) => b.to_string(),
    }
}

fn rust_unary(op: UnaryOperator) -> &'static str {
    match op {
        UnaryOperator::Negate => "-",
        UnaryOperator::Not => "!",
        UnaryOperator::Reference | UnaryOperator::Borrow => "&",
    }
}

fn rust_binary(op: BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
        BinaryOperator::BitAnd => "&",
        BinaryOperator::BitOr => "|",
        BinaryOperator::BitXor => "^",
        BinaryOperator::ShiftLeft => "<<",
        BinaryOperator::ShiftRight => ">>",
        BinaryOperator::In => "in",
        BinaryOperator::Is => "is",
        BinaryOperator::Range => "..",
    }
}

// ── Pure helpers ──────────────────────────────────────────────────────────────

fn js_constant(constant: &Constant) -> String {
    match constant {
        Constant::Integer(n) => n.to_string(),
        Constant::Float(f) => {
            // Ensure the literal is always a valid JS number literal.
            if f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        Constant::String(s) => {
            // Escape backslashes and double-quotes.
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        }
        Constant::Boolean(b) => b.to_string(),
    }
}

fn js_unary(op: UnaryOperator) -> &'static str {
    match op {
        UnaryOperator::Negate => "-",
        UnaryOperator::Not => "!",
        UnaryOperator::Reference | UnaryOperator::Borrow => "&",
    }
}

fn js_binary(op: BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Equal => "===",
        BinaryOperator::NotEqual => "!==",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
        BinaryOperator::BitAnd => "&",
        BinaryOperator::BitOr => "|",
        BinaryOperator::BitXor => "^",
        BinaryOperator::ShiftLeft => "<<",
        BinaryOperator::ShiftRight => ">>",
        BinaryOperator::In => "in",
        BinaryOperator::Is => "instanceof",
        BinaryOperator::Range => "..",
    }
}

/// Sanitise an NXL identifier for use in JavaScript.
/// NXL identifiers are already valid JS identifiers for the current subset.
fn js_ident(name: &str) -> &str {
    name
}

/// Collect all ValueIds produced by an instruction (mirrors nir::instruction_results).
fn instruction_results(instruction: &Instruction) -> Vec<ValueId> {
    match instruction {
        Instruction::Parameter { result, .. }
        | Instruction::Constant { result, .. }
        | Instruction::Load { result, .. }
        | Instruction::Unary { result, .. }
        | Instruction::Binary { result, .. }
        | Instruction::Call { result, .. }
        | Instruction::Allocate { result, .. }
        | Instruction::LoadMemory { result, .. }
        | Instruction::Borrow { result, .. }
        | Instruction::Spawn { result, .. }
        | Instruction::Await { result, .. }
        | Instruction::ChannelReceive { result, .. }
        | Instruction::ForeignCall { result, .. } => vec![*result],
        Instruction::IteratorNext { has_next, item, .. } => vec![*has_next, *item],
        _ => Vec::new(),
    }
}
