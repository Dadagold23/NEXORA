use crate::ast::{BinaryOperator, UnaryOperator};
use crate::diagnostic::Diagnostic;
use crate::nir::{
    BasicBlock, BasicBlockId, Constant, Function, Instruction, Module, Terminator, Type, ValueId,
};
use crate::source::{SourceFile, Span};
use std::collections::HashMap;
use std::collections::HashSet;
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
    compile_js_module(&module)
}

/// Compile an already-lowered module (possibly multi-file) to JavaScript.
pub fn compile_js_module(module: &Module) -> Result<CompiledArtifact, Diagnostic> {
    let js = JsBackend::new(module).emit()?;
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

/// A natural loop recovered from the back-edges of a lowered CFG.
///
/// Lowering turns `while`/`for` into a header block, a body and an exit, so a
/// back-edge (a jump to the header) is enough to rebuild the original loop for
/// native emission: header instructions, the condition test, then the body
/// emitted recursively.
#[derive(Clone, Debug)]
struct LoopInfo {
    /// Block index of the loop header.
    header: usize,
    /// Block index of the single loop exit, when one exists.
    exit: Option<usize>,
    /// Every block index belonging to the loop, nested loops included.
    blocks: HashSet<usize>,
    /// JS label used for `break`/`continue`.
    label: String,
}

/// Index of a block by its id, if the function contains it.
fn block_index(function: &Function, id: BasicBlockId) -> Option<usize> {
    function.basic_blocks.iter().position(|b| b.id == id)
}

/// Blocks control can flow to next.
fn successors(block: &BasicBlock) -> Vec<BasicBlockId> {
    match &block.terminator {
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        Terminator::Return { .. } | Terminator::Unreachable { .. } => Vec::new(),
    }
}

/// Rebuild loops for one function from the metadata lowering recorded.
/// Blocks are the header plus everything reachable from the body entry that
/// does not leave through the exit or revisit the header, which is exactly the
/// set the VM walks at runtime.
fn analyze_loops(function: &Function) -> HashMap<usize, LoopInfo> {
    let mut loops: HashMap<usize, LoopInfo> = HashMap::new();
    for meta in &function.loops {
        let (Some(header), Some(body), Some(exit)) = (
            block_index(function, meta.header),
            block_index(function, meta.body),
            block_index(function, meta.exit),
        ) else {
            continue;
        };

        let mut blocks: HashSet<usize> = HashSet::from([header]);
        let mut worklist = vec![body];
        while let Some(current) = worklist.pop() {
            if current == exit || !blocks.insert(current) {
                continue;
            }
            for successor in successors(&function.basic_blocks[current]) {
                if let Some(next) = block_index(function, successor) {
                    if next != header && next != exit {
                        worklist.push(next);
                    }
                }
            }
        }

        loops.insert(
            header,
            LoopInfo {
                header,
                exit: Some(exit),
                blocks,
                label: format!("__nxl_loop{header}"),
            },
        );
    }
    loops
}

/// Loop currently being emitted.
#[derive(Clone, Debug)]
struct LoopFrame {
    header: usize,
    exit: Option<usize>,
    label: String,
    blocks: HashSet<usize>,
}

struct JsBackend<'m> {
    module: &'m Module,
    out: String,
    /// Maps ValueId → JS variable name within the current function.
    vars: HashMap<ValueId, String>,
    /// Maps NXL name → current JS variable name (updated by Store/Parameter).
    name_vars: HashMap<String, String>,
    /// Block indices that have been inlined into a Branch and must not be
    /// re-emitted by the main block loop.
    inlined_blocks: HashSet<usize>,
    /// JS identifiers already declared in the current function scope.
    /// Lowering rejects shadowing, so a repeated name is always a sequential
    /// rebinding (sibling match arms, repeated catches) and must assign.
    declared_names: HashSet<String>,
    /// When emitting a class method, the JS variable holding `self`.
    method_self: Option<String>,
    /// Loops recovered for the current function, keyed by header block index.
    loops: HashMap<usize, LoopInfo>,
    /// Loop currently being emitted.
    loop_frames: Vec<LoopFrame>,
    /// Join block whose jump is held back while an `if` is emitted, so it is
    /// emitted once after the `if` instead of inside one arm.
    suppressed_jump: Option<BasicBlockId>,
    /// Blocks already emitted as part of a loop body.
    emitted_blocks: HashSet<usize>,
    indent: usize,
}

impl<'m> JsBackend<'m> {
    fn new(module: &'m Module) -> Self {
        Self {
            module,
            out: String::new(),
            vars: HashMap::new(),
            name_vars: HashMap::new(),
            inlined_blocks: HashSet::new(),
            declared_names: HashSet::new(),
            method_self: None,
            loops: HashMap::new(),
            loop_frames: Vec::new(),
            suppressed_jump: None,
            emitted_blocks: HashSet::new(),
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
        // print() maps to console.log; values render like the VM does.
        writeln!(
            self.out,
            "function print(...args) {{ console.log(args.map(__nxl_show).join(\" \")); }}\n"
        )
        .unwrap();
        // Runtime type names for annotated catch dispatch (phase 1 errors).
        writeln!(
            self.out,
            "function __nxl_typename(v) {{ if (Array.isArray(v)) return \"Array\"; if (v && typeof v.__nxl_type === \"string\") return v.__nxl_type; if (typeof v === \"number\") return Number.isInteger(v) ? \"Int\" : \"Float\"; if (typeof v === \"boolean\") return \"Bool\"; if (typeof v === \"string\") return \"String\"; if (v === undefined || v === null) return \"Void\"; return \"Unknown\"; }}\n"
        )
        .unwrap();
        // Value display matching the VM (used by print and string conversion).
        writeln!(
            self.out,
            "function __nxl_show(v) {{ if (Array.isArray(v)) return \"[\" + v.map(__nxl_show).join(\", \") + \"]\"; if (v && typeof v.__nxl_type === \"string\") {{ const tn = v.__nxl_type.split(\"#\").pop().split(\"::\").pop(); if (typeof v.__nxl_variant === \"string\") {{ const args = Array.isArray(v.__nxl_args) ? v.__nxl_args : []; return args.length === 0 ? tn + \".\" + v.__nxl_variant : tn + \".\" + v.__nxl_variant + \"(\" + args.map(__nxl_show).join(\", \") + \")\"; }} const fields = Object.keys(v).filter(k => k !== \"__nxl_type\").map(k => k + \": \" + __nxl_show(v[k])); return tn + \" {{ \" + fields.join(\", \") + \" }}\"; }} if (typeof v === \"string\") return v; if (v === undefined || v === null) return \"()\"; return String(v); }}\n"
        )
        .unwrap();
        writeln!(
            self.out,
            "function __nxl_index(o, i) {{ if (typeof i !== \"number\" || !Number.isInteger(i)) throw new Error(\"array index must be Int\"); const a = (o && Array.isArray(o.__nxl_args)) ? o.__nxl_args : o; if (!Array.isArray(a)) throw new Error(\"cannot index \" + __nxl_show(o)); if (i < 0 || i >= a.length) throw new Error(\"index out of bounds\"); return a[i]; }}\n"
        )
        .unwrap();
        writeln!(
            self.out,
            "function __nxl_index_set(o, i, v) {{ if (typeof i !== \"number\" || !Number.isInteger(i)) throw new Error(\"array index must be Int\"); if (!Array.isArray(o)) throw new Error(\"cannot index \" + __nxl_show(o)); if (i < 0 || i >= o.length) throw new Error(\"index out of bounds\"); const c = o.slice(); c[i] = v; return c; }}\n"
        )
        .unwrap();
        writeln!(
            self.out,
            "function __nxl_field(o, f) {{ if (!o || typeof o !== \"object\" || Array.isArray(o)) throw new Error(\"cannot read field '\" + f + \"'\"); if (!(f in o)) throw new Error(\"unknown field '\" + f + \"'\"); return o[f]; }}\n"
        )
        .unwrap();
        writeln!(
            self.out,
            "function __nxl_field_set(o, f, v) {{ if (!o || typeof o !== \"object\" || Array.isArray(o)) throw new Error(\"cannot set field '\" + f + \"'\"); const c = Object.assign({{}}, o); c[f] = v; return c; }}\n"
        )
        .unwrap();
        // Total pattern reads: undefined instead of throwing on mismatch.
        writeln!(
            self.out,
            "function __nxl_pattern_index(o, i) {{ const a = (o && Array.isArray(o.__nxl_args)) ? o.__nxl_args : o; if (!Array.isArray(a) || typeof i !== \"number\" || !Number.isInteger(i) || i < 0 || i >= a.length) return undefined; return a[i]; }}\n"
        )
        .unwrap();
        writeln!(
            self.out,
            "function __nxl_pattern_field(o, f) {{ if (!o || typeof o !== \"object\" || Array.isArray(o) || !(f in o)) return undefined; return o[f]; }}\n"
        )
        .unwrap();
        // Class method registry and dynamic dispatch (methods return pairs).
        writeln!(self.out, "const __nxl_methods = {{}};\n").unwrap();
        writeln!(
            self.out,
            "function __nxl_call_method(o, m, args) {{ if (o && o.__nxl_stub === true) throw new Error(\"channels need the VM (`nxl run`)\"); const t = (o && typeof o.__nxl_type === \"string\") ? o.__nxl_type : __nxl_typename(o); const f = __nxl_methods[t + \"#\" + m]; if (typeof f !== \"function\") throw new Error(\"unknown method '\" + m + \"' on type '\" + t.split(\"#\").pop().split(\"::\").pop() + \"'\"); return f(o, ...args); }}\n"
        )
        .unwrap();
        // Iteration matches the VM: an Int counts 0..n-1, an array yields its
        // elements, anything else is not iterable. Channels stream on the VM.
        writeln!(
            self.out,
            "function __nxl_iter(v) {{ if (Array.isArray(v)) return v; if (typeof v === \"number\" && Number.isInteger(v)) {{ const n = []; for (let i = 0; i < v; i++) n.push(i); return n; }} throw new Error(__nxl_show(v) + \" is not iterable\"); }}\n"
        )
        .unwrap();
    }

    // ── Function ──────────────────────────────────────────────────────────────

    fn emit_function(&mut self, function: &Function) -> Result<(), Diagnostic> {
        self.vars.clear();
        self.name_vars.clear();
        self.inlined_blocks.clear();
        self.declared_names.clear();
        self.method_self = None;
        self.loops = analyze_loops(function);
        self.loop_frames.clear();
        self.emitted_blocks.clear();

        // Assign JS variable names to every value defined in this function.
        for block in &function.basic_blocks {
            for instruction in &block.instructions {
                self.assign_var_names(instruction);
            }
        }

        // Parameters bind to a JS variable named after the NXL parameter, so
        // rebinding a parameter later is visible to every `Load`.
        let is_method = function.name.contains('#');
        let params: Vec<String> = function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, (name, _))| {
                let js_var = js_ident(name).to_owned();
                self.name_vars.insert(name.clone(), js_var.clone());
                self.declared_names.insert(js_var.clone());
                if index == 0 && is_method {
                    // Tracked by NXL name; Stores rebind the JS variable.
                    self.method_self = Some(name.clone());
                }
                js_var
            })
            .collect();

        // Debug comment
        let span = function.debug.function_span;
        if span.start != span.end {
            writeln!(self.out, "// {}:{}", function.debug.source_name, span.start).unwrap();
        }

        // Methods and module-qualified items need sanitising; plain
        // identifiers pass through unchanged.
        let js_name = if function.name.contains(['#', '.', ':']) {
            js_safe(&function.name)
        } else {
            js_ident(&function.name).to_owned()
        };
        writeln!(self.out, "function {}({}) {{", js_name, params.join(", "))
            .unwrap();
        self.indent = 1;

        // Emit blocks in order; skip blocks that were inlined into a Branch
        // or consumed by a loop emitted at its header.
        let block_count = function.basic_blocks.len();
        for (idx, block) in function.basic_blocks.iter().enumerate() {
            if self.inlined_blocks.contains(&idx) || self.emitted_blocks.contains(&idx) {
                continue;
            }
            if let Some(info) = self.loops.get(&idx).cloned() {
                self.emit_loop(&info, function)?;
                continue;
            }
            self.emit_block(block, idx, block_count, function)?;
        }

        if is_method {
            // Falling off the end still honours the [value, self] protocol.
            if let Some(name) = self.method_self.clone() {
                let s = self
                    .name_vars
                    .get(&name)
                    .cloned()
                    .unwrap_or(name);
                self.line(&format!("return [undefined, {s}];"));
            }
            self.indent = 0;
            writeln!(self.out, "}}\n").unwrap();
            writeln!(
                self.out,
                "__nxl_methods[\"{}\"] = {};",
                function.name, js_name
            )
            .unwrap();
        } else {
            self.indent = 0;
            writeln!(self.out, "}}\n").unwrap();
        }
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

    /// Declare-or-assign an NXL name: `let` on first declaration in this
    /// function, plain assignment for sequential rebindings.
    fn declare(&mut self, name: &str, rhs: &str) -> String {
        let js_name = js_ident(name).to_owned();
        if self.declared_names.insert(js_name.clone()) {
            format!("let {js_name} = {rhs};")
        } else {
            format!("{js_name} = {rhs};")
        }
    }

    /// Emit a return honouring the method protocol (methods return
    /// `[value, self]` so the caller observes receiver mutations).
    /// The self variable resolves through `name_vars` at each return site
    /// because stores rebind it over the method body.
    fn emit_return(&mut self, value: Option<ValueId>) {
        let method_self = self.method_self.clone().and_then(|name| {
            self.name_vars
                .get(&name)
                .cloned()
                .or(Some(name))
        });
        match (method_self, value) {
            (Some(s), Some(v)) => {
                let js = self.var(v);
                self.line(&format!("return [{js}, {s}];"));
            }
            (Some(s), None) => {
                self.line(&format!("return [undefined, {s}];"));
            }
            (None, Some(v)) => {
                let js = self.var(v);
                self.line(&format!("return {js};"));
            }
            (None, None) => {
                self.line("return;");
            }
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
            self.emit_instruction(instruction)?;
        }

        self.emit_terminator(&block.terminator, idx, total, function)?;
        Ok(())
    }

    // ── Loops ─────────────────────────────────────────────────────────────────

    /// Emit one recovered loop as a native JS loop, consuming every block in
    /// its body so the straight-line emitter never sees them again.
    fn emit_loop(&mut self, info: &LoopInfo, function: &Function) -> Result<(), Diagnostic> {
        for idx in &info.blocks {
            self.emitted_blocks.insert(*idx);
        }
        let header = &function.basic_blocks[info.header];
        self.loop_frames.push(LoopFrame {
            header: info.header,
            exit: info.exit,
            label: info.label.clone(),
            blocks: info.blocks.clone(),
        });

        let result = self.emit_loop_body(info, header, function);
        self.loop_frames.pop();
        result
    }

    fn emit_loop_body(
        &mut self,
        info: &LoopInfo,
        header: &BasicBlock,
        function: &Function,
    ) -> Result<(), Diagnostic> {
        // `for` loops iterate a sequence, so they become a native for-of with
        // the item bound before the rest of the header instructions run.
        let iterator = header.instructions.iter().find_map(|instr| {
            if let Instruction::IteratorNext { item, iterable, .. } = instr {
                Some((*item, *iterable))
            } else {
                None
            }
        });

        if let Some((_, iterable)) = iterator {
            let sequence = self.var(iterable);
            self.line(&format!(
                "{}: for (const __nxl_item of __nxl_iter({sequence})) {{",
                info.label
            ));
        } else {
            self.line(&format!("{}: while (true) {{", info.label));
        }
        self.indent += 1;

        for instruction in &header.instructions {
            if matches!(instruction, Instruction::IteratorNext { .. }) {
                if let Some((item, _)) = iterator {
                    self.line(&format!("let {} = __nxl_item;", self.var(item)));
                }
                continue;
            }
            self.emit_instruction(instruction)?;
        }

        match &header.terminator {
            Terminator::Branch {
                condition,
                then_block,
                ..
            } => {
                if iterator.is_none() {
                    let cond = self.var(*condition);
                    self.line(&format!("if (!({cond})) break {};", info.label));
                }
                let body = self.require_block(*then_block, function)?;
                self.emit_inline_region(body, function)?;
            }
            Terminator::Jump { target, .. } => {
                let body = self.require_block(*target, function)?;
                self.emit_inline_region(body, function)?;
            }
            Terminator::Return { value, .. } => self.emit_return(*value),
            Terminator::Unreachable { .. } => self.line("throw new Error(\"unreachable\");"),
        }

        self.indent -= 1;
        self.line("}");
        Ok(())
    }

    fn require_block(&self, id: BasicBlockId, function: &Function) -> Result<usize, Diagnostic> {
        block_index(function, id).ok_or_else(|| {
            Diagnostic::error(
                "NXL2007",
                format!("the JavaScript backend cannot resolve control-flow block bb{id}"),
                Span::default(),
            )
        })
    }

    // ── Instructions ──────────────────────────────────────────────────────────

    fn emit_instruction(&mut self, instruction: &Instruction) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::Parameter { result, .. } => {
                // The parameter already arrives in the signature; bind its
                // value slot so any instruction that reads it works.
                let slot = self.var(*result);
                self.line(&format!("let {slot} = undefined;"));
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
                // The NXL name owns a JS variable of its own, so every later
                // `Load` of it reads the current value — including after a
                // loop iteration rebinds it.
                self.name_vars
                    .insert(name.clone(), js_ident(name).to_owned());
                let line = self.declare(name, &rhs);
                self.line(&line);
            }

            Instruction::Assign { name, value, .. } => {
                let rhs = self.var(*value);
                let target = js_ident(name);
                self.name_vars.insert(name.clone(), target.to_owned());
                self.line(&format!("{target} = {rhs};"));
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
                let call = format!("{}({})", js_safe(callee), args.join(", "));
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
                index,
                ..
            } => {
                // Step the iterator protocol directly. This path is used by
                // loops that survive inside `try` regions, where the header
                // stays structured; the persisted index matches the VM, which
                // resumes where a re-entered loop left off.
                let sequence = self.var(*iterable);
                let cursor = self.var(*index);
                let has = self.var(*has_next);
                let value = self.var(*item);
                self.line(&format!(
                    "let __nxl_seq = __nxl_iter({sequence});"
                ));
                self.line(&format!("let {has} = {cursor} < __nxl_seq.length;"));
                self.line(&format!(
                    "let {value} = {has} ? __nxl_seq[{cursor}] : undefined;"
                ));
                self.line(&format!("{cursor} = {cursor} + 1;"));
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
                let call = format!("{}({})", js_safe(callee), args.join(", "));
                if matches!(return_type, Type::Void) {
                    self.line(&format!("{call};"));
                } else {
                    let v = self.var(*result);
                    self.line(&format!("let {v} = {call};"));
                }
            }

            Instruction::ToString { result, value, .. } => {
                let src = self.var(*value);
                let v = self.var(*result);
                self.line(&format!("let {v} = String({src});"));
            }

            Instruction::TypeName { result, value, .. } => {
                let src = self.var(*value);
                let v = self.var(*result);
                self.line(&format!("let {v} = __nxl_typename({src});"));
            }

            Instruction::Throw { value, .. } => {
                let src = self.var(*value);
                self.line(&format!("throw {src};"));
            }

            Instruction::ArrayNew { result, elements, .. } => {
                let parts: Vec<String> = elements.iter().map(|e| self.var(*e)).collect();
                let v = self.var(*result);
                self.line(&format!("let {v} = [{}];", parts.join(", ")));
            }

            Instruction::IndexGet {
                result, object, index, ..
            } => {
                let obj = self.var(*object);
                let idx = self.var(*index);
                let v = self.var(*result);
                self.line(&format!("let {v} = __nxl_index({obj}, {idx});"));
            }

            Instruction::IndexSet {
                result,
                object,
                index,
                value,
                ..
            } => {
                let obj = self.var(*object);
                let idx = self.var(*index);
                let val = self.var(*value);
                let v = self.var(*result);
                self.line(&format!(
                    "let {v} = __nxl_index_set({obj}, {idx}, {val});"
                ));
            }

            Instruction::StructNew {
                result,
                type_name,
                fields,
                ..
            } => {
                let mut parts = vec![format!("__nxl_type: \"{type_name}\"")];
                for (name, value) in fields {
                    parts.push(format!("\"{name}\": {}", self.var(*value)));
                }
                let v = self.var(*result);
                self.line(&format!("let {v} = {{{}}};", parts.join(", ")));
            }

            Instruction::FieldGet {
                result, object, field, ..
            } => {
                let obj = self.var(*object);
                let v = self.var(*result);
                self.line(&format!("let {v} = __nxl_field({obj}, \"{field}\");"));
            }

            Instruction::FieldSet {
                result,
                object,
                field,
                value,
                ..
            } => {
                let obj = self.var(*object);
                let val = self.var(*value);
                let v = self.var(*result);
                self.line(&format!(
                    "let {v} = __nxl_field_set({obj}, \"{field}\", {val});"
                ));
            }

            Instruction::EnumNew {
                result,
                type_name,
                variant,
                arguments,
                ..
            } => {
                let args: Vec<String> = arguments.iter().map(|a| self.var(*a)).collect();
                let v = self.var(*result);
                self.line(&format!(
                    "let {v} = {{__nxl_type: \"{type_name}\", __nxl_variant: \"{variant}\", __nxl_args: [{}]}};",
                    args.join(", ")
                ));
            }

            Instruction::VariantName { result, value, .. } => {
                let src = self.var(*value);
                let v = self.var(*result);
                self.line(&format!(
                    "let {v} = ({src} && {src}.__nxl_variant) || \"\";"
                ));
            }

            Instruction::PatternIndex {
                result, object, index, ..
            } => {
                let obj = self.var(*object);
                let idx = self.var(*index);
                let v = self.var(*result);
                self.line(&format!(
                    "let {v} = __nxl_pattern_index({obj}, {idx});"
                ));
            }

            Instruction::PatternField {
                result, object, field, ..
            } => {
                let obj = self.var(*object);
                let v = self.var(*result);
                self.line(&format!(
                    "let {v} = __nxl_pattern_field({obj}, \"{field}\");"
                ));
            }

            Instruction::MethodCall {
                result,
                receiver_out,
                receiver,
                method,
                arguments,
                ..
            } => {
                let recv = self.var(*receiver);
                let args: Vec<String> =
                    arguments.iter().map(|a| self.var(*a)).collect();
                let v = self.var(*result);
                let s = self.var(*receiver_out);
                self.line(&format!(
                    "let [{v}, {s}] = __nxl_call_method({recv}, \"{method}\", [{}]);",
                    args.join(", ")
                ));
            }

            Instruction::SpawnCall { span, .. }
            | Instruction::SpawnMethod { span, .. }
            | Instruction::AwaitTask { span, .. }
            | Instruction::CompleteTask { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "concurrent tasks need the VM (`nxl run`); the JavaScript backend cannot schedule threads",
                    *span,
                ));
            }

            Instruction::ChannelNew { result, .. } => {
                // A synchronous stub: constructing channels (e.g. inside
                // actors) works, but send/recv/close need the VM.
                let v = self.var(*result);
                self.line(&format!(
                    "let {v} = {{__nxl_type: \"Channel\", __nxl_stub: true}};"
                ));
            }

            Instruction::Try {
                body,
                catches,
                finally,
                ..
            } => {
                self.line("try {");
                self.indent += 1;
                for instr in body {
                    self.emit_nested(instr)?;
                }
                self.indent -= 1;
                self.line("} catch (__exc) {");
                self.indent += 1;
                for (index, catch) in catches.iter().enumerate() {
                    // A bare catch is always last (enforced by lowering).
                    let cond = match &catch.annotation {
                        Some(ann) => format!("__nxl_typename(__exc) === \"{ann}\""),
                        None => "true".to_owned(),
                    };
                    let keyword = if index == 0 { "if" } else { "else if" };
                    self.line(&format!("{keyword} ({cond}) {{"));
                    self.indent += 1;
                    let js_name = js_ident(&catch.name).to_owned();
                    let old_binding = self.name_vars.insert(catch.name.clone(), js_name.clone());
                    self.line(&format!("let {js_name} = __exc;"));
                    for instr in &catch.body {
                        self.emit_nested(instr)?;
                    }
                    match old_binding {
                        Some(previous) => {
                            self.name_vars.insert(catch.name.clone(), previous);
                        }
                        None => {
                            self.name_vars.remove(&catch.name);
                        }
                    }
                    self.indent -= 1;
                    self.line("}");
                }
                // No catch claimed the value: rethrow so `finally` still runs
                // exactly once via the native `finally` block below.
                if catches.iter().all(|catch| catch.annotation.is_some()) {
                    self.line("else {");
                    self.indent += 1;
                    self.line("throw __exc;");
                    self.indent -= 1;
                    self.line("}");
                }
                self.indent -= 1;
                if let Some(fin) = finally {
                    self.line("} finally {");
                    self.indent += 1;
                    for instr in fin {
                        self.emit_nested(instr)?;
                    }
                    self.indent -= 1;
                }
                self.line("}");
            }

            // Handler-protocol instructions are expanded before emission and
            // never survive into finished blocks; ignore defensively.
            Instruction::Catch { .. } | Instruction::PushHandler { .. } | Instruction::PopHandler { .. } => {
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

    /// Emit an instruction that appears inside a nested `try` region (or inside
    /// nested control flow within one). Structured instructions are emitted
    /// recursively as native JS; anything else delegates to `emit_instruction`.
    ///
    /// Loop bodies run in a fresh block scope per iteration so `let`
    /// declarations inside them cannot collide across iterations.
    fn emit_nested(&mut self, instruction: &Instruction) -> Result<(), Diagnostic> {
        match instruction {
            Instruction::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond = self.var(*condition);
                self.line(&format!("if ({cond}) {{"));
                self.indent += 1;
                for instr in then_branch {
                    self.emit_nested(instr)?;
                }
                self.indent -= 1;
                if let Some(els) = else_branch {
                    self.line("} else {");
                    self.indent += 1;
                    for instr in els {
                        self.emit_nested(instr)?;
                    }
                    self.indent -= 1;
                }
                self.line("}");
                Ok(())
            }
            Instruction::Loop {
                condition,
                condition_instructions,
                body,
                ..
            } => {
                self.line("while (true) {");
                self.indent += 1;
                self.line("{");
                self.indent += 1;
                for instr in condition_instructions {
                    self.emit_nested(instr)?;
                }
                if let Some(cond) = condition {
                    let c = self.var(*cond);
                    self.line(&format!("if (!({c})) break;"));
                }
                for instr in body {
                    self.emit_nested(instr)?;
                }
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
                Ok(())
            }
            Instruction::Break { .. } => {
                self.line("break;");
                Ok(())
            }
            Instruction::Continue { .. } => {
                self.line("continue;");
                Ok(())
            }
            Instruction::Return { value, .. } => {
                self.emit_return(*value);
                Ok(())
            }
            _ => self.emit_instruction(instruction),
        }
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
                if value.is_some() {
                    self.emit_return(*value);
                } else if self.method_self.is_some() {
                    // Methods always return the [value, self] pair.
                    self.emit_return(None);
                } else {
                    // Only emit explicit return if not the last block (avoids
                    // redundant `return;` at end of void functions).
                    if block_idx + 1 < total_blocks {
                        self.line("return;");
                    }
                }
            }

            Terminator::Jump { target, span } => {
                // Straight-line code: a jump to the next block is free, a
                // back-edge belongs to a loop emitted at its header, and
                // anything else is refused by `emit_jump`.
                self.emit_jump(*target, *span, block_idx, function)?;
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

                // Emit structured if/else by inlining the then/else regions.
                // Regions are emitted recursively so else-if chains and
                // trailing branches lower correctly; loop back-edges stay
                // out of scope until the backend control-flow phase.
                self.line(&format!("if ({cond}) {{"));
                self.indent += 1;
                self.emit_inline_region(then_idx, function)?;
                self.indent -= 1;
                self.line("} else {");
                self.indent += 1;
                self.emit_inline_region(else_idx, function)?;
                self.indent -= 1;
                self.line("}");
            }

            Terminator::Unreachable { .. } => {
                self.line("throw new Error(\"unreachable\");");
            }
        }
        Ok(())
    }

    /// Emit one inlined region: its instructions plus its terminator.
    /// Branch terminators recurse (else-if chains, match ladders); jumps are
    /// loop back-edges, loop exits, or a fall-through to the next block.
    fn emit_inline_region(
        &mut self,
        block_idx: usize,
        function: &Function,
    ) -> Result<(), Diagnostic> {
        // A nested loop is emitted natively at its own header, then control
        // continues where the loop leaves off.
        if let Some(info) = self.loops.get(&block_idx).cloned() {
            self.emit_loop(&info, function)?;
            if let Some(exit) = info.exit {
                let resumes_inside_enclosing_loop = self
                    .loop_frames
                    .last()
                    .is_some_and(|frame| frame.blocks.contains(&exit));
                if resumes_inside_enclosing_loop && !self.inlined_blocks.contains(&exit) {
                    return self.emit_inline_region(exit, function);
                }
            }
            return Ok(());
        }
        self.inlined_blocks.insert(block_idx);
        let (instructions, terminator) = {
            let block = &function.basic_blocks[block_idx];
            (block.instructions.clone(), block.terminator.clone())
        };
        for instr in &instructions {
            self.emit_instruction(instr)?;
        }
        match &terminator {
            Terminator::Return { value, .. } => {
                // A return always exits the function, even mid-region.
                self.emit_return(*value);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                ..
            } => {
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
                let cond = self.var(*condition);

                // Inside a loop, both arms of an `if` usually jump to the same
                // join block. That block belongs after the `if`, not inside one
                // arm, so hold the jump back and emit it once below.
                let follow = self.loop_frames.last().and_then(|_| {
                    let then_term = &function.basic_blocks[then_idx].terminator;
                    let else_term = &function.basic_blocks[else_idx].terminator;
                    match (then_term, else_term) {
                        (
                            Terminator::Jump {
                                target: then_target,
                                ..
                            },
                            Terminator::Jump {
                                target: else_target,
                                ..
                            },
                        ) if then_target == else_target => Some(*then_target),
                        _ => None,
                    }
                });
                let previous_suppressed = self.suppressed_jump;
                self.suppressed_jump = follow;
                let follow_index = follow.and_then(|target| block_index(function, target));

                self.line(&format!("if ({cond}) {{"));
                self.indent += 1;
                self.emit_inline_region(then_idx, function)?;
                self.indent -= 1;
                self.line("} else {");
                self.indent += 1;
                self.emit_inline_region(else_idx, function)?;
                self.indent -= 1;
                self.line("}");

                self.suppressed_jump = previous_suppressed;
                // Continue at the join block when it is still inside this loop.
                if let Some(follow_index) = follow_index {
                    let resumes_in_loop = self
                        .loop_frames
                        .last()
                        .is_some_and(|frame| frame.blocks.contains(&follow_index));
                    if resumes_in_loop && !self.inlined_blocks.contains(&follow_index) {
                        return self.emit_inline_region(follow_index, function);
                    }
                }
            }
            Terminator::Jump { target, span } => {
                self.emit_jump(*target, *span, block_idx, function)?;
            }
            Terminator::Unreachable { .. } => {}
        }
        Ok(())
    }

    /// Emit a jump: a loop back-edge becomes `continue`, a loop exit becomes
    /// `break`, a jump to the block that follows needs nothing, and a jump to
    /// another block of the same loop body is emitted inline where it stands.
    /// Any other shape is refused rather than silently mis-emitted.
    fn emit_jump(
        &mut self,
        target: BasicBlockId,
        span: Span,
        block_idx: usize,
        function: &Function,
    ) -> Result<(), Diagnostic> {
        let Some(target_idx) = block_index(function, target) else {
            return Err(Diagnostic::error(
                "NXL2007",
                format!("the JavaScript backend cannot resolve control-flow block bb{target}"),
                span,
            ));
        };

        if let Some(frame) = self.loop_frames.last() {
            if target_idx == frame.header {
                self.line(&format!("continue {};", frame.label));
                return Ok(());
            }
            if frame.exit == Some(target_idx) {
                self.line(&format!("break {};", frame.label));
                return Ok(());
            }
            // The join point of the `if` being emitted is emitted after it.
            if self.suppressed_jump == Some(target) {
                return Ok(());
            }
            if self.inlined_blocks.contains(&target_idx) {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the JavaScript backend cannot emit a loop body that jumps back into an already emitted block",
                    span,
                ));
            }
            // Still inside this loop: emit that block's code right here.
            return self.emit_inline_region(target_idx, function);
        }

        if target_idx != block_idx + 1 {
            // Outside a loop, a jump lands on a block the main emitter still
            // has to reach (an `if` merge point), so control simply falls
            // through. Jumping into a block that was already emitted would
            // skip or repeat code, which we refuse rather than guess.
            if self.inlined_blocks.contains(&target_idx)
                || self.emitted_blocks.contains(&target_idx)
            {
                return Err(Diagnostic::error(
                    "NXL2007",
                    format!(
                        "the JavaScript backend cannot emit a jump from bb{} back into bb{target}; run it with `nxl run` (VM)",
                        function.basic_blocks[block_idx].id
                    ),
                    span,
                ));
            }
            return Ok(());
        }
        Ok(())
    }

    // ── Entry point ───────────────────────────────────────────────────────────

    fn emit_entry_point(&mut self) {
        if let Some(entry) = &self.module.entry_point {
            writeln!(self.out, "// entry point\n{}();", js_safe(entry)).unwrap();
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

/// Whether a structured instruction list returns out of its enclosing region.
fn contains_return(instructions: &[Instruction]) -> bool {
    instructions.iter().any(|instruction| match instruction {
        Instruction::Return { .. } => true,
        Instruction::If {
            then_branch,
            else_branch,
            ..
        } => {
            contains_return(then_branch)
                || else_branch
                    .as_ref()
                    .is_some_and(|branch| contains_return(branch))
        }
        Instruction::Loop { body, .. } => contains_return(body),
        Instruction::Try {
            body,
            catches,
            finally,
            ..
        } => {
            contains_return(body)
                || catches
                    .iter()
                    .any(|catch| contains_return(&catch.body))
                || finally.as_ref().is_some_and(|block| contains_return(block))
        }
        _ => false,
    })
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
        // Runtime shim: NXL exceptions travel as panics so `try` can catch
        // them without threading a `Result` through every function. The hook
        // keeps the default handler from printing caught exceptions.
        writeln!(
            self.out,
            "#[derive(Debug, Clone)]\npub struct NxlError(pub String);\n\nfn __nxl_throw(message: String) -> ! {{\n    std::panic::panic_any(NxlError(message));\n}}\n\nfn __nxl_message(payload: Box<dyn std::any::Any + Send>) -> String {{\n    match payload.downcast::<NxlError>() {{\n        Ok(error) => error.0.clone(),\n        Err(other) => if let Some(message) = other.downcast_ref::<String>() {{\n            message.clone()\n        }} else if let Some(message) = other.downcast_ref::<&str>() {{\n            message.to_string()\n        }} else {{\n            \"non-error panic\".to_owned()\n        }},\n    }}\n}}\n\nfn __nxl_install_hook() {{\n    let previous = std::panic::take_hook();\n    std::panic::set_hook(Box::new(move |info| {{\n        if info.payload().downcast_ref::<NxlError>().is_some() {{ return; }}\n        previous(info);\n    }}));\n}}\n\nfn print(args: &[String]) {{\n    println!(\"{{}}\", args.join(\" \"));\n}}\n"
        )
        .unwrap();
    }

    fn emit_function(&mut self, function: &Function) -> Result<(), Diagnostic> {
        self.vars.clear();
        self.name_vars.clear();
        self.inlined_blocks.clear();

        // Class methods have no Rust equivalent yet; skip them so the rest
        // of the module still emits (calls already fail with NXL2007).
        if function.name.contains('#') {
            return Ok(());
        }

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

        let fn_name = rust_safe(&function.name);
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
        if is_main {
            // Caught NXL exceptions must not print a panic banner.
            self.line("__nxl_install_hook();");
        }

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

    /// `function` is only forwarded when recursing into nested regions (a `try`
/// body or handler).
#[allow(clippy::only_used_in_recursion)]
fn emit_instruction(
        &mut self,
        instruction: &Instruction,
        function: &Function,
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
                let callee_name = rust_safe(callee);
                let call = if callee_name == "print" {
                    // Matches the VM: every argument rendered, space-separated.
                    let rendered: Vec<String> = args
                        .iter()
                        .map(|arg| format!("format!(\"{{}}\", {arg})"))
                        .collect();
                    format!("print(&[{}] as &[String]);", rendered.join(", "))
                } else {
                    format!("{}({})", callee_name, args.join(", "))
                };
                if matches!(return_type, Type::Void) || callee_name == "print" {
                    self.line(&call);
                } else {
                    let v = self.var(*result);
                    self.line(&format!("let mut {v} = {};", call.trim_end_matches(';')));
                }
            }
            Instruction::IteratorNext { span, .. } => {
                // `for` loops need the loop scaffolding emitted around this
                // step; emitting it alone produced code that never compiled.
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support for-in loops yet; run it with `nxl run` (VM) or the JavaScript backend",
                    *span,
                ));
            }
            Instruction::ToString { result, value, .. } => {
                let src = self.var(*value);
                let v = self.var(*result);
                self.line(&format!("let mut {v} = format!(\"{{}}\", {src});"));
            }
            Instruction::ArrayNew { result, elements, .. } => {
                let parts: Vec<String> = elements.iter().map(|e| self.var(*e)).collect();
                let v = self.var(*result);
                self.line(&format!("let mut {v} = vec![{}];", parts.join(", ")));
            }
            Instruction::IndexGet {
                result, object, index, ..
            } => {
                let obj = self.var(*object);
                let idx = self.var(*index);
                let v = self.var(*result);
                self.line(&format!("let mut {v} = {obj}[{idx} as usize].clone();"));
            }
            Instruction::IndexSet {
                result,
                object,
                index,
                value,
                ..
            } => {
                let obj = self.var(*object);
                let idx = self.var(*index);
                let val = self.var(*value);
                let v = self.var(*result);
                self.line(&format!("{obj}[{idx} as usize] = {val}.clone();"));
                self.line(&format!("let mut {v} = {val};"));
            }
            Instruction::TypeName { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support runtime type names yet",
                    *span,
                ));
            }
            Instruction::VariantName { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support enum variants yet",
                    *span,
                ));
            }
            Instruction::StructNew { span, .. } | Instruction::FieldGet { span, .. } | Instruction::FieldSet { span, .. } | Instruction::EnumNew { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support composite data yet",
                    *span,
                ));
            }
            Instruction::PatternIndex { span, .. } | Instruction::PatternField { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support match patterns yet",
                    *span,
                ));
            }
            Instruction::MethodCall { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support method calls yet",
                    *span,
                ));
            }
            Instruction::SpawnCall { span, .. }
            | Instruction::SpawnMethod { span, .. }
            | Instruction::AwaitTask { span, .. }
            | Instruction::CompleteTask { span, .. }
            | Instruction::ChannelNew { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support concurrency yet",
                    *span,
                ));
            }
            Instruction::Throw { value, .. } => {
                let src = self.var(*value);
                // NXL exceptions become panics carrying the rendered message;
                // `try` catches them back into a handler.
                self.line(&format!("__nxl_throw(format!(\"{{}}\", {src}));"));
            }
            Instruction::Try {
                body,
                catches,
                finally,
                span,
                ..
            } => {
                if catches
                    .iter()
                    .any(|catch| catch.annotation.is_some())
                {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "the Rust backend does not support `catch e: Type` yet; catch without a type instead",
                        *span,
                    ));
                }
                // `return` inside a try body would be swallowed by the
                // catching closure, exactly as the VM refuses it.
                if contains_return(body) {
                    return Err(Diagnostic::error(
                        "NXL2007",
                        "return across a try boundary is not supported yet",
                        *span,
                    ));
                }

                self.line("let __nxl_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {");
                self.indent += 1;
                for instruction in body {
                    self.emit_instruction(instruction, function)?;
                }
                self.indent -= 1;
                self.line("}));");
                self.line("match __nxl_outcome {");
                self.indent += 1;
                self.line("Ok(()) => {");
                self.indent += 1;
                if let Some(block) = finally {
                    for instruction in block {
                        self.emit_instruction(instruction, function)?;
                    }
                }
                self.indent -= 1;
                self.line("}");
                self.line("Err(__nxl_panic) => {");
                self.indent += 1;
                if let Some(catch) = catches.first() {
                    self.line("let __nxl_error = __nxl_message(__nxl_panic);");
                    let binding = rust_ident(&catch.name).to_owned();
                    self.line(&format!("let {binding} = __nxl_error;"));
                    for instruction in &catch.body {
                        self.emit_instruction(instruction, function)?;
                    }
                    self.name_vars.insert(catch.name.clone(), binding);
                }
                if let Some(block) = finally {
                    for instruction in block {
                        self.emit_instruction(instruction, function)?;
                    }
                }
                if catches.is_empty() {
                    self.line("std::panic::resume_unwind(__nxl_panic);");
                }
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            Instruction::Return { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "return across a try boundary is not supported yet",
                    *span,
                ));
            }
            Instruction::If { span, .. }
            | Instruction::Loop { span, .. }
            | Instruction::Break { span, .. }
            | Instruction::Continue { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support loops and branching yet; run it with `nxl run` (VM) or the JavaScript backend",
                    *span,
                ));
            }
            // Handler-protocol instructions are expanded before emission and
            // never survive into finished blocks; ignore defensively.
            Instruction::Catch { .. }
            | Instruction::PushHandler { .. }
            | Instruction::PopHandler { .. } => {}
            _ => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support this instruction yet; run it with `nxl run` (VM) or the JavaScript backend",
                    instruction.span(),
                ));
            }
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
            Terminator::Jump { target, span } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    format!(
                        "the Rust backend cannot emit the jump to bb{target}; run it with `nxl run` (VM) or the JavaScript backend"
                    ),
                    *span,
                ));
            }
            Terminator::Branch { span, .. } => {
                return Err(Diagnostic::error(
                    "NXL2007",
                    "the Rust backend does not support branching yet; run it with `nxl run` (VM) or the JavaScript backend",
                    *span,
                ));
            }
            Terminator::Unreachable { .. } => {
                self.line("panic!(\"unreachable\");");
            }
        }
        Ok(())
    }

fn emit_entry_point(&mut self) {
        if let Some(entry) = &self.module.entry_point {
            let name = rust_safe(entry);
            if name != "main" {
                // The NXL entry function is emitted as a plain Rust function,
                // so the process entry installs the hook and calls it.
                writeln!(
                    self.out,
                    "// entry point\nfn main() {{ __nxl_install_hook(); {name}(); }}"
                )
                .unwrap();
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

/// Module-qualified names (`app::math#add`) are not valid Rust identifiers.
fn rust_safe(name: &str) -> String {
    name.replace(['#', '.', ':'], "_")
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

/// Module-qualified and method names (`app::math#add`) are not valid JS
/// identifiers; `$` keeps every mapping injective and readable.
fn js_safe(name: &str) -> String {
    name.replace(['#', '.', ':'], "$")
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
        Instruction::PatternIndex { result, .. }
        | Instruction::PatternField { result, .. } => vec![*result],
        Instruction::MethodCall {
            result, receiver_out, ..
        } => vec![*result, *receiver_out],
        Instruction::ArrayNew { result, .. }
        | Instruction::IndexGet { result, .. }
        | Instruction::IndexSet { result, .. }
        | Instruction::StructNew { result, .. }
        | Instruction::FieldGet { result, .. }
        | Instruction::FieldSet { result, .. }
        | Instruction::EnumNew { result, .. }
        | Instruction::VariantName { result, .. }
        | Instruction::ToString { result, .. }
        | Instruction::TypeName { result, .. }
        | Instruction::Catch { result, .. } => vec![*result],
        _ => Vec::new(),
    }
}

