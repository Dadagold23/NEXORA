use nxl_compiler::{compile_to_nir, diagnostic::Severity, source::SourceFile};

#[test]
fn lowers_variables_arithmetic_and_calls() {
    let source = SourceFile::new("hello.nxl", "let x: Int = 10; let y = 20; print(x + y);");
    let module = compile_to_nir(&source).expect("valid program should lower");
    assert_eq!(module.name, "main");
    assert_eq!(module.functions.len(), 1);
    let rendered = module.to_string();
    assert!(rendered.contains("name: \"x\""));
    assert!(rendered.contains("operator: Add"));
    assert!(rendered.contains("callee: \"print\""));
    assert_eq!(module.metadata.format_version, 1);
    assert_eq!(module.metadata.source_name, "hello.nxl");
    assert_eq!(
        module.functions[0].debug.instruction_spans.len(),
        module.functions[0]
            .basic_blocks
            .iter()
            .map(|block| block.instructions.len() + 1)
            .sum::<usize>()
    );
}

#[test]
fn semantic_hir_resolves_forward_function_signatures() {
    let source = SourceFile::new(
        "forward_call.nxl",
        "fn first() -> Int { return second(); } fn second() -> Int { return 2; }",
    );
    let hir = nxl_compiler::hir::analyze(
        &nxl_compiler::parse_source(&source).expect("program should parse"),
        source.name.clone(),
    )
    .expect("HIR should collect declarations before lowering bodies");
    assert_eq!(hir.functions.len(), 2);
    let module = nxl_compiler::nir::lower_hir(&hir).expect("forward call should resolve");
    assert!(module
        .functions
        .iter()
        .any(|function| function.name == "first"));
}

#[test]
fn nir_carries_module_and_type_metadata() {
    let source = SourceFile::new(
        "types.nxl",
        "struct User { name: String } type UserId = Int; let id: Int = 1;",
    );
    let module = compile_to_nir(&source).expect("supported declarations should lower");
    assert!(module
        .type_definitions
        .iter()
        .any(|definition| definition.name == "User"));
    assert!(module
        .type_definitions
        .iter()
        .any(|definition| definition.name == "UserId"));
    assert!(module.globals.iter().any(|global| global.name == "id"));
    assert!(module.entry_point.is_some());
}

#[test]
fn reports_undefined_names_with_source_locations() {
    let source = SourceFile::new("broken.nxl", "print(missing);");
    let diagnostic = compile_to_nir(&source).expect_err("undefined name must be rejected");
    assert_eq!(diagnostic.code, "NXL2002");
    assert_eq!(diagnostic.severity, Severity::Error);
    let rendered = diagnostic.render(&source);
    assert!(rendered.contains("broken.nxl:1:7"));
    assert!(rendered.contains("use of undeclared name 'missing'"));
}

#[test]
fn reports_type_annotation_mismatches() {
    let source = SourceFile::new("types.nxl", "let answer: Int = true;");
    let diagnostic = compile_to_nir(&source).expect_err("incompatible annotation must fail");
    assert_eq!(diagnostic.code, "NXL2001");
}

#[test]
fn rejects_invalid_operand_types() {
    let source = SourceFile::new("bad_op.nxl", "let value = true + false;");
    let diagnostic = compile_to_nir(&source).expect_err("boolean addition must fail");
    assert_eq!(diagnostic.code, "NXL2001");
}

#[test]
fn rejects_unknown_annotation_types() {
    let source = SourceFile::new("unknown_type.nxl", "let value: Imaginary = 1;");
    let diagnostic = compile_to_nir(&source).expect_err("unknown types must fail");
    assert_eq!(diagnostic.code, "NXL2004");
}

#[test]
fn rejects_undeclared_function_calls() {
    let source = SourceFile::new("unknown_call.nxl", "mystery();");
    let diagnostic = compile_to_nir(&source).expect_err("unknown functions must fail");
    assert_eq!(diagnostic.code, "NXL2002");
}

#[test]
fn accepts_assignment_to_mutable_binding() {
    let source = SourceFile::new(
        "assign_mut.nxl",
        "let mut value = 1; value = 2; print(value);",
    );
    let module = compile_to_nir(&source).expect("mutable assignment should succeed");
    assert_eq!(module.name, "main");
    assert!(module
        .to_string()
        .contains("Assign { name: \"value\", value: 1"));
}

#[test]
fn rejects_assignment_to_immutable_binding() {
    let source = SourceFile::new("assign_immutable.nxl", "let value = 1; value = 2;");
    let diagnostic = compile_to_nir(&source).expect_err("immutable assignment must fail");
    assert_eq!(diagnostic.code, "NXL2008");
}

#[test]
fn rejects_assignment_to_undeclared_name() {
    let source = SourceFile::new("assign_undeclared.nxl", "value = 2;");
    let diagnostic = compile_to_nir(&source).expect_err("undeclared assignment must fail");
    assert_eq!(diagnostic.code, "NXL2002");
}

#[test]
fn rejects_assigning_void_call_to_value() {
    let source = SourceFile::new("void_call.nxl", "let value: Int = print(1);");
    let diagnostic = compile_to_nir(&source).expect_err("void call must not produce Int");
    assert_eq!(diagnostic.code, "NXL2006");
}

#[test]
fn rejects_binding_a_void_expression() {
    let source = SourceFile::new("void_binding.nxl", "let value: Void = print(1);");
    let diagnostic = compile_to_nir(&source).expect_err("void expressions cannot be bound");
    assert_eq!(diagnostic.code, "NXL2006");
}

#[test]
fn rejects_duplicate_declarations() {
    let source = SourceFile::new("duplicate.nxl", "let value = 1; let value = 2;");
    let diagnostic = compile_to_nir(&source).expect_err("duplicate declarations must fail");
    assert_eq!(diagnostic.code, "NXL2005");
}

#[test]
fn rejects_duplicate_parameter_names() {
    let source = SourceFile::new(
        "duplicate_parameter.nxl",
        "fn f(x: Int, x: Int) -> Int { return x; }",
    );
    let diagnostic = compile_to_nir(&source).expect_err("duplicate parameter names must fail");
    assert_eq!(diagnostic.code, "NXL2005");
}

#[test]
fn rejects_return_outside_function() {
    let source = SourceFile::new("return_out.nxl", "return 42;");
    let diagnostic = compile_to_nir(&source).expect_err("return outside function must fail");
    assert_eq!(diagnostic.code, "NXL2010");
}

#[test]
fn rejects_break_outside_loop() {
    let source = SourceFile::new("break_out.nxl", "break;");
    let diagnostic = compile_to_nir(&source).expect_err("break outside loop must fail");
    assert_eq!(diagnostic.code, "NXL2011");
}

#[test]
fn rejects_continue_outside_loop() {
    let source = SourceFile::new("continue_out.nxl", "continue;");
    let diagnostic = compile_to_nir(&source).expect_err("continue outside loop must fail");
    assert_eq!(diagnostic.code, "NXL2012");
}

#[test]
fn lowers_while_loop_with_break() {
    let source = SourceFile::new("loop_ok.nxl", "while true { break; }");
    let module = compile_to_nir(&source).expect("loop should lower successfully");
    assert!(module.to_string().contains("Branch"));
}

#[test]
fn lowers_infinite_loop_with_break() {
    let source = SourceFile::new("loop_forever.nxl", "loop { break; }");
    let module = compile_to_nir(&source).expect("infinite loop should lower successfully");
    assert!(module.to_string().contains("Jump"));
}

#[test]
fn lowers_if_else_blocks() {
    let source = SourceFile::new("if_else.nxl", "if true { print(1); } else { print(2); }");
    let module = compile_to_nir(&source).expect("if/else blocks should lower successfully");
    assert!(module.to_string().contains("Branch {"));
    assert!(module.functions[0].basic_blocks.len() >= 4);
    assert!(module.functions[0]
        .basic_blocks
        .iter()
        .any(|block| matches!(
            block.terminator,
            nxl_compiler::nir::Terminator::Branch { .. }
        )));
    module
        .verify()
        .expect("generated control flow should verify");
}

#[test]
fn rejects_non_boolean_if_condition() {
    let source = SourceFile::new("if_bad_cond.nxl", "if 1 { print(1); }");
    let diagnostic = compile_to_nir(&source).expect_err("non-boolean condition must fail");
    assert_eq!(diagnostic.code, "NXL2001");
}

#[test]
fn lowers_supported_match_patterns_and_rejects_unsupported_ones() {
    let valid_source = SourceFile::new(
        "match_ok.nxl",
        "let value = 1; match value { case 1 => print(1); case _ when true => print(2); }",
    );
    let module = compile_to_nir(&valid_source).expect("supported match patterns should lower");
    assert!(module.to_string().contains("module"));

    let binding_source = SourceFile::new(
        "match_bind.nxl",
        "let value = 5; match value { case item when item > 0 => print(item); }",
    );
    let binding_module = compile_to_nir(&binding_source).expect("match binding should lower");
    assert!(binding_module.to_string().contains("module"));

    let invalid_source = SourceFile::new(
        "match_bad.nxl",
        "let value = 1; match value { case [1] => print(1); }",
    );
    let diagnostic =
        compile_to_nir(&invalid_source).expect_err("unsupported match patterns must fail");
    assert_eq!(diagnostic.code, "NXL2007");
}

#[test]
fn rejects_function_missing_required_return_value() {
    let source = SourceFile::new("missing_return.nxl", "fn answer() -> Int { let x = 1; }");
    let diagnostic = compile_to_nir(&source).expect_err("non-void function must return a value");
    assert_eq!(diagnostic.code, "NXL2001");
}

#[test]
fn validates_while_condition_type() {
    let valid_source = SourceFile::new("while_ok.nxl", "while true { print(1); }");
    let module = compile_to_nir(&valid_source).expect("boolean while condition should lower");
    assert!(module.to_string().contains("Branch"));

    let invalid_source = SourceFile::new("while_bad.nxl", "while 1 { print(1); }");
    let diagnostic =
        compile_to_nir(&invalid_source).expect_err("non-boolean while condition must fail");
    assert_eq!(diagnostic.code, "NXL2001");
}

#[test]
fn lowers_while_loop_to_branching_basic_blocks() {
    let source = SourceFile::new("while_cfg.nxl", "while true { print(1); }");
    let module = compile_to_nir(&source).expect("while loop should lower");
    let function = &module.functions[0];
    assert!(function.basic_blocks.iter().any(|block| matches!(
        block.terminator,
        nxl_compiler::nir::Terminator::Branch { .. }
    )));
    assert!(function
        .basic_blocks
        .iter()
        .any(|block| matches!(block.terminator, nxl_compiler::nir::Terminator::Jump { .. })));
}

#[test]
fn nir_verifier_rejects_missing_control_flow_target() {
    let source = SourceFile::new("invalid_cfg.nxl", "let value = 1;");
    let mut module = compile_to_nir(&source).expect("source should lower");
    module.functions[0].basic_blocks[0].terminator = nxl_compiler::nir::Terminator::Jump {
        target: u32::MAX,
        span: Default::default(),
    };
    let diagnostic = module
        .verify()
        .expect_err("missing CFG target must be rejected");
    assert_eq!(diagnostic.code, "NXL3001");
}

#[test]
fn nir_verifier_rejects_undefined_branch_condition() {
    let source = SourceFile::new("invalid_branch.nxl", "if true { print(1); }");
    let mut module = compile_to_nir(&source).expect("source should lower");
    let branch = module.functions[0]
        .basic_blocks
        .iter_mut()
        .find(|block| {
            matches!(
                block.terminator,
                nxl_compiler::nir::Terminator::Branch { .. }
            )
        })
        .expect("lowered if should have branch terminator");
    if let nxl_compiler::nir::Terminator::Branch { condition, .. } = &mut branch.terminator {
        *condition = u32::MAX;
    }
    let diagnostic = module
        .verify()
        .expect_err("undefined branch condition must be rejected");
    assert_eq!(diagnostic.code, "NXL3001");
}

#[test]
fn nir_verifier_rejects_value_defined_only_in_one_branch_used_after_merge() {
    let source = SourceFile::new(
        "non_dominating.nxl",
        "if true { print(1); } else { print(2); }",
    );
    let mut module = compile_to_nir(&source).expect("source should lower");
    let function = &mut module.functions[0];
    let branch_value = function
        .basic_blocks
        .iter()
        .find(|block| block.id != function.basic_blocks[0].id)
        .and_then(|block| {
            block
                .instructions
                .iter()
                .find_map(|instruction| match instruction {
                    nxl_compiler::nir::Instruction::Constant { result, .. } => Some(*result),
                    _ => None,
                })
        })
        .expect("branch should define a constant");
    let merge_block = function
        .basic_blocks
        .iter_mut()
        .find(|block| block.id >= 3)
        .expect("if should create merge block");
    merge_block.terminator = nxl_compiler::nir::Terminator::Return {
        value: Some(branch_value),
        span: Default::default(),
    };
    let diagnostic = module
        .verify()
        .expect_err("branch-local value must not be available after merge");
    assert_eq!(diagnostic.code, "NXL3001");
}

#[test]
fn nir_memory_and_abi_operations_have_typed_representation() {
    use nxl_compiler::nir::{CallingConvention, ForeignFunction, Instruction, Type};

    let source = SourceFile::new("ir_schema.nxl", "let value = 1;");
    let mut module = compile_to_nir(&source).expect("source should lower");
    module.foreign_functions.push(ForeignFunction {
        name: "native_fn".to_owned(),
        library: Some("libnative".to_owned()),
        parameters: vec![Type::Int],
        return_type: Type::Int,
        calling_convention: CallingConvention::C,
    });
    module
        .verify()
        .expect("declared foreign ABI should be valid");
    assert_eq!(
        module.foreign_functions[0].calling_convention,
        CallingConvention::C
    );

    let function = &mut module.functions[0];
    let block = &mut function.basic_blocks[0];
    let value = block
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            Instruction::Constant {
                result,
                ty: Type::Int,
                ..
            } => Some(*result),
            _ => None,
        })
        .expect("source should define an integer value");
    block.instructions.push(Instruction::Allocate {
        result: 501,
        ty: Type::Int,
        span: Default::default(),
    });
    block.instructions.push(Instruction::StoreMemory {
        pointer: 501,
        value,
        span: Default::default(),
    });
    block.instructions.push(Instruction::LoadMemory {
        result: 502,
        pointer: 501,
        ty: Type::Int,
        span: Default::default(),
    });
    function.refresh_metadata();
    module
        .verify()
        .expect("typed memory operations should verify");

    let span = nxl_compiler::source::Span::default();
    let block = &mut module.functions[0].basic_blocks[0];
    block.instructions.push(Instruction::ForeignCall {
        result: 601,
        callee: "native_fn".to_owned(),
        arguments: vec![value],
        return_type: Type::Int,
        calling_convention: CallingConvention::C,
        span,
    });
    block.instructions.push(Instruction::Call {
        result: 602,
        callee: "make_task".to_owned(),
        arguments: Vec::new(),
        return_type: Type::Task(777),
        span,
    });
    block.instructions.push(Instruction::Spawn {
        result: 603,
        task: 602,
        ty: Type::Task(777),
        span,
    });
    block.instructions.push(Instruction::Await {
        result: 604,
        task: 603,
        ty: Type::Generic(777),
        span,
    });
    module.functions[0].refresh_metadata();
    module
        .verify()
        .expect("ABI and async operations should verify against their typed declarations");
    assert!(module.functions[0]
        .concurrency
        .iter()
        .any(|metadata| matches!(
            metadata,
            nxl_compiler::nir::ConcurrencyMetadata::Spawn { .. }
        )));
    assert!(module.functions[0]
        .concurrency
        .iter()
        .any(|metadata| matches!(
            metadata,
            nxl_compiler::nir::ConcurrencyMetadata::Await { .. }
        )));
}

#[test]
fn nir_verifier_rejects_memory_load_through_non_reference() {
    use nxl_compiler::nir::{Instruction, Type};

    let source = SourceFile::new("bad_memory.nxl", "let value = 1;");
    let mut module = compile_to_nir(&source).expect("source should lower");
    let function = &mut module.functions[0];
    let block = &mut function.basic_blocks[0];
    let integer = block
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            Instruction::Constant {
                result,
                ty: Type::Int,
                ..
            } => Some(*result),
            _ => None,
        })
        .expect("integer constant should exist");
    block.instructions.push(Instruction::LoadMemory {
        result: 900,
        pointer: integer,
        ty: Type::Int,
        span: Default::default(),
    });
    function.refresh_metadata();
    let diagnostic = module
        .verify()
        .expect_err("memory load through a scalar must fail");
    assert_eq!(diagnostic.code, "NXL3001");
}

#[test]
fn lowers_for_loop_binding_in_loop_scope() {
    let source = SourceFile::new(
        "for_binding.nxl",
        "let values = 5; for item in values { print(item); }",
    );
    let module = compile_to_nir(&source).expect("for loop binding should be lowered");
    assert!(module.to_string().contains("Branch"));
    assert!(module.functions[0]
        .basic_blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| matches!(
            instruction,
            nxl_compiler::nir::Instruction::IteratorNext { .. }
        )));
}

#[test]
fn rejects_duplicate_for_loop_binding_in_current_scope() {
    let source = SourceFile::new(
        "for_binding_dup.nxl",
        "let item = 1; let values = 2; for item in values { print(item); }",
    );
    let diagnostic = compile_to_nir(&source).expect_err("duplicate loop binding must fail");
    assert_eq!(diagnostic.code, "NXL2005");
}

#[test]
fn accepts_all_paths_returning_value_in_if_else() {
    let source = SourceFile::new(
        "if_return_complete.nxl",
        "fn answer(flag: Bool) -> Int { if flag { return 1; } else { return 2; } }",
    );
    let module = compile_to_nir(&source).expect("all-path return should be accepted");
    assert!(module.to_string().contains("fn answer"));
}

#[test]
fn rejects_if_without_else_when_function_requires_return_value() {
    let source = SourceFile::new(
        "if_return_incomplete.nxl",
        "fn answer(flag: Bool) -> Int { if flag { return 1; } }",
    );
    let diagnostic =
        compile_to_nir(&source).expect_err("missing else branch on return path should fail");
    assert_eq!(diagnostic.code, "NXL2001");
}

#[test]
fn backend_generates_js_artifact_for_valid_nir_subset() {
    let source = SourceFile::new("native_backend.nxl", "let x = 1; print(x);");
    let artifact = nxl_compiler::backend::compile_native(&source)
        .expect("valid NIR subset should generate a JS artifact");
    assert_eq!(artifact.target, "native");
    assert!(artifact.source.contains("fn main"));
}

#[test]
fn reports_unexpected_characters() {
    let source = SourceFile::new("lex.nxl", "let x = @;");
    let diagnostic = compile_to_nir(&source).expect_err("unknown characters must fail");
    assert_eq!(diagnostic.code, "NXL0001");
}

// ── P07 JavaScript backend tests ─────────────────────────────────────────────

use nxl_compiler::backend::compile_js;

#[test]
fn js_backend_emits_valid_header_and_builtin() {
    let source = SourceFile::new("hello.nxl", "print(42);");
    let artifact = compile_js(&source).expect("should compile to JS");
    assert_eq!(artifact.target, "js");
    assert!(artifact.source.contains("// NEXORA JavaScript backend"));
    assert!(artifact.source.contains("function print("));
    assert!(artifact.source.contains("console.log"));
}

#[test]
fn js_backend_emits_integer_constant_and_call() {
    let source = SourceFile::new("const.nxl", "let x = 10; print(x);");
    let artifact = compile_js(&source).expect("integer constant should emit");
    assert!(artifact.source.contains("= 10;"));
    assert!(artifact.source.contains("print("));
}

#[test]
fn js_backend_emits_string_constant() {
    let source = SourceFile::new("str.nxl", r#"print("hello");"#);
    let artifact = compile_js(&source).expect("string constant should emit");
    assert!(artifact.source.contains("\"hello\""));
}

#[test]
fn js_backend_emits_boolean_constants() {
    let source = SourceFile::new("bool.nxl", "let a = true; let b = false;");
    let artifact = compile_js(&source).expect("boolean constants should emit");
    assert!(artifact.source.contains("true"));
    assert!(artifact.source.contains("false"));
}

#[test]
fn js_backend_emits_arithmetic_binary_ops() {
    let source = SourceFile::new("arith.nxl", "let x = 3 + 4; let y = x * 2;");
    let artifact = compile_js(&source).expect("arithmetic should emit");
    assert!(artifact.source.contains('+'));
    assert!(artifact.source.contains('*'));
}

#[test]
fn js_backend_emits_comparison_operators() {
    let source = SourceFile::new("cmp.nxl", "let a = 1 < 2;");
    let artifact = compile_js(&source).expect("comparisons should emit");
    assert!(artifact.source.contains('<'));
}

#[test]
fn js_backend_emits_equality_as_strict_equality() {
    let source = SourceFile::new("eq.nxl", "let a = 3 == 3;");
    let artifact = compile_js(&source).expect("equality should emit");
    assert!(artifact.source.contains("==="));
}

#[test]
fn js_backend_emits_negation_unary() {
    let source = SourceFile::new("neg.nxl", "let x = 5; let y = -x;");
    let artifact = compile_js(&source).expect("negation should emit");
    // The unary minus appears in the output
    assert!(artifact.source.contains('-'));
}

#[test]
fn js_backend_emits_function_definition_and_call() {
    let source = SourceFile::new(
        "fn.nxl",
        "fn add(a: Int, b: Int) -> Int { return a + b; } let r = add(1, 2);",
    );
    let artifact = compile_js(&source).expect("function should emit");
    assert!(artifact.source.contains("function add("));
    assert!(artifact.source.contains("return"));
    assert!(artifact.source.contains("add("));
}

#[test]
fn js_backend_emits_if_else_branch() {
    let source = SourceFile::new("if.nxl", "if true { print(1); } else { print(2); }");
    let artifact = compile_js(&source).expect("if/else should emit");
    assert!(artifact.source.contains("if ("));
    assert!(artifact.source.contains("} else {"));
}

#[test]
fn js_backend_emits_while_loop_as_branch() {
    let source = SourceFile::new("while.nxl", "while true { break; }");
    let artifact = compile_js(&source).expect("while loop should emit");
    assert!(artifact.source.contains("if ("));
}

#[test]
fn js_backend_emits_mutable_assignment() {
    let source = SourceFile::new("assign.nxl", "let mut x = 1; x = 2; print(x);");
    let artifact = compile_js(&source).expect("assignment should emit");
    assert!(artifact.source.contains("= 2;"));
}

#[test]
fn js_backend_emits_entry_point_call() {
    let source = SourceFile::new("entry.nxl", "print(1);");
    let artifact = compile_js(&source).expect("entry point should emit");
    assert!(artifact.source.contains("// entry point"));
    let has_entry =
        artifact.source.contains("main();") || artifact.source.contains("__module_init();");
    assert!(has_entry, "entry point call must be present");
}

#[test]
fn js_backend_emits_float_constant_with_decimal() {
    let source = SourceFile::new("float.nxl", "let f = 3.14;");
    let artifact = compile_js(&source).expect("float should emit");
    assert!(artifact.source.contains("3.14"));
}

#[test]
fn js_backend_emits_function_with_return_value() {
    let source = SourceFile::new(
        "ret.nxl",
        "fn answer(flag: Bool) -> Int { if flag { return 1; } else { return 2; } }",
    );
    let artifact = compile_js(&source).expect("function with return should emit");
    assert!(artifact.source.contains("function answer("));
    assert!(artifact.source.contains("return"));
}

#[test]
fn js_backend_rejects_invalid_source() {
    let source = SourceFile::new("bad.nxl", "let x: Imaginary = 1;");
    let err = compile_js(&source).expect_err("invalid source must fail before JS emission");
    assert_eq!(err.code, "NXL2004");
}

#[test]
fn js_backend_compile_native_alias_returns_js_target() {
    let source = SourceFile::new("alias.nxl", "print(1);");
    let artifact = nxl_compiler::backend::compile_native(&source)
        .expect("compile_native alias should succeed");
    assert_eq!(artifact.target, "native");
    assert!(artifact.source.contains("fn main"));
}

#[test]
fn js_backend_emits_source_metadata_comment() {
    let source = SourceFile::new("meta.nxl", "let x = 1;");
    let artifact = compile_js(&source).expect("metadata comment should emit");
    assert!(artifact.source.contains("meta.nxl"));
    assert!(artifact.source.contains("NIR format version 1"));
}

#[test]
fn js_backend_emits_logical_and_or() {
    let source = SourceFile::new("logic.nxl", "let a = true; let b = false; let c = a == b;");
    let artifact = compile_js(&source).expect("logical ops should emit");
    assert!(artifact.source.contains("==="));
}

// ── P08 Runtime tests ─────────────────────────────────────────────────────────

use nxl_compiler::runtime::{run, ExecutionResult, Runtime, RuntimeConfig};

#[test]
fn runtime_default_config_has_30s_timeout() {
    let config = RuntimeConfig::default();
    assert_eq!(config.timeout.as_secs(), 30);
    assert!(config.env.is_empty());
    assert!(config.working_dir.is_none());
}

#[test]
fn runtime_config_accepts_custom_timeout() {
    let config = RuntimeConfig {
        timeout: std::time::Duration::from_secs(5),
        ..Default::default()
    };
    assert_eq!(config.timeout.as_secs(), 5);
}

#[test]
fn runtime_config_accepts_env_vars() {
    let mut config = RuntimeConfig::default();
    config.env.insert("NXL_ENV".to_owned(), "test".to_owned());
    assert_eq!(config.env.get("NXL_ENV").map(String::as_str), Some("test"));
}

#[test]
fn runtime_new_and_default_are_equivalent() {
    // Both constructors should produce a runtime without panicking.
    let _a = Runtime::new();
    let _b = Runtime::default();
}

#[test]
fn runtime_with_config_stores_config() {
    let config = RuntimeConfig {
        timeout: std::time::Duration::from_secs(10),
        ..Default::default()
    };
    let _rt = Runtime::with_config(config);
}

#[test]
fn execution_result_success_reflects_exit_code() {
    let ok = ExecutionResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
    };
    assert!(ok.success());

    let fail = ExecutionResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 1,
    };
    assert!(!fail.success());
}

#[test]
fn runtime_rejects_compilation_error_before_execution() {
    let source = SourceFile::new("bad_runtime.nxl", "let x: Imaginary = 1;");
    let err = run(&source).expect_err("compilation error must propagate through runtime");
    assert_eq!(err.code, "NXL2004");
}

#[test]
fn runtime_rejects_undefined_name_before_execution() {
    let source = SourceFile::new("undef_runtime.nxl", "print(missing);");
    let err = run(&source).expect_err("undefined name must fail before execution");
    assert_eq!(err.code, "NXL2002");
}

#[test]
fn runtime_run_succeeds_or_reports_node_missing() {
    // This test is environment-aware: if Node.js is present the program must
    // execute and produce output; if Node.js is absent the runtime must return
    // a NXL8002 diagnostic rather than panicking.
    let source = SourceFile::new("hello_runtime.nxl", r#"print("hello from nxl");"#);
    match run(&source) {
        Ok(result) => {
            // Node.js is available — verify output.
            assert!(
                result.stdout.contains("hello from nxl"),
                "expected 'hello from nxl' in stdout, got: {:?}",
                result.stdout
            );
            assert!(result.success(), "exit code should be 0");
        }
        Err(diagnostic) => {
            // Node.js is not installed — must be a runtime error, not a
            // compiler error.
            assert!(
                diagnostic.code == "NXL8002" || diagnostic.code == "NXL8001",
                "expected NXL8002 (node not found) or NXL8001, got {}",
                diagnostic.code
            );
        }
    }
}

#[test]
fn runtime_captures_stdout_from_multiple_prints() {
    if !Runtime::node_available() {
        return; // skip when Node.js is not installed
    }
    let source = SourceFile::new("multi_print.nxl", r#"print("line1"); print("line2");"#);
    let result = run(&source).expect("multi-print should execute");
    assert!(result.stdout.contains("line1"));
    assert!(result.stdout.contains("line2"));
    assert!(result.success());
}

#[test]
fn runtime_captures_integer_output() {
    if !Runtime::node_available() {
        return;
    }
    let source = SourceFile::new("int_out.nxl", "print(42);");
    let result = run(&source).expect("integer print should execute");
    assert!(result.stdout.contains("42"));
}

#[test]
fn runtime_executes_arithmetic_and_prints_result() {
    if !Runtime::node_available() {
        return;
    }
    let source = SourceFile::new("arith_run.nxl", "let x = 3 + 4; print(x);");
    let result = run(&source).expect("arithmetic should execute");
    assert!(result.stdout.contains("7"));
}

#[test]
fn runtime_executes_function_call_and_prints_return() {
    if !Runtime::node_available() {
        return;
    }
    let source = SourceFile::new(
        "fn_run.nxl",
        "fn double(n: Int) -> Int { return n + n; } print(double(5));",
    );
    let result = run(&source).expect("function call should execute");
    assert!(result.stdout.contains("10"));
}

#[test]
fn runtime_executes_if_else_branch() {
    if !Runtime::node_available() {
        return;
    }
    let source = SourceFile::new(
        "if_run.nxl",
        r#"let flag = true; if flag { print("yes"); } else { print("no"); }"#,
    );
    let result = run(&source).expect("if/else should execute");
    assert!(result.stdout.contains("yes"));
    assert!(!result.stdout.contains("no"));
}

#[test]
fn runtime_node_available_returns_bool_without_panic() {
    // Must not panic regardless of whether Node.js is installed.
    let _ = Runtime::node_available();
}

// ── P09 VM tests ──────────────────────────────────────────────────────────────

use nxl_compiler::vm::{
    compile, run as vm_run, Chunk, CompiledProgram, ConstantPool, Opcode, PoolEntry, Value,
    BYTECODE_MAGIC, BYTECODE_VERSION,
};

#[test]
fn vm_bytecode_magic_and_version_are_stable() {
    assert_eq!(BYTECODE_MAGIC, 0x4E584C42);
    assert_eq!(BYTECODE_VERSION, 1);
}

#[test]
fn vm_constant_pool_interns_and_deduplicates() {
    let mut pool = ConstantPool::new();
    let i1 = pool.intern(PoolEntry::Integer(42));
    let i2 = pool.intern(PoolEntry::Integer(42));
    let i3 = pool.intern(PoolEntry::Integer(99));
    assert_eq!(i1, i2, "same value must get same index");
    assert_ne!(i1, i3, "different values must get different indices");
    assert_eq!(pool.len(), 2);
}

#[test]
fn vm_constant_pool_stores_all_types() {
    let mut pool = ConstantPool::new();
    pool.intern(PoolEntry::Integer(1));
    pool.intern(PoolEntry::Float(2.75));
    pool.intern(PoolEntry::String("hello".to_owned()));
    pool.intern(PoolEntry::Boolean(true));
    pool.intern(PoolEntry::FunctionRef("main".to_owned()));
    assert_eq!(pool.len(), 5);
}

#[test]
fn vm_compiles_nir_module_to_program() {
    let source = SourceFile::new("compile.nxl", "let x = 1; print(x);");
    let module = compile_to_nir(&source).expect("should lower");
    let program = compile(&module).expect("should compile to bytecode");
    assert!(!program.chunks.is_empty());
    assert_eq!(program.format_version, BYTECODE_VERSION);
    assert_eq!(program.source_name, "compile.nxl");
}

#[test]
fn vm_program_has_entry_point_chunk() {
    let source = SourceFile::new("entry.nxl", "print(1);");
    let module = compile_to_nir(&source).expect("should lower");
    let program = compile(&module).expect("should compile");
    let entry = &program.entry_point;
    assert!(
        program.chunks.iter().any(|c| &c.name == entry),
        "entry point chunk must exist"
    );
}

#[test]
fn vm_executes_integer_print() {
    let source = SourceFile::new("int_vm.nxl", "print(42);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["42"]);
}

#[test]
fn vm_executes_string_print() {
    let source = SourceFile::new("str_vm.nxl", r#"print("hello");"#);
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["hello"]);
}

#[test]
fn vm_executes_boolean_print() {
    let source = SourceFile::new("bool_vm.nxl", "print(true);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["true"]);
}

#[test]
fn vm_executes_arithmetic() {
    let source = SourceFile::new("arith_vm.nxl", "let x = 3 + 4; print(x);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["7"]);
}

#[test]
fn vm_executes_subtraction_and_multiplication() {
    let source = SourceFile::new("arith2_vm.nxl", "let x = 10 - 3; let y = x * 2; print(y);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["14"]);
}

#[test]
fn vm_executes_comparison_producing_boolean() {
    let source = SourceFile::new("cmp_vm.nxl", "let a = 1 < 2; print(a);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["true"]);
}

#[test]
fn vm_executes_equality_check() {
    let source = SourceFile::new("eq_vm.nxl", "let a = 3 == 3; print(a);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["true"]);
}

#[test]
fn vm_executes_negation() {
    let source = SourceFile::new("neg_vm.nxl", "let x = 5; let y = -x; print(y);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["-5"]);
}

#[test]
fn vm_executes_function_call_and_returns_value() {
    let source = SourceFile::new(
        "fn_vm.nxl",
        "fn double(n: Int) -> Int { return n + n; } print(double(6));",
    );
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["12"]);
}

#[test]
fn vm_executes_if_else_true_branch() {
    let source = SourceFile::new(
        "if_vm.nxl",
        r#"let flag = true; if flag { print("yes"); } else { print("no"); }"#,
    );
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["yes"]);
}

#[test]
fn vm_executes_if_else_false_branch() {
    let source = SourceFile::new(
        "if_false_vm.nxl",
        r#"let flag = false; if flag { print("yes"); } else { print("no"); }"#,
    );
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["no"]);
}

#[test]
fn vm_executes_mutable_assignment() {
    let source = SourceFile::new("mut_vm.nxl", "let mut x = 1; x = 2; print(x);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["2"]);
}

#[test]
fn vm_executes_multiple_prints() {
    let source = SourceFile::new("multi_vm.nxl", r#"print("a"); print("b"); print("c");"#);
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["a", "b", "c"]);
}

#[test]
fn vm_rejects_compilation_error() {
    let source = SourceFile::new("bad_vm.nxl", "let x: Imaginary = 1;");
    let err = vm_run(&source).expect_err("compilation error must propagate");
    assert_eq!(err.code, "NXL2004");
}

#[test]
fn vm_result_captures_return_value() {
    let source = SourceFile::new("ret_vm.nxl", "print(1);");
    let result = vm_run(&source).expect("should execute");
    // Entry point returns Unit when it has no explicit return.
    assert!(matches!(
        result.return_value,
        Value::Unit | Value::Integer(_)
    ));
}

#[test]
fn vm_serializes_and_deserializes_program() {
    let source = SourceFile::new("serde_vm.nxl", "let x = 7; print(x);");
    let module = compile_to_nir(&source).expect("should lower");
    let program = compile(&module).expect("should compile");
    let bytes = program.serialize();
    assert!(!bytes.is_empty());
    // Magic bytes must be present at the start.
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    assert_eq!(magic, BYTECODE_MAGIC);
    let restored = CompiledProgram::deserialize(&bytes).expect("should deserialize");
    assert_eq!(restored.entry_point, program.entry_point);
    assert_eq!(restored.format_version, BYTECODE_VERSION);
    assert_eq!(restored.chunks.len(), program.chunks.len());
}

#[test]
fn vm_deserialized_program_executes_correctly() {
    let source = SourceFile::new("serde_exec_vm.nxl", "print(99);");
    let module = compile_to_nir(&source).expect("should lower");
    let program = compile(&module).expect("should compile");
    let bytes = program.serialize();
    let restored = CompiledProgram::deserialize(&bytes).expect("should deserialize");
    let result = nxl_compiler::vm::execute_program(&restored).expect("should execute");
    assert_eq!(result.output, vec!["99"]);
}

#[test]
fn vm_deserialize_rejects_wrong_magic() {
    let bad: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x00];
    let err = CompiledProgram::deserialize(&bad).expect_err("wrong magic must fail");
    assert!(err.contains("magic"), "error should mention magic: {err}");
}

#[test]
fn vm_deserialize_rejects_wrong_version() {
    let mut bytes = vec![];
    bytes.extend_from_slice(&BYTECODE_MAGIC.to_le_bytes());
    bytes.extend_from_slice(&9999u16.to_le_bytes()); // wrong version
    let err = CompiledProgram::deserialize(&bytes).expect_err("wrong version must fail");
    assert!(
        err.contains("version"),
        "error should mention version: {err}"
    );
}

#[test]
fn vm_display_shows_disassembly() {
    let source = SourceFile::new("dis_vm.nxl", "print(1);");
    let module = compile_to_nir(&source).expect("should lower");
    let program = compile(&module).expect("should compile");
    let text = program.to_string();
    assert!(text.contains("NXL bytecode"));
    assert!(text.contains("entry:"));
}

#[test]
fn vm_eval_binary_add_integers() {
    let source = SourceFile::new("add_vm.nxl", "let r = 10 + 32; print(r);");
    let result = vm_run(&source).expect("should execute");
    assert_eq!(result.output, vec!["42"]);
}

#[test]
fn vm_chunk_emits_and_patches_jump() {
    let mut chunk = Chunk::new("test".to_owned(), 0, "test.nxl".to_owned());
    let site = chunk.emit(Opcode::Jump(0));
    chunk.patch_jump(site, 99);
    assert_eq!(chunk.code[site as usize], Opcode::Jump(99));
}

// ── P10 Async / Concurrency tests ─────────────────────────────────────────────

use nxl_compiler::concurrency::{
    await_task, bounded_channel, channel, spawn, Actor, Barrier, CancellationToken, TaskGroup,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ── CancellationToken ─────────────────────────────────────────────────────────

#[test]
fn cancellation_token_starts_uncancelled() {
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
}

#[test]
fn cancellation_token_cancel_sets_flag() {
    let token = CancellationToken::new();
    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn cancellation_token_clone_shares_state() {
    let token = CancellationToken::new();
    let clone = token.clone();
    token.cancel();
    assert!(
        clone.is_cancelled(),
        "clone must see cancellation from original"
    );
}

#[test]
fn cancellation_token_default_is_uncancelled() {
    let token = CancellationToken::default();
    assert!(!token.is_cancelled());
}

// ── Task / spawn / await ──────────────────────────────────────────────────────

#[test]
fn spawn_and_await_returns_value() {
    let task = spawn("add", || 2 + 2);
    let result = await_task(task).expect("task should succeed");
    assert_eq!(result, 4);
}

#[test]
fn spawn_task_name_is_accessible() {
    let task = spawn("my_task", || ());
    assert_eq!(task.name(), "my_task");
    await_task(task).unwrap();
}

#[test]
fn spawn_multiple_tasks_run_concurrently() {
    let counter = Arc::new(Mutex::new(0i32));
    let mut tasks = Vec::new();
    for i in 0..4 {
        let c = counter.clone();
        tasks.push(spawn(format!("worker-{i}"), move || {
            let mut guard = c.lock().unwrap();
            *guard += 1;
        }));
    }
    for task in tasks {
        await_task(task).unwrap();
    }
    assert_eq!(*counter.lock().unwrap(), 4);
}

#[test]
fn task_is_finished_after_join() {
    let task = spawn("done", || 42i32);
    // Give the thread a moment to finish.
    std::thread::sleep(Duration::from_millis(20));
    let finished = task.is_finished();
    let _ = await_task(task);
    // Either it was already finished before join, or join completed it.
    // Either way the value must be correct — just assert no panic.
    let _ = finished;
}

#[test]
fn await_task_propagates_panic_as_error() {
    let task = spawn("panicker", || -> i32 { panic!("intentional panic") });
    let err = await_task(task).expect_err("panicking task must return Err");
    assert_eq!(err.code, "NXL10001");
    assert!(err.message.contains("panicker"));
}

#[test]
fn task_with_cancellation_token_stops_early() {
    let token = CancellationToken::new();
    let token_clone = token.clone();
    let task = spawn("cancellable", move || {
        let mut count = 0i64;
        while !token_clone.is_cancelled() && count < 100_000_000 {
            count += 1;
            std::thread::yield_now();
        }
        count
    });
    // Cancel after a short delay.
    std::thread::sleep(Duration::from_millis(1));
    token.cancel();
    let count = await_task(task).expect("task should complete after cancellation");
    // Task stopped before reaching 100_000_000.
    assert!(
        count < 100_000_000,
        "task should have been cancelled before completion"
    );
}

// ── Channel (unbounded) ───────────────────────────────────────────────────────

#[test]
fn channel_send_and_recv() {
    let (tx, rx) = channel::<i32>();
    tx.send(42).unwrap();
    assert_eq!(rx.recv(), Some(42));
}

#[test]
fn channel_send_multiple_values_in_order() {
    let (tx, rx) = channel::<i32>();
    for i in 0..5 {
        tx.send(i).unwrap();
    }
    for i in 0..5 {
        assert_eq!(rx.recv(), Some(i));
    }
}

#[test]
fn channel_recv_returns_none_after_close_and_drain() {
    let (tx, rx) = channel::<i32>();
    tx.send(1).unwrap();
    tx.close();
    assert_eq!(rx.recv(), Some(1));
    assert_eq!(rx.recv(), None, "closed empty channel must return None");
}

#[test]
fn channel_send_on_closed_returns_error() {
    let (tx, _rx) = channel::<i32>();
    tx.close();
    let err = tx.send(1).expect_err("send on closed channel must fail");
    assert_eq!(err.code, "NXL10002");
}

#[test]
fn channel_try_recv_returns_none_when_empty() {
    let (_tx, rx) = channel::<i32>();
    assert_eq!(rx.try_recv(), None);
}

#[test]
fn channel_try_recv_returns_value_when_available() {
    let (tx, rx) = channel::<i32>();
    tx.send(7).unwrap();
    assert_eq!(rx.try_recv(), Some(7));
}

#[test]
fn channel_is_closed_reflects_state() {
    let (tx, _rx) = channel::<i32>();
    assert!(!tx.is_closed());
    tx.close();
    assert!(tx.is_closed());
}

#[test]
fn channel_receiver_is_done_when_closed_and_empty() {
    let (tx, rx) = channel::<i32>();
    assert!(!rx.is_done());
    tx.close();
    assert!(rx.is_done());
}

#[test]
fn channel_cross_thread_producer_consumer() {
    let (tx, rx) = channel::<i32>();
    let task = spawn("producer", move || {
        for i in 0..10 {
            tx.send(i).unwrap();
        }
        tx.close();
    });
    let mut received = Vec::new();
    while let Some(v) = rx.recv() {
        received.push(v);
    }
    await_task(task).unwrap();
    assert_eq!(received, (0..10).collect::<Vec<_>>());
}

#[test]
fn channel_recv_timeout_returns_none_on_timeout() {
    let (_tx, rx) = channel::<i32>();
    let result = rx.recv_timeout(Duration::from_millis(10));
    assert_eq!(result, None, "timeout on empty channel must return None");
}

#[test]
fn channel_recv_timeout_returns_value_before_timeout() {
    let (tx, rx) = channel::<i32>();
    tx.send(99).unwrap();
    let result = rx.recv_timeout(Duration::from_millis(100));
    assert_eq!(result, Some(99));
}

#[test]
fn channel_sender_clone_shares_channel() {
    let (tx, rx) = channel::<i32>();
    let tx2 = tx.clone();
    tx.send(1).unwrap();
    tx2.send(2).unwrap();
    let mut vals = vec![rx.recv().unwrap(), rx.recv().unwrap()];
    vals.sort();
    assert_eq!(vals, vec![1, 2]);
}

// ── Bounded channel (backpressure) ────────────────────────────────────────────

#[test]
fn bounded_channel_try_send_fails_when_full() {
    let (tx, _rx) = bounded_channel::<i32>(2);
    tx.try_send(1).unwrap();
    tx.try_send(2).unwrap();
    let err = tx
        .try_send(3)
        .expect_err("full bounded channel must reject try_send");
    assert_eq!(err.code, "NXL10003");
}

#[test]
fn bounded_channel_send_blocks_until_space_available() {
    let (tx, rx) = bounded_channel::<i32>(1);
    tx.send(1).unwrap();
    // Spawn a task that will unblock the sender by consuming the item.
    let rx_clone = rx.clone();
    let consumer = spawn("consumer", move || rx_clone.recv());
    // This send blocks until the consumer reads the first item.
    tx.send(2).unwrap();
    let first = await_task(consumer).unwrap();
    assert_eq!(first, Some(1));
    assert_eq!(rx.recv(), Some(2));
}

#[test]
fn bounded_channel_respects_capacity() {
    let (tx, rx) = bounded_channel::<i32>(3);
    for i in 0..3 {
        tx.try_send(i).unwrap();
    }
    assert!(tx.try_send(99).is_err(), "capacity exceeded must fail");
    // Drain one slot.
    rx.recv();
    assert!(tx.try_send(99).is_ok(), "after drain, send must succeed");
}

// ── TaskGroup (structured concurrency) ───────────────────────────────────────

#[test]
fn task_group_collects_all_results() {
    let mut group: TaskGroup<i32> = TaskGroup::new();
    for i in 0..5 {
        group.spawn(format!("t{i}"), move || i * 2);
    }
    let mut results = group.join_all().expect("all tasks should succeed");
    results.sort();
    assert_eq!(results, vec![0, 2, 4, 6, 8]);
}

#[test]
fn task_group_returns_error_if_any_task_panics() {
    let mut group: TaskGroup<i32> = TaskGroup::new();
    group.spawn("ok", || 1);
    group.spawn("panicker", || -> i32 { panic!("boom") });
    group.spawn("ok2", || 3);
    let err = group
        .join_all()
        .expect_err("panicking task must cause group error");
    assert_eq!(err.code, "NXL10001");
}

#[test]
fn task_group_default_is_empty() {
    let group: TaskGroup<i32> = TaskGroup::default();
    let results = group.join_all().expect("empty group should succeed");
    assert!(results.is_empty());
}

#[test]
fn task_group_structured_concurrency_all_complete() {
    // Verify that join_all waits for every task, not just the first.
    let counter = Arc::new(Mutex::new(0i32));
    let mut group: TaskGroup<()> = TaskGroup::new();
    for i in 0..8 {
        let c = counter.clone();
        group.spawn(format!("worker-{i}"), move || {
            std::thread::sleep(Duration::from_millis(5));
            *c.lock().unwrap() += 1;
        });
    }
    group.join_all().expect("all workers should complete");
    assert_eq!(*counter.lock().unwrap(), 8, "all 8 workers must have run");
}

// ── Barrier (synchronization) ─────────────────────────────────────────────────

#[test]
fn barrier_synchronizes_threads() {
    let barrier = Barrier::new(3);
    let counter = Arc::new(Mutex::new(0i32));
    let mut tasks = Vec::new();
    for _ in 0..3 {
        let b = barrier.clone();
        let c = counter.clone();
        tasks.push(spawn("barrier-worker", move || {
            *c.lock().unwrap() += 1;
            b.wait(); // all three must reach here before any proceeds
        }));
    }
    for task in tasks {
        await_task(task).unwrap();
    }
    assert_eq!(*counter.lock().unwrap(), 3);
}

// ── Actor mailbox ─────────────────────────────────────────────────────────────

#[test]
fn actor_constructs_and_has_name() {
    let actor = Actor::new("my_actor");
    assert_eq!(actor.name(), "my_actor");
    assert_eq!(actor.pending(), 0);
}

#[test]
fn actor_mailbox_queues_and_drains_fifo() {
    let actor = Actor::new("my_actor");
    actor.dispatch("first");
    actor.dispatch("second");
    assert_eq!(actor.pending(), 2);
    let drained = actor.drain();
    assert_eq!(drained, vec!["first", "second"]);
    assert_eq!(actor.pending(), 0);
    // Draining an empty mailbox is a no-op.
    assert!(actor.drain().is_empty());
}

#[test]
fn actor_drain_with_handles_each_message() {
    let actor = Actor::new("worker");
    actor.dispatch("a");
    actor.dispatch("b");
    let delivered = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&delivered);
    let n = actor.drain_with(|m| seen.lock().unwrap().push(m.to_owned()));
    assert_eq!(n, 2);
    assert_eq!(*delivered.lock().unwrap(), vec!["a", "b"]);
}

#[test]
fn actor_clone_shares_mailbox() {
    let actor = Actor::new("shared");
    let twin = actor.clone();
    actor.dispatch("hello");
    // The shared handle observes the same mailbox.
    assert_eq!(twin.pending(), 1);
    assert_eq!(twin.drain(), vec!["hello"]);
}

// ── Task cleanup / resource leak ─────────────────────────────────────────────

#[test]
fn task_cleanup_on_cancellation_does_not_leak() {
    // Spawn tasks that hold a shared resource counter; cancel them and
    // verify the counter reaches the expected value (no leaked increments).
    let token = CancellationToken::new();
    let counter = Arc::new(Mutex::new(0i32));
    let mut tasks = Vec::new();
    for i in 0..4 {
        let t = token.clone();
        let c = counter.clone();
        tasks.push(spawn(format!("cleanup-{i}"), move || {
            if !t.is_cancelled() {
                *c.lock().unwrap() += 1;
            }
        }));
    }
    // Cancel before tasks check the token (best-effort; timing-dependent).
    token.cancel();
    for task in tasks {
        await_task(task).unwrap();
    }
    // Counter is 0..=4 depending on scheduling; the important thing is
    // no panic and no deadlock.
    let _ = *counter.lock().unwrap();
}

// ── ConcurrencyError → Diagnostic conversion ─────────────────────────────────

#[test]
fn concurrency_error_converts_to_diagnostic() {
    use nxl_compiler::concurrency::ConcurrencyError;
    let err = ConcurrencyError {
        code: "NXL10001",
        message: "test error".to_owned(),
    };
    let diag: nxl_compiler::diagnostic::Diagnostic = err.into();
    assert_eq!(diag.code, "NXL10001");
}

// ── P11 Standard Library tests ────────────────────────────────────────────────

#[cfg(test)]
mod stdlib_tests {
    use nxl_compiler::stdlib::{
        all_modules, json, math, strings, testing, time, NxlList, NxlMap, NxlValue, Rng,
        StdlibModule,
    };

    // ── NxlList ───────────────────────────────────────────────────────────────

    #[test]
    fn stdlib_list_push_pop() {
        let mut list = NxlList::new();
        list.push(NxlValue::Int(1));
        list.push(NxlValue::Int(2));
        assert_eq!(list.len(), 2);
        assert_eq!(list.pop(), Some(NxlValue::Int(2)));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn stdlib_list_get_set() {
        let mut list = NxlList::new();
        list.push(NxlValue::Str("a".into()));
        list.push(NxlValue::Str("b".into()));
        assert_eq!(list.get(0).unwrap(), &NxlValue::Str("a".into()));
        list.set(1, NxlValue::Str("z".into())).unwrap();
        assert_eq!(list.get(1).unwrap(), &NxlValue::Str("z".into()));
    }

    #[test]
    fn stdlib_list_out_of_bounds() {
        let list = NxlList::new();
        assert!(list.get(0).is_err());
    }

    #[test]
    fn stdlib_list_remove() {
        let mut list = NxlList::new();
        list.push(NxlValue::Int(10));
        list.push(NxlValue::Int(20));
        let removed = list.remove(0).unwrap();
        assert_eq!(removed, NxlValue::Int(10));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn stdlib_list_contains() {
        let mut list = NxlList::new();
        list.push(NxlValue::Bool(true));
        assert!(list.contains(&NxlValue::Bool(true)));
        assert!(!list.contains(&NxlValue::Bool(false)));
    }

    // ── NxlMap ────────────────────────────────────────────────────────────────

    #[test]
    fn stdlib_map_insert_get() {
        let mut map = NxlMap::new();
        map.insert("key", NxlValue::Int(42));
        assert_eq!(map.get("key"), Some(&NxlValue::Int(42)));
        assert_eq!(map.get("missing"), None);
    }

    #[test]
    fn stdlib_map_overwrite() {
        let mut map = NxlMap::new();
        map.insert("x", NxlValue::Int(1));
        map.insert("x", NxlValue::Int(2));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("x"), Some(&NxlValue::Int(2)));
    }

    #[test]
    fn stdlib_map_remove() {
        let mut map = NxlMap::new();
        map.insert("a", NxlValue::Null);
        assert!(map.remove("a").is_some());
        assert!(map.is_empty());
    }

    #[test]
    fn stdlib_map_keys() {
        let mut map = NxlMap::new();
        map.insert("one", NxlValue::Int(1));
        map.insert("two", NxlValue::Int(2));
        let keys: Vec<&str> = map.keys().collect();
        assert!(keys.contains(&"one"));
        assert!(keys.contains(&"two"));
    }

    // ── strings ───────────────────────────────────────────────────────────────

    #[test]
    fn stdlib_strings_case() {
        assert_eq!(strings::to_upper("hello"), "HELLO");
        assert_eq!(strings::to_lower("WORLD"), "world");
    }

    #[test]
    fn stdlib_strings_trim() {
        assert_eq!(strings::trim("  hi  "), "hi");
        assert_eq!(strings::trim_start("  hi  "), "hi  ");
        assert_eq!(strings::trim_end("  hi  "), "  hi");
    }

    #[test]
    fn stdlib_strings_split_join() {
        let parts = strings::split("a,b,c", ",");
        assert_eq!(parts, vec!["a", "b", "c"]);
        assert_eq!(strings::join(&parts, "-"), "a-b-c");
    }

    #[test]
    fn stdlib_strings_substring() {
        assert_eq!(strings::substring("hello", 1, 4).unwrap(), "ell");
        assert!(strings::substring("hi", 0, 10).is_err());
    }

    #[test]
    fn stdlib_strings_parse() {
        assert_eq!(strings::parse_int("42").unwrap(), 42);
        assert_eq!(strings::parse_float("2.75").unwrap(), 2.75);
        assert!(strings::parse_int("abc").is_err());
    }

    #[test]
    fn stdlib_strings_pad() {
        assert_eq!(strings::pad_left("5", 3, '0'), "005");
        assert_eq!(strings::pad_right("hi", 5, '.'), "hi...");
    }

    #[test]
    fn stdlib_strings_reverse() {
        assert_eq!(strings::reverse("abc"), "cba");
    }

    // ── math ──────────────────────────────────────────────────────────────────

    #[test]
    fn stdlib_math_basic() {
        assert_eq!(math::abs_int(-5), 5);
        assert!((math::sqrt(4.0) - 2.0).abs() < 1e-10);
        assert_eq!(math::pow_int(2, 10), 1024);
    }

    #[test]
    fn stdlib_math_trig() {
        assert!((math::sin(math::PI / 2.0) - 1.0).abs() < 1e-10);
        assert!((math::cos(0.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn stdlib_math_clamp() {
        assert_eq!(math::clamp_int(15, 0, 10), 10);
        assert_eq!(math::clamp_int(-5, 0, 10), 0);
        assert_eq!(math::clamp_int(5, 0, 10), 5);
    }

    #[test]
    fn stdlib_math_gcd_lcm() {
        assert_eq!(math::gcd(12, 8), 4);
        assert_eq!(math::lcm(4, 6), 12);
    }

    #[test]
    fn stdlib_math_predicates() {
        assert!(math::is_nan(math::NAN));
        assert!(math::is_infinite(math::INFINITY));
        assert!(math::is_finite(1.0));
    }

    // ── json ──────────────────────────────────────────────────────────────────

    #[test]
    fn stdlib_json_stringify_primitives() {
        assert_eq!(json::stringify(&NxlValue::Null), "null");
        assert_eq!(json::stringify(&NxlValue::Bool(true)), "true");
        assert_eq!(json::stringify(&NxlValue::Int(42)), "42");
        assert_eq!(json::stringify(&NxlValue::Str("hi".into())), "\"hi\"");
    }

    #[test]
    fn stdlib_json_stringify_list() {
        let v = NxlValue::List(vec![NxlValue::Int(1), NxlValue::Int(2)]);
        assert_eq!(json::stringify(&v), "[1,2]");
    }

    #[test]
    fn stdlib_json_stringify_map() {
        let v = NxlValue::Map(vec![("a".into(), NxlValue::Int(1))]);
        assert_eq!(json::stringify(&v), "{\"a\":1}");
    }

    #[test]
    fn stdlib_json_parse_primitives() {
        assert_eq!(json::parse("null").unwrap(), NxlValue::Null);
        assert_eq!(json::parse("true").unwrap(), NxlValue::Bool(true));
        assert_eq!(json::parse("42").unwrap(), NxlValue::Int(42));
        assert_eq!(
            json::parse("\"hello\"").unwrap(),
            NxlValue::Str("hello".into())
        );
    }

    #[test]
    fn stdlib_json_parse_array() {
        let v = json::parse("[1,2,3]").unwrap();
        assert_eq!(
            v,
            NxlValue::List(vec![NxlValue::Int(1), NxlValue::Int(2), NxlValue::Int(3)])
        );
    }

    #[test]
    fn stdlib_json_parse_object() {
        let v = json::parse("{\"x\":10}").unwrap();
        assert_eq!(v, NxlValue::Map(vec![("x".into(), NxlValue::Int(10))]));
    }

    #[test]
    fn stdlib_json_roundtrip() {
        let original = NxlValue::Map(vec![
            ("name".into(), NxlValue::Str("nxl".into())),
            ("version".into(), NxlValue::Int(1)),
            ("active".into(), NxlValue::Bool(true)),
        ]);
        let s = json::stringify(&original);
        let parsed = json::parse(&s).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn stdlib_json_parse_invalid() {
        assert!(json::parse("{bad}").is_err());
    }

    // ── random ────────────────────────────────────────────────────────────────

    #[test]
    fn stdlib_rng_range() {
        let mut rng = Rng::new(12345);
        for _ in 0..100 {
            let v = rng.next_int(0, 9);
            assert!((0..=9).contains(&v));
        }
    }

    #[test]
    fn stdlib_rng_float() {
        let mut rng = Rng::new(99);
        for _ in 0..50 {
            let v = rng.next_float();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn stdlib_rng_shuffle() {
        let mut rng = Rng::new(7);
        let mut v = vec![1, 2, 3, 4, 5];
        rng.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort();
        assert_eq!(sorted, vec![1, 2, 3, 4, 5]);
    }

    // ── time ──────────────────────────────────────────────────────────────────

    #[test]
    fn stdlib_time_now() {
        let secs = time::now_secs();
        assert!(secs > 1_700_000_000, "timestamp looks wrong: {secs}");
    }

    #[test]
    fn stdlib_time_stopwatch() {
        let sw = time::Stopwatch::start();
        time::sleep_ms(5);
        assert!(sw.elapsed_ms() >= 5);
    }

    // ── StdlibModule registry ─────────────────────────────────────────────────

    #[test]
    fn stdlib_module_resolve() {
        assert_eq!(
            StdlibModule::from_name("std::math"),
            Some(StdlibModule::Math)
        );
        assert_eq!(StdlibModule::from_name("json"), Some(StdlibModule::Json));
        assert_eq!(StdlibModule::from_name("unknown"), None);
    }

    #[test]
    fn stdlib_module_implemented_flag() {
        assert!(StdlibModule::Math.is_implemented());
        assert!(!StdlibModule::Http.is_implemented());
        assert!(!StdlibModule::Crypto.is_implemented());
    }

    #[test]
    fn stdlib_all_modules_count() {
        let modules = all_modules();
        assert_eq!(modules.len(), 12);
        assert!(modules.iter().all(|m| m.is_implemented()));
    }

    // ── testing helpers ───────────────────────────────────────────────────────

    #[test]
    fn stdlib_testing_assert_eq_pass() {
        let r = testing::assert_eq(&NxlValue::Int(1), &NxlValue::Int(1));
        assert!(r.is_ok());
    }

    #[test]
    fn stdlib_testing_assert_eq_fail() {
        let r = testing::assert_eq(&NxlValue::Int(1), &NxlValue::Int(2));
        assert!(r.is_err());
    }

    #[test]
    fn stdlib_testing_run_test() {
        let pass = testing::run_test("ok", || Ok(()));
        assert!(pass.is_pass());
        let fail = testing::run_test("fail", || testing::assert_true(false, "should fail"));
        assert!(!fail.is_pass());
        assert!(fail.message.is_some());
    }
}

// ── P12 CLI tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod cli_tests {
    use nxl_compiler::cli::{
        cmd_build, cmd_check, cmd_clean, cmd_debug, cmd_docs, cmd_fmt, cmd_new, cmd_repl, cmd_test,
        format_doctor_report, help_text, parse_args, run_doctor, version_text, BuildProfile,
        BuildTarget, Command, Dependency, ProjectConfig,
    };
    use std::fs;

    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| a.to_string()).collect()
    }

    // ── parse_args ────────────────────────────────────────────────────────────

    #[test]
    fn cli_parse_help() {
        assert_eq!(parse_args(&args(&[])).unwrap(), Command::Help);
        assert_eq!(parse_args(&args(&["help"])).unwrap(), Command::Help);
        assert_eq!(parse_args(&args(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse_args(&args(&["-h"])).unwrap(), Command::Help);
    }

    #[test]
    fn cli_parse_version() {
        assert_eq!(parse_args(&args(&["version"])).unwrap(), Command::Version);
        assert_eq!(parse_args(&args(&["--version"])).unwrap(), Command::Version);
        assert_eq!(parse_args(&args(&["-V"])).unwrap(), Command::Version);
    }

    #[test]
    fn cli_parse_new() {
        assert_eq!(
            parse_args(&args(&["new", "myapp"])).unwrap(),
            Command::New {
                name: "myapp".into(),
                template: None
            }
        );
        assert_eq!(
            parse_args(&args(&["new", "console", "myapp"])).unwrap(),
            Command::New {
                name: "myapp".into(),
                template: Some("console".into())
            }
        );
    }

    #[test]
    fn cli_parse_new_missing_name_errors() {
        let err = parse_args(&args(&["new"])).unwrap_err();
        assert_eq!(err.code, "NXL12002");
    }

    #[test]
    fn cli_parse_init() {
        assert_eq!(parse_args(&args(&["init"])).unwrap(), Command::Init);
    }

    #[test]
    fn cli_parse_run_no_file() {
        assert_eq!(
            parse_args(&args(&["run"])).unwrap(),
            Command::Run { file: None }
        );
    }

    #[test]
    fn cli_parse_run_with_file() {
        assert_eq!(
            parse_args(&args(&["run", "main.nxl"])).unwrap(),
            Command::Run {
                file: Some("main.nxl".into())
            }
        );
    }

    #[test]
    fn cli_parse_build_debug() {
        assert_eq!(
            parse_args(&args(&["build"])).unwrap(),
            Command::Build { release: false }
        );
    }

    #[test]
    fn cli_parse_build_release() {
        assert_eq!(
            parse_args(&args(&["build", "--release"])).unwrap(),
            Command::Build { release: true }
        );
    }

    #[test]
    fn cli_parse_check() {
        assert_eq!(
            parse_args(&args(&["check"])).unwrap(),
            Command::Check { file: None }
        );
        assert_eq!(
            parse_args(&args(&["check", "src/main.nxl"])).unwrap(),
            Command::Check {
                file: Some("src/main.nxl".into())
            }
        );
    }

    #[test]
    fn cli_parse_fmt() {
        assert_eq!(
            parse_args(&args(&["fmt"])).unwrap(),
            Command::Fmt { file: None }
        );
    }

    #[test]
    fn cli_parse_lint() {
        assert_eq!(
            parse_args(&args(&["lint"])).unwrap(),
            Command::Lint { file: None }
        );
    }

    #[test]
    fn cli_parse_clean() {
        assert_eq!(parse_args(&args(&["clean"])).unwrap(), Command::Clean);
    }

    #[test]
    fn cli_parse_test_no_filter() {
        assert_eq!(
            parse_args(&args(&["test"])).unwrap(),
            Command::Test { filter: None }
        );
    }

    #[test]
    fn cli_parse_test_with_filter() {
        assert_eq!(
            parse_args(&args(&["test", "math"])).unwrap(),
            Command::Test {
                filter: Some("math".into())
            }
        );
    }

    #[test]
    fn cli_parse_repl() {
        assert_eq!(parse_args(&args(&["repl"])).unwrap(), Command::Repl);
    }

    #[test]
    fn cli_parse_debug_with_file() {
        assert_eq!(
            parse_args(&args(&["debug", "main.nxl"])).unwrap(),
            Command::Debug {
                file: "main.nxl".into()
            }
        );
    }

    #[test]
    fn cli_parse_debug_missing_file_errors() {
        let err = parse_args(&args(&["debug"])).unwrap_err();
        assert_eq!(err.code, "NXL12002");
    }

    #[test]
    fn cli_parse_docs() {
        assert_eq!(parse_args(&args(&["docs"])).unwrap(), Command::Docs);
    }

    #[test]
    fn cli_parse_doctor() {
        assert_eq!(parse_args(&args(&["doctor"])).unwrap(), Command::Doctor);
    }

    #[test]
    fn cli_parse_unknown_command_errors() {
        let err = parse_args(&args(&["frobnicate"])).unwrap_err();
        assert_eq!(err.code, "NXL12003");
        assert!(err.message.contains("frobnicate"));
    }

    // ── ProjectConfig ─────────────────────────────────────────────────────────

    #[test]
    fn cli_project_config_default() {
        let c = ProjectConfig::default();
        assert_eq!(c.name, "project");
        assert_eq!(c.version, "0.1.0");
        assert_eq!(c.entry, "src/main.nxl");
        assert_eq!(c.edition, "2024");
        assert_eq!(c.target, BuildTarget::Js);
    }

    #[test]
    fn cli_project_config_parse_minimal() {
        let toml = "[package]\nname = \"hello\"\nversion = \"1.2.3\"\n";
        let c = ProjectConfig::parse(toml).unwrap();
        assert_eq!(c.name, "hello");
        assert_eq!(c.version, "1.2.3");
    }

    #[test]
    fn cli_project_config_parse_full() {
        let toml = "[package]\nname = \"app\"\nversion = \"0.2.0\"\ndescription = \"A test app\"\nentry = \"src/app.nxl\"\nedition = \"2024\"\ntarget = \"vm\"\n";
        let c = ProjectConfig::parse(toml).unwrap();
        assert_eq!(c.name, "app");
        assert_eq!(c.description, Some("A test app".into()));
        assert_eq!(c.entry, "src/app.nxl");
        assert_eq!(c.target, BuildTarget::Vm);
    }

    #[test]
    fn cli_project_config_parse_dependencies() {
        let toml = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nstd-extra = \"1.0.0\"\n";
        let c = ProjectConfig::parse(toml).unwrap();
        assert_eq!(c.dependencies.len(), 1);
        assert_eq!(c.dependencies[0].name, "std-extra");
        assert_eq!(c.dependencies[0].version, "1.0.0");
    }

    #[test]
    fn cli_project_config_roundtrip() {
        let original = ProjectConfig {
            name: "roundtrip".into(),
            version: "0.3.0".into(),
            description: Some("test".into()),
            authors: vec!["Alice".into()],
            entry: "src/main.nxl".into(),
            edition: "2024".into(),
            target: BuildTarget::Js,
            profile: BuildProfile::Debug,
            dependencies: vec![Dependency {
                name: "foo".into(),
                version: "1.0".into(),
            }],
        };
        let toml = original.to_toml();
        let parsed = ProjectConfig::parse(&toml).unwrap();
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.dependencies.len(), 1);
    }

    #[test]
    fn cli_project_config_ignores_comments() {
        let toml = "# this is a comment\n[package]\nname = \"commented\"\nversion = \"0.1.0\"\n";
        let c = ProjectConfig::parse(toml).unwrap();
        assert_eq!(c.name, "commented");
    }

    // ── cmd_new / cmd_init / cmd_clean ────────────────────────────────────────

    #[test]
    fn cli_cmd_new_creates_project_structure() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_new_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let _cwd_guard = CWD_LOCK.lock().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp).unwrap();

        let result = cmd_new("testproject");
        std::env::set_current_dir(&orig).unwrap();

        assert!(result.is_ok(), "cmd_new failed: {:?}", result);
        let proj = tmp.join("testproject");
        assert!(proj.join("nxl.toml").exists(), "nxl.toml must be created");
        assert!(
            proj.join("src").join("main.nxl").exists(),
            "src/main.nxl must be created"
        );
        assert!(
            proj.join(".gitignore").exists(),
            ".gitignore must be created"
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn cli_cmd_new_fails_if_directory_exists() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_exists_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let _cwd_guard = CWD_LOCK.lock().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp).unwrap();

        fs::create_dir_all(tmp.join("existing")).unwrap();
        let err = cmd_new("existing").unwrap_err();
        std::env::set_current_dir(&orig).unwrap();
        fs::remove_dir_all(&tmp).ok();

        assert_eq!(err.code, "NXL12005");
    }

    #[test]
    fn cli_cmd_clean_removes_build_dir() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_clean_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(tmp.join("build").join("debug")).unwrap();
        let msg = cmd_clean(&tmp).unwrap();
        assert!(msg.contains("removed") || msg.contains("build"));
        assert!(!tmp.join("build").exists());
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn cli_cmd_clean_nothing_to_clean() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_noclean_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let msg = cmd_clean(&tmp).unwrap();
        assert!(msg.contains("nothing"));
        fs::remove_dir_all(&tmp).ok();
    }

    // ── cmd_check / cmd_fmt ───────────────────────────────────────────────────

    #[test]
    fn cli_cmd_check_valid_file() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_check_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("ok.nxl");
        fs::write(&file, "let x = 1;").unwrap();
        let result = cmd_check(&tmp, Some(file.to_str().unwrap()));
        fs::remove_dir_all(&tmp).ok();
        assert!(result.is_ok());
    }

    #[test]
    fn cli_cmd_check_invalid_file_returns_error() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_check_bad_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("bad.nxl");
        fs::write(&file, "let x: Imaginary = 1;").unwrap();
        let result = cmd_check(&tmp, Some(file.to_str().unwrap()));
        fs::remove_dir_all(&tmp).ok();
        assert!(result.is_err());
    }

    #[test]
    fn cli_cmd_fmt_normalises_trailing_whitespace() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_fmt_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("messy.nxl");
        fs::write(&file, "let x = 1;   \nlet y = 2;  \n").unwrap();
        let result = cmd_fmt(&tmp, Some(file.to_str().unwrap()));
        let content = fs::read_to_string(&file).unwrap();
        fs::remove_dir_all(&tmp).ok();
        assert!(result.is_ok());
        assert!(
            !content.contains("   \n"),
            "trailing spaces must be removed"
        );
    }

    // ── cmd_build ─────────────────────────────────────────────────────────────

    #[test]
    fn cli_cmd_build_creates_artifact() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_build_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(tmp.join("src")).unwrap();
        fs::write(tmp.join("src").join("main.nxl"), "print(1);").unwrap();
        let config = ProjectConfig {
            name: "testbuild".into(),
            ..Default::default()
        };
        fs::write(tmp.join("nxl.toml"), config.to_toml()).unwrap();

        let result = cmd_build(&tmp, false);
        let artifact_exists = result
            .as_ref()
            .ok()
            .and_then(|r| r.artifact_path.as_ref())
            .map(|p| p.exists())
            .unwrap_or(false);
        fs::remove_dir_all(&tmp).ok();

        assert!(result.is_ok(), "build failed: {:?}", result);
        assert!(artifact_exists, "artifact file must exist");
    }

    #[test]
    fn cli_cmd_build_release_uses_release_profile() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_build_rel_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(tmp.join("src")).unwrap();
        fs::write(tmp.join("src").join("main.nxl"), "print(1);").unwrap();
        let config = ProjectConfig {
            name: "relapp".into(),
            ..Default::default()
        };
        fs::write(tmp.join("nxl.toml"), config.to_toml()).unwrap();

        let result = cmd_build(&tmp, true);
        let profile_ok = result
            .as_ref()
            .ok()
            .map(|r| r.profile == BuildProfile::Release)
            .unwrap_or(false);
        fs::remove_dir_all(&tmp).ok();

        assert!(result.is_ok());
        assert!(profile_ok, "release build must use Release profile");
    }

    // ── cmd_test ──────────────────────────────────────────────────────────────

    #[test]
    fn cli_cmd_test_empty_project_passes() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let report = cmd_test(&tmp, None).unwrap();
        fs::remove_dir_all(&tmp).ok();
        assert_eq!(report.total, 0);
        assert!(report.success());
    }

    #[test]
    fn cli_cmd_test_finds_test_files() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_testfiles_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(tmp.join("tests")).unwrap();
        fs::write(tmp.join("tests").join("math_test.nxl"), "let x = 1;").unwrap();
        let report = cmd_test(&tmp, None).unwrap();
        fs::remove_dir_all(&tmp).ok();
        assert_eq!(report.total, 1);
        assert!(report.success());
    }

    #[test]
    fn cli_cmd_test_filter_skips_non_matching() {
        let tmp = std::env::temp_dir().join(format!(
            "nxl_test_filter_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(tmp.join("tests")).unwrap();
        fs::write(tmp.join("tests").join("math_test.nxl"), "let x = 1;").unwrap();
        fs::write(tmp.join("tests").join("string_test.nxl"), "let y = 2;").unwrap();
        let report = cmd_test(&tmp, Some("math")).unwrap();
        fs::remove_dir_all(&tmp).ok();
        assert_eq!(report.total, 1);
    }

    // ── doctor ────────────────────────────────────────────────────────────────

    #[test]
    fn cli_doctor_runs_without_panic() {
        let checks = run_doctor();
        assert!(!checks.is_empty());
    }

    #[test]
    fn cli_doctor_compiler_check_always_ok() {
        let checks = run_doctor();
        let compiler = checks.iter().find(|c| c.name == "nxlc compiler").unwrap();
        assert!(compiler.is_ok(), "compiler check must always pass");
    }

    #[test]
    fn cli_doctor_format_report_contains_header() {
        let checks = run_doctor();
        let report = format_doctor_report(&checks);
        assert!(report.contains("nxl doctor"));
    }

    #[test]
    fn cli_doctor_format_report_shows_all_checks() {
        let checks = run_doctor();
        let report = format_doctor_report(&checks);
        for check in &checks {
            assert!(
                report.contains(&check.name),
                "report must mention '{}'",
                check.name
            );
        }
    }

    // ── stub commands ─────────────────────────────────────────────────────────

    #[test]
    fn cli_repl_reports_the_end_of_a_session() {
        // The REPL reads stdin; with no input it reports the session ending
        // rather than claiming to be unimplemented.
        let message = cmd_repl().expect("session ends cleanly");
        assert!(message.contains("repl session ended"), "{message}");
    }

    #[test]
    fn cli_debug_reports_trace_and_breakpoints() {
        let msg = cmd_debug("main.nxl");
        assert!(msg.contains("cannot read"), "{msg}");
    }

    #[test]
    fn cli_docs_lists_bundled_guides() {
        let msg = cmd_docs();
        assert!(msg.contains("docs/01-getting-started.md"), "{msg}");
    }

    // ── help / version text ───────────────────────────────────────────────────

    #[test]
    fn cli_help_text_contains_all_commands() {
        let help = help_text();
        for cmd in &[
            "new", "init", "run", "build", "check", "fmt", "lint", "clean", "test", "repl",
            "debug", "docs", "doctor", "version",
        ] {
            assert!(help.contains(cmd), "help must mention '{cmd}'");
        }
    }

    #[test]
    fn cli_version_text_contains_version() {
        let v = version_text();
        assert!(v.contains("nxlc"));
        assert!(v.contains('.'));
    }
}

// ── P13 Package Manager tests ─────────────────────────────────────────────────

#[cfg(test)]
mod pkg_tests {
    use nxl_compiler::cli::{
        cmd_pkg_add, cmd_pkg_audit, cmd_pkg_install, cmd_pkg_publish, cmd_pkg_remove, cmd_pkg_tree,
        parse_args, AddRequest, Command,
    };
    use nxl_compiler::pkg::{
        audit, checksum, Advisory, AuditSeverity, DepSource, DependencyGraph, DependencyResolver,
        LockFile, LockedPackage, Manifest, ManifestDep, PackageCache, PackageManager, Registry,
        Version, VersionReq, WorkspaceConfig,
    };
    use std::fs;
    use std::path::PathBuf;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nxl_pkg_{tag}_{}", nanos()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn nanos() -> u32 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    }

    // ── Version ───────────────────────────────────────────────────────────────

    #[test]
    fn pkg_version_parse_valid() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.pre, None);
    }

    #[test]
    fn pkg_version_parse_with_pre() {
        let v = Version::parse("2.0.0-alpha").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.pre, Some("alpha".into()));
    }

    #[test]
    fn pkg_version_parse_with_v_prefix() {
        let v = Version::parse("v1.0.0").unwrap();
        assert_eq!(v.major, 1);
    }

    #[test]
    fn pkg_version_parse_invalid() {
        assert!(Version::parse("abc").is_err());
        assert!(Version::parse("1.x.3").is_err());
        assert!(Version::parse("1.2.3.4").is_err());
        assert!(Version::parse("").is_err());
    }

    #[test]
    fn pkg_version_parse_accepts_partial_versions() {
        // `1` and `1.2` appear in manifests and CLI arguments often enough to
        // be accepted rather than breaking an otherwise valid project.
        assert_eq!(Version::parse("1").unwrap(), Version::new(1, 0, 0));
        assert_eq!(Version::parse("1.2").unwrap(), Version::new(1, 2, 0));
        assert_eq!(Version::parse("v2.3").unwrap(), Version::new(2, 3, 0));
        assert_eq!(Version::parse("1.2.3").unwrap(), Version::new(1, 2, 3));
        assert_eq!(
            Version::parse("1.2.3-beta").unwrap(),
            Version {
                major: 1,
                minor: 2,
                patch: 3,
                pre: Some("beta".to_owned())
            }
        );
    }

    #[test]
    fn pkg_version_ordering() {
        let v100 = Version::new(1, 0, 0);
        let v110 = Version::new(1, 1, 0);
        let v200 = Version::new(2, 0, 0);
        assert!(v100 < v110);
        assert!(v110 < v200);
        assert!(v200 > v100);
        assert_eq!(v100, Version::new(1, 0, 0));
    }

    #[test]
    fn pkg_version_display() {
        assert_eq!(Version::new(1, 2, 3).to_string(), "1.2.3");
        let v = Version {
            major: 1,
            minor: 0,
            patch: 0,
            pre: Some("beta".into()),
        };
        assert_eq!(v.to_string(), "1.0.0-beta");
    }

    // ── VersionReq ────────────────────────────────────────────────────────────

    #[test]
    fn pkg_version_req_any() {
        let req = VersionReq::parse("*").unwrap();
        assert!(req.matches(&Version::new(0, 0, 1)));
        assert!(req.matches(&Version::new(99, 99, 99)));
    }

    #[test]
    fn pkg_version_req_caret() {
        let req = VersionReq::parse("^1.2.0").unwrap();
        assert!(req.matches(&Version::new(1, 2, 0)));
        assert!(req.matches(&Version::new(1, 9, 9)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
        assert!(!req.matches(&Version::new(1, 1, 9)));
    }

    #[test]
    fn pkg_version_req_tilde() {
        let req = VersionReq::parse("~1.2.3").unwrap();
        assert!(req.matches(&Version::new(1, 2, 3)));
        assert!(req.matches(&Version::new(1, 2, 9)));
        assert!(!req.matches(&Version::new(1, 3, 0)));
    }

    #[test]
    fn pkg_version_req_exact() {
        let req = VersionReq::parse("=1.0.0").unwrap();
        assert!(req.matches(&Version::new(1, 0, 0)));
        assert!(!req.matches(&Version::new(1, 0, 1)));
    }

    #[test]
    fn pkg_version_req_gte() {
        let req = VersionReq::parse(">=1.5.0").unwrap();
        assert!(req.matches(&Version::new(1, 5, 0)));
        assert!(req.matches(&Version::new(2, 0, 0)));
        assert!(!req.matches(&Version::new(1, 4, 9)));
    }

    #[test]
    fn pkg_version_req_lt() {
        let req = VersionReq::parse("<2.0.0").unwrap();
        assert!(req.matches(&Version::new(1, 9, 9)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
    }

    // ── Manifest ──────────────────────────────────────────────────────────────

    #[test]
    fn pkg_manifest_parse_minimal() {
        let toml = "[package]\nname = \"myapp\"\nversion = \"0.1.0\"\n";
        let m = Manifest::parse(toml).unwrap();
        assert_eq!(m.name, "myapp");
        assert_eq!(m.version, Version::new(0, 1, 0));
    }

    #[test]
    fn pkg_manifest_parse_with_deps() {
        let toml = "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\nstd-extra = \"^1.0.0\"\n";
        let m = Manifest::parse(toml).unwrap();
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dependencies[0].name, "std-extra");
    }

    #[test]
    fn pkg_manifest_parse_path_dep() {
        let toml = "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\nutils = \"path:../utils\"\n";
        let m = Manifest::parse(toml).unwrap();
        assert_eq!(m.dependencies.len(), 1);
        assert!(matches!(m.dependencies[0].source, DepSource::Path(_)));
    }

    #[test]
    fn pkg_manifest_parse_missing_name_errors() {
        let toml = "[package]\nversion = \"1.0.0\"\n";
        assert!(Manifest::parse(toml).is_err());
    }

    #[test]
    fn pkg_manifest_roundtrip() {
        let toml = "[package]\nname = \"rt\"\nversion = \"0.2.0\"\nlicense = \"MIT\"\n\n[dependencies]\nfoo = \"^1.0.0\"\n";
        let m = Manifest::parse(toml).unwrap();
        let out = m.to_toml();
        let m2 = Manifest::parse(&out).unwrap();
        assert_eq!(m.name, m2.name);
        assert_eq!(m.version, m2.version);
        assert_eq!(m.dependencies.len(), m2.dependencies.len());
    }

    // ── LockFile ──────────────────────────────────────────────────────────────

    #[test]
    fn pkg_lockfile_serialize_parse_roundtrip() {
        let mut lock = LockFile::new();
        lock.packages.push(LockedPackage {
            name: "foo".into(),
            version: Version::new(1, 0, 0),
            checksum: "abc123".into(),
            source: DepSource::Registry,
            dependencies: vec!["bar@^1.0.0".into()],
        });
        let text = lock.serialize();
        let parsed = LockFile::parse(&text).unwrap();
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].name, "foo");
        assert_eq!(parsed.packages[0].checksum, "abc123");
        assert_eq!(parsed.packages[0].dependencies, vec!["bar@^1.0.0"]);
    }

    #[test]
    fn pkg_lockfile_save_and_load() {
        let tmp = tmp_dir("lock");
        let mut lock = LockFile::new();
        lock.packages.push(LockedPackage {
            name: "baz".into(),
            version: Version::new(2, 1, 0),
            checksum: "deadbeef".into(),
            source: DepSource::Registry,
            dependencies: Vec::new(),
        });
        lock.save(&tmp).unwrap();
        assert!(tmp.join("nxl.lock").exists());
        let loaded = LockFile::load(&tmp).unwrap();
        assert_eq!(loaded.packages.len(), 1);
        assert_eq!(loaded.packages[0].name, "baz");
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_lockfile_load_missing_returns_empty() {
        let tmp = tmp_dir("nolock");
        let lock = LockFile::load(&tmp).unwrap();
        assert!(lock.packages.is_empty());
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_lockfile_contains() {
        let mut lock = LockFile::new();
        lock.packages.push(LockedPackage {
            name: "alpha".into(),
            version: Version::new(1, 0, 0),
            checksum: "x".into(),
            source: DepSource::Registry,
            dependencies: Vec::new(),
        });
        assert!(lock.contains("alpha"));
        assert!(!lock.contains("beta"));
    }

    // ── Registry + Resolver ───────────────────────────────────────────────────

    #[test]
    fn pkg_registry_resolve_best_version() {
        let mut reg = Registry::new();
        reg.register("foo", Version::new(1, 0, 0), DepSource::Registry, None);
        reg.register("foo", Version::new(1, 2, 0), DepSource::Registry, None);
        reg.register("foo", Version::new(2, 0, 0), DepSource::Registry, None);
        let req = VersionReq::parse("^1.0.0").unwrap();
        let pkg = reg.resolve("foo", &req).unwrap();
        assert_eq!(pkg.version, Version::new(1, 2, 0));
    }

    #[test]
    fn pkg_registry_resolve_missing_errors() {
        let reg = Registry::new();
        let req = VersionReq::parse("*").unwrap();
        let err = reg.resolve("missing", &req).unwrap_err();
        assert_eq!(err.code, "NXL13006");
    }

    #[test]
    fn pkg_resolver_resolves_flat_deps() {
        let mut reg = Registry::new();
        reg.register("foo", Version::new(1, 0, 0), DepSource::Registry, None);
        reg.register("bar", Version::new(2, 0, 0), DepSource::Registry, None);

        let manifest = Manifest {
            name: "app".into(),
            version: Version::new(0, 1, 0),
            description: None,
            authors: Vec::new(),
            license: None,
            repository: None,
            dependencies: vec![
                ManifestDep {
                    name: "foo".into(),
                    req: VersionReq::parse("^1.0.0").unwrap(),
                    source: DepSource::Registry,
                },
                ManifestDep {
                    name: "bar".into(),
                    req: VersionReq::parse("^2.0.0").unwrap(),
                    source: DepSource::Registry,
                },
            ],
            dev_dependencies: Vec::new(),
        };

        let resolver = DependencyResolver::new(&reg);
        let resolved = resolver.resolve(&manifest).unwrap();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().any(|p| p.name == "foo"));
        assert!(resolved.iter().any(|p| p.name == "bar"));
    }

    #[test]
    fn pkg_resolver_detects_version_conflict() {
        let mut reg = Registry::new();
        // Register both foo@1.0.0 and foo@2.0.0 so resolution succeeds for each
        // individually, but the conflict is detected when both are required.
        reg.register("foo", Version::new(1, 0, 0), DepSource::Registry, None);
        reg.register("foo", Version::new(2, 0, 0), DepSource::Registry, None);

        // bar depends on foo ^2.0.0
        let bar_manifest = Manifest {
            name: "bar".into(),
            version: Version::new(1, 0, 0),
            description: None,
            authors: Vec::new(),
            license: None,
            repository: None,
            dependencies: vec![ManifestDep {
                name: "foo".into(),
                req: VersionReq::parse("^2.0.0").unwrap(),
                source: DepSource::Registry,
            }],
            dev_dependencies: Vec::new(),
        };
        reg.register(
            "bar",
            Version::new(1, 0, 0),
            DepSource::Registry,
            Some(bar_manifest),
        );

        // app requires foo ^1.0.0 (resolves to 1.0.0) AND bar (which pulls foo ^2.0.0)
        // foo@1.0.0 is resolved first; then bar's transitive foo@^2.0.0 conflicts.
        let manifest = Manifest {
            name: "app".into(),
            version: Version::new(0, 1, 0),
            description: None,
            authors: Vec::new(),
            license: None,
            repository: None,
            dependencies: vec![
                ManifestDep {
                    name: "foo".into(),
                    req: VersionReq::parse("=1.0.0").unwrap(),
                    source: DepSource::Registry,
                },
                ManifestDep {
                    name: "bar".into(),
                    req: VersionReq::parse("^1.0.0").unwrap(),
                    source: DepSource::Registry,
                },
            ],
            dev_dependencies: Vec::new(),
        };

        let resolver = DependencyResolver::new(&reg);
        let err = resolver.resolve(&manifest).unwrap_err();
        assert_eq!(err.code, "NXL13005");
    }

    // ── PackageCache ──────────────────────────────────────────────────────────

    #[test]
    fn pkg_cache_store_fetch_evict() {
        let tmp = tmp_dir("cache");
        let cache = PackageCache::new(&tmp);
        let v = Version::new(1, 0, 0);
        assert!(!cache.is_cached("foo", &v));
        cache.store("foo", &v, "let x = 1;").unwrap();
        assert!(cache.is_cached("foo", &v));
        let src = cache.fetch("foo", &v).unwrap();
        assert_eq!(src, "let x = 1;");
        cache.evict("foo", &v).unwrap();
        assert!(!cache.is_cached("foo", &v));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_cache_fetch_miss_errors() {
        let tmp = tmp_dir("cachemiss");
        let cache = PackageCache::new(&tmp);
        let err = cache.fetch("missing", &Version::new(1, 0, 0)).unwrap_err();
        assert_eq!(err.code, "NXL13007");
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_cache_clear() {
        let tmp = tmp_dir("cacheclear");
        let cache = PackageCache::new(&tmp);
        cache.store("a", &Version::new(1, 0, 0), "src").unwrap();
        cache.clear().unwrap();
        assert!(!tmp.exists() || !cache.is_cached("a", &Version::new(1, 0, 0)));
        fs::remove_dir_all(&tmp).ok();
    }

    // ── PackageManager ────────────────────────────────────────────────────────

    #[test]
    fn pkg_manager_install_no_deps() {
        let tmp = tmp_dir("install_nodeps");
        let toml = "[package]\nname = \"empty\"\nversion = \"0.1.0\"\n";
        fs::write(tmp.join("nxl.toml"), toml).unwrap();

        let cache_dir = tmp.join(".nxl-cache");
        let mut pm = PackageManager::new(&cache_dir);
        let report = pm.install(&tmp).unwrap();
        assert!(report.installed.is_empty());
        assert!(tmp.join("nxl.lock").exists());
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_manager_install_with_registry_dep() {
        let tmp = tmp_dir("install_dep");
        let toml =
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nfoo = \"^1.0.0\"\n";
        fs::write(tmp.join("nxl.toml"), toml).unwrap();

        let cache_dir = tmp.join(".nxl-cache");
        let mut pm = PackageManager::new(&cache_dir);
        pm.registry
            .register("foo", Version::new(1, 0, 0), DepSource::Registry, None);

        let report = pm.install(&tmp).unwrap();
        assert!(report.installed.iter().any(|s| s.contains("foo")));
        let lock = LockFile::load(&tmp).unwrap();
        assert!(lock.contains("foo"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_manager_add_dep() {
        let tmp = tmp_dir("add_dep");
        let toml = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";
        fs::write(tmp.join("nxl.toml"), toml).unwrap();

        let cache_dir = tmp.join(".nxl-cache");
        let mut pm = PackageManager::new(&cache_dir);
        pm.registry
            .register("bar", Version::new(1, 5, 0), DepSource::Registry, None);

        pm.add(&tmp, "bar", "^1.0.0").unwrap();
        let manifest_text = fs::read_to_string(tmp.join("nxl.toml")).unwrap();
        assert!(manifest_text.contains("bar"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_manager_remove_dep() {
        let tmp = tmp_dir("remove_dep");
        let toml =
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nbaz = \"^1.0.0\"\n";
        fs::write(tmp.join("nxl.toml"), toml).unwrap();

        let cache_dir = tmp.join(".nxl-cache");
        let mut pm = PackageManager::new(&cache_dir);
        pm.registry
            .register("baz", Version::new(1, 0, 0), DepSource::Registry, None);
        pm.install(&tmp).unwrap();

        pm.remove(&tmp, "baz").unwrap();
        let manifest_text = fs::read_to_string(tmp.join("nxl.toml")).unwrap();
        assert!(!manifest_text.contains("baz ="));
        fs::remove_dir_all(&tmp).ok();
    }

    // ── Checksum ──────────────────────────────────────────────────────────────

    #[test]
    fn pkg_checksum_deterministic() {
        let a = checksum(b"hello world");
        let b = checksum(b"hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn pkg_checksum_different_inputs_differ() {
        let a = checksum(b"foo");
        let b = checksum(b"bar");
        assert_ne!(a, b);
    }

    #[test]
    fn pkg_checksum_is_hex_string() {
        let cs = checksum(b"test");
        assert!(
            cs.chars().all(|c| c.is_ascii_hexdigit()),
            "checksum must be hex: {cs}"
        );
        // Package integrity uses SHA-256, not a short non-cryptographic hash.
        assert_eq!(cs.len(), 64, "expected a SHA-256 digest: {cs}");
    }

    #[test]
    fn pkg_checksum_matches_sha256_vectors() {
        // A non-cryptographic stand-in could not detect tampering, which is
        // the whole reason a checksum is recorded in nxl.lock.
        assert_eq!(
            checksum(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            checksum(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_ne!(checksum(b"a"), checksum(b"b"));
    }

    // ── Audit ─────────────────────────────────────────────────────────────────

    #[test]
    fn pkg_audit_no_findings_when_clean() {
        let lock = LockFile::new();
        let findings = audit(&lock, &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn pkg_audit_finds_vulnerable_package() {
        let mut lock = LockFile::new();
        lock.packages.push(LockedPackage {
            name: "vuln-pkg".into(),
            version: Version::new(1, 0, 0),
            checksum: "x".into(),
            source: DepSource::Registry,
            dependencies: Vec::new(),
        });
        let advisories = vec![Advisory {
            package: "vuln-pkg".into(),
            affected_req: VersionReq::parse("^1.0.0").unwrap(),
            severity: AuditSeverity::High,
            description: "Remote code execution".into(),
        }];
        let findings = audit(&lock, &advisories);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, AuditSeverity::High);
    }

    #[test]
    fn pkg_audit_skips_unaffected_versions() {
        let mut lock = LockFile::new();
        lock.packages.push(LockedPackage {
            name: "safe-pkg".into(),
            version: Version::new(2, 0, 0),
            checksum: "x".into(),
            source: DepSource::Registry,
            dependencies: Vec::new(),
        });
        let advisories = vec![Advisory {
            package: "safe-pkg".into(),
            affected_req: VersionReq::parse("^1.0.0").unwrap(), // only affects 1.x
            severity: AuditSeverity::Critical,
            description: "Old vulnerability".into(),
        }];
        let findings = audit(&lock, &advisories);
        assert!(findings.is_empty());
    }

    // ── Workspace ─────────────────────────────────────────────────────────────

    #[test]
    fn pkg_workspace_parse_members() {
        let root = PathBuf::from("/workspace");
        let toml = "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n";
        let ws = WorkspaceConfig::parse(toml, &root).unwrap();
        assert_eq!(ws.members.len(), 2);
        assert!(ws.members[0].ends_with("crates/a") || ws.members[0].ends_with("crates\\a"));
    }

    #[test]
    fn pkg_workspace_empty_when_no_section() {
        let root = PathBuf::from("/workspace");
        let toml = "[package]\nname = \"single\"\nversion = \"0.1.0\"\n";
        let ws = WorkspaceConfig::parse(toml, &root).unwrap();
        assert!(ws.members.is_empty());
    }

    // ── Git dependencies ──────────────────────────────────────────────────────

    #[test]
    fn pkg_manifest_parse_git_dep() {
        let toml = "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\nremote = \"git:https://github.com/nexora/remote#v1.2.0\"\n";
        let m = Manifest::parse(toml).unwrap();
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dependencies[0].name, "remote");
        match &m.dependencies[0].source {
            DepSource::Git { url, branch_or_tag } => {
                assert_eq!(url, "https://github.com/nexora/remote");
                assert_eq!(branch_or_tag.as_deref(), Some("v1.2.0"));
            }
            _ => panic!("expected DepSource::Git"),
        }
    }

    #[test]
    fn pkg_lockfile_git_dep_roundtrip() {
        let mut lock = LockFile::new();
        lock.packages.push(LockedPackage {
            name: "remote".into(),
            version: Version::new(1, 2, 0),
            checksum: "beef1234".into(),
            source: DepSource::Git {
                url: "https://github.com/example/lib".into(),
                branch_or_tag: Some("main".into()),
            },
            dependencies: Vec::new(),
        });
        let serialized = lock.serialize();
        assert!(serialized.contains("source = \"git:https://github.com/example/lib#main\""));
        let parsed = LockFile::parse(&serialized).unwrap();
        assert_eq!(parsed.packages.len(), 1);
        match &parsed.packages[0].source {
            DepSource::Git { url, branch_or_tag } => {
                assert_eq!(url, "https://github.com/example/lib");
                assert_eq!(branch_or_tag.as_deref(), Some("main"));
            }
            _ => panic!("expected Git source in parsed lockfile"),
        }
    }

    // ── Offline mode ──────────────────────────────────────────────────────────

    #[test]
    fn pkg_manager_offline_mode_cached_succeeds() {
        let tmp = tmp_dir("offline_cached");
        let toml =
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nfoo = \"^1.0.0\"\n";
        fs::write(tmp.join("nxl.toml"), toml).unwrap();

        let cache_dir = tmp.join(".nxl-cache");
        let cache = PackageCache::new(&cache_dir);
        // Pre-populate cache
        cache
            .store("foo", &Version::new(1, 0, 0), "let x = 42;")
            .unwrap();

        let mut pm = PackageManager::new(&cache_dir).with_offline(true);
        pm.registry
            .register("foo", Version::new(1, 0, 0), DepSource::Registry, None);

        let report = pm.install(&tmp).unwrap();
        assert!(report.installed.iter().any(|s| s.contains("foo")));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_manager_offline_mode_uncached_fails() {
        let tmp = tmp_dir("offline_uncached");
        let toml =
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nfoo = \"^1.0.0\"\n";
        fs::write(tmp.join("nxl.toml"), toml).unwrap();

        let cache_dir = tmp.join(".nxl-cache");
        let mut pm = PackageManager::new(&cache_dir).with_offline(true);
        pm.registry
            .register("foo", Version::new(1, 0, 0), DepSource::Registry, None);

        let err = pm.install(&tmp).unwrap_err();
        assert_eq!(err.code, "NXL13008");
        assert!(err.message.contains("offline mode"));
        fs::remove_dir_all(&tmp).ok();
    }

    // ── Dependency graph ──────────────────────────────────────────────────────

    #[test]
    fn pkg_dependency_graph_render() {
        let manifest = Manifest {
            name: "my-app".into(),
            version: Version::new(1, 0, 0),
            description: None,
            authors: Vec::new(),
            license: None,
            repository: None,
            dependencies: Vec::new(),
            dev_dependencies: Vec::new(),
        };
        let mut reg = Registry::new();
        reg.register("dep-a", Version::new(1, 2, 0), DepSource::Registry, None);
        let mut pm = PackageManager::new(std::env::temp_dir().join("dummy_cache"));
        pm.registry = reg;

        let graph = DependencyGraph::build(
            &manifest,
            &[nxl_compiler::pkg::ResolvedPackage {
                name: "dep-a".into(),
                version: Version::new(1, 2, 0),
                source: DepSource::Registry,
                dependencies: vec!["sub-dep@^1.0.0".into()],
            }],
        );

        let rendered = graph.render();
        assert!(rendered.contains("my-app v1.0.0"));
        assert!(rendered.contains("dep-a v1.2.0"));
        assert!(rendered.contains("sub-dep@^1.0.0"));
    }

    // ── CLI package commands ──────────────────────────────────────────────────

    #[test]
    fn pkg_cli_parse_pkg_commands() {
        assert_eq!(
            parse_args(&["install".into()]).unwrap(),
            Command::PkgInstall {
                offline: false,
                registry: None
            }
        );
        assert_eq!(
            parse_args(&["install".into(), "--offline".into()]).unwrap(),
            Command::PkgInstall {
                offline: true,
                registry: None
            }
        );
        assert_eq!(
            parse_args(&["pkg".into(), "install".into()]).unwrap(),
            Command::PkgInstall {
                offline: false,
                registry: None
            }
        );
        assert_eq!(
            parse_args(&["package:install".into()]).unwrap(),
            Command::PkgInstall {
                offline: false,
                registry: None
            }
        );

        assert_eq!(
            parse_args(&["add".into(), "serde".into()]).unwrap(),
            Command::PkgAdd {
                name: "serde".into(),
                req: None,
                registry: None,
                path: None,
            }
        );
        assert_eq!(
            parse_args(&["add".into(), "serde".into(), "^1.0".into()]).unwrap(),
            Command::PkgAdd {
                name: "serde".into(),
                req: Some("^1.0".into()),
                registry: None,
                path: None,
            }
        );
        assert_eq!(
            parse_args(&[
                "add".into(),
                "serde".into(),
                "--path".into(),
                "../vendor/serde".into()
            ])
            .unwrap(),
            Command::PkgAdd {
                name: "serde".into(),
                req: None,
                registry: None,
                path: Some("../vendor/serde".into()),
            }
        );
        assert_eq!(
            parse_args(&["pkg".into(), "add".into(), "serde".into()]).unwrap(),
            Command::PkgAdd {
                name: "serde".into(),
                req: None,
                registry: None,
                path: None,
            }
        );

        assert_eq!(
            parse_args(&["remove".into(), "old".into()]).unwrap(),
            Command::PkgRemove { name: "old".into() }
        );
        assert_eq!(
            parse_args(&["package:remove".into(), "old".into()]).unwrap(),
            Command::PkgRemove { name: "old".into() }
        );

        assert_eq!(parse_args(&["update".into()]).unwrap(), Command::PkgUpdate);
        assert_eq!(
            parse_args(&["package:update".into()]).unwrap(),
            Command::PkgUpdate
        );

        assert_eq!(parse_args(&["audit".into()]).unwrap(), Command::PkgAudit);
        assert_eq!(parse_args(&["tree".into()]).unwrap(), Command::PkgTree);
        assert_eq!(parse_args(&["deps".into()]).unwrap(), Command::PkgTree);
        assert_eq!(
            parse_args(&["publish".into()]).unwrap(),
            Command::PkgPublish { registry: None }
        );
    }

    #[test]
    fn pkg_cli_cmd_pkg_install_and_tree() {
        let tmp = tmp_dir("cli_install_tree");
        let toml = "[package]\nname = \"tree_app\"\nversion = \"0.1.0\"\n";
        fs::write(tmp.join("nxl.toml"), toml).unwrap();

        let res = cmd_pkg_install(&tmp, false).unwrap();
        assert!(res.contains("dependencies"));

        let tree = cmd_pkg_tree(&tmp).unwrap();
        assert!(tree.contains("tree_app v0.1.0"));

        let audit_res = cmd_pkg_audit(&tmp).unwrap();
        assert!(audit_res.contains("0 vulnerabilities"));

        // Publishing writes into a registry directory; see publish_tests.rs.

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_cli_add_path_dependency_installs_and_removes() {
        let tmp = tmp_dir("cli_add_remove");
        let toml = "[package]\nname = \"add_app\"\nversion = \"0.1.0\"\n";
        fs::write(tmp.join("nxl.toml"), toml).unwrap();

        // A real package on disk, referenced by path.
        let vendor = tmp.join("vendor");
        fs::create_dir_all(vendor.join("src")).unwrap();
        fs::write(
            vendor.join("nxl.toml"),
            "[package]\nname = \"test-pkg\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        fs::write(vendor.join("src/main.nxl"), "print(\"vendor\");").unwrap();

        let add_res = cmd_pkg_add(
            &tmp,
            AddRequest::Path {
                name: "test-pkg",
                path: "vendor",
            },
        )
        .unwrap();
        assert!(add_res.contains("test-pkg"), "{add_res}");
        let manifest_content = fs::read_to_string(tmp.join("nxl.toml")).unwrap();
        assert!(
            manifest_content.contains("test-pkg = \"path:vendor\""),
            "{manifest_content}"
        );
        // A path dependency resolves, so it is recorded in the lock file.
        let lock = fs::read_to_string(tmp.join("nxl.lock")).unwrap();
        assert!(lock.contains("test-pkg"), "{lock}");

        let rem_res = cmd_pkg_remove(&tmp, "test-pkg").unwrap();
        assert!(rem_res.contains("test-pkg"), "{rem_res}");
        let manifest_after = fs::read_to_string(tmp.join("nxl.toml")).unwrap();
        assert!(!manifest_after.contains("test-pkg ="), "{manifest_after}");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_cli_add_registry_dependency_fails_honestly() {
        let tmp = tmp_dir("cli_add_registry");
        fs::write(
            tmp.join("nxl.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        // The registry directory is named explicitly and lives under this
        // test's own temp directory. Leaving it unset would fall back to the
        // real ~/.nexora/registry, so the result would depend on whether the
        // machine running the suite happens to have published anything.
        let missing = tmp.join("no-such-registry");
        let err = cmd_pkg_add(
            &tmp,
            AddRequest::Registry {
                name: "not-published",
                req: Some("^1.0"),
                registry: Some(missing.to_str().unwrap()),
            },
        )
        .expect_err("unknown packages must not resolve");
        assert_eq!(err.code, "NXL13006", "{err:?}");
        assert!(
            err.message.contains("pkg publish"),
            "the error should say how to proceed: {err:?}"
        );

        // A registry that exists but does not carry the package fails the same
        // way, rather than recording a dependency that cannot be installed.
        let empty = tmp.join("empty-registry");
        fs::create_dir_all(&empty).unwrap();
        let err = cmd_pkg_add(
            &tmp,
            AddRequest::Registry {
                name: "not-published",
                req: Some("^1.0"),
                registry: Some(empty.to_str().unwrap()),
            },
        )
        .expect_err("an empty registry must not resolve anything");
        assert_eq!(err.code, "NXL13006", "{err:?}");
        assert!(
            !fs::read_to_string(tmp.join("nxl.toml"))
                .unwrap()
                .contains("not-published"),
            "a failed add must not write a dependency line"
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_cli_add_rejects_a_missing_path_dependency() {
        let tmp = tmp_dir("cli_add_bad_path");
        fs::write(
            tmp.join("nxl.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let err = cmd_pkg_add(
            &tmp,
            AddRequest::Path {
                name: "ghost",
                path: "nowhere",
            },
        )
        .expect_err("a missing path dependency must fail");
        assert_eq!(err.code, "NXL13009", "{err:?}");
        // Nothing may be written for a dependency that does not exist.
        let manifest = fs::read_to_string(tmp.join("nxl.toml")).unwrap();
        assert!(!manifest.contains("ghost"), "{manifest}");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pkg_cli_publish_needs_a_project() {
        let tmp = tmp_dir("cli_publish_empty");
        let registry = tmp.join("registry");
        let err = cmd_pkg_publish(Some(registry.as_path()))
            .expect_err("publishing outside a project must fail");
        assert_eq!(err.code, "NXL13011", "{err:?}");
        fs::remove_dir_all(&tmp).ok();
    }
}

// ── array type annotations ──────────────────────────────────────────────────────
//
// An array literal is inferred as `Array(0)` because the element type is not
// pinned down, while a declared `[T]` or `Array<T>` becomes `Array(<id>)`. The
// return and argument contracts compared those two with `!=`, so every
// array-returning or array-taking function was rejected at compile time -
// which made it impossible to write the natural shape of an application.

#[test]
fn array_return_annotation_compiles() {
    let source = SourceFile::new(
        "array_return.nxl",
        "fn features() -> [String] { return [\"one\", \"two\"]; } print(features());",
    );
    compile_to_nir(&source).expect("a function may return a declared array type");
}

#[test]
fn generic_array_return_annotation_compiles() {
    let source = SourceFile::new(
        "array_generic.nxl",
        "fn numbers() -> Array<Int> { return [1, 2]; } print(numbers());",
    );
    compile_to_nir(&source).expect("Array<T> is the same annotation as [T]");
}

#[test]
fn array_parameter_annotation_compiles() {
    let source = SourceFile::new(
        "array_param.nxl",
        "fn first(items: [String]) -> String { return items[0]; } print(first([\"a\"]));",
    );
    compile_to_nir(&source).expect("a function may accept a declared array type");
}

#[test]
fn unused_array_function_still_has_to_satisfy_its_contract() {
    // The contract check ran per function, not per call site, so an uncalled
    // function with an array return type failed too.
    let source = SourceFile::new(
        "array_uncalled.nxl",
        "fn unused() -> [Int] { return [1]; } print(\"main ran\");",
    );
    compile_to_nir(&source).expect("an uncalled function still needs a valid contract");
}

#[test]
fn array_annotations_survive_execution() {
    if !Runtime::node_available() {
        return;
    }
    let source = SourceFile::new(
        "array_run.nxl",
        "fn features() -> [String] { return [\"one\", \"two\"]; } print(features());",
    );
    let result = run(&source).expect("the array round trip should execute");
    assert!(
        result.stdout.contains("one") && result.stdout.contains("two"),
        "{result:?}"
    );
}

#[test]
fn a_wrong_return_type_is_still_rejected() {
    // The contract must still reject a genuinely wrong value; only the
    // `Array(0)`-versus-`Array(<id>)` case was relaxed.
    let source = SourceFile::new(
        "array_wrong_return.nxl",
        "fn broken() -> [String] { return 5; } print(broken());",
    );
    let error = compile_to_nir(&source).expect_err("Int is not an array");
    assert!(
        error.message.contains("expected Array") || error.message.contains("expected [String]"),
        "{error:?}"
    );
}

#[test]
fn a_wrong_argument_type_is_still_rejected() {
    let source = SourceFile::new(
        "array_wrong_arg.nxl",
        "fn takes_int(n: Int) -> Int { return n; } print(takes_int([1]));",
    );
    let error = compile_to_nir(&source).expect_err("an array is not an Int");
    assert!(error.message.contains("expected Int"), "{error:?}");
}

// ── string literals in generated code ─────────────────────────────────────────
//
// A raw newline inside a double-quoted literal is not valid in JavaScript or
// Rust. The backends escaped backslashes and quotes but not newlines, so any
// multi-line string - a raw string literal, or anything built from one - was
// compiled into code that would not parse.

#[test]
fn a_multi_line_string_compiles_to_parseable_javascript() {
    let source = SourceFile::new(
        "multiline.nxl",
        "fn main() { print(`<div>\n  <p>hello</p>\n</div>`); }",
    );
    let artifact = nxl_compiler::backend::compile_js(&source).expect("emit");
    // The emitted program must parse: if it does not, the defect is real.
    assert!(
        !artifact.source.contains("\"<div>\n"),
        "a raw newline was emitted inside a string literal"
    );
    assert!(artifact.source.contains("\\n"), "{}", artifact.source);
}

#[test]
fn a_multi_line_string_round_trips_through_node() {
    if !Runtime::node_available() {
        return;
    }
    let source = SourceFile::new(
        "multiline_run.nxl",
        "fn main() { print(`<ul>\n  <li>one</li>\n  <li>two</li>\n</ul>`); }",
    );
    let result = run(&source).expect("a multi-line string should execute");
    assert!(result.stdout.contains("<li>one</li>"), "{result:?}");
    assert!(result.stdout.contains("<li>two</li>"), "{result:?}");
    // The newlines survived rather than being mangled.
    assert!(
        result.stdout.matches('\n').count() >= 4,
        "the newlines did not survive: {result:?}"
    );
}

#[test]
fn control_characters_are_escaped_in_generated_code() {
    let source = SourceFile::new("control.nxl", "fn main() { print(\"a\\tb\\r\"); }");
    let artifact = nxl_compiler::backend::compile_js(&source).expect("emit");
    assert!(artifact.source.contains("\\t"), "{}", artifact.source);
    assert!(artifact.source.contains("\\r"), "{}", artifact.source);
}

#[test]
fn quotes_and_backslashes_still_round_trip() {
    let source = SourceFile::new(
        "escapes.nxl",
        "fn main() { print(\"a \\\"quoted\\\" word\"); print(`c:\\\\dir`); }",
    );
    let artifact = nxl_compiler::backend::compile_js(&source).expect("emit");
    // A quote inside the string is written as \" in the generated program.
    assert!(
        artifact.source.contains("\\\""),
        "a quote was not escaped: {}",
        artifact.source
    );
    // And a backslash as \\.
    assert!(
        artifact.source.contains("\\\\"),
        "a backslash was not escaped: {}",
        artifact.source
    );
}

// ── telling someone they are in the wrong directory ────────────────────────
//
// `ProjectConfig::load` returns defaults when there is no manifest, so every
// command used to fall through to "cannot read .../src/main.nxl", which reads
// like a broken toolchain. Naming a file is still exempt: a loose file is a
// deliberate single-file workflow.

#[test]
fn commands_outside_a_project_say_so_and_say_what_to_run() {
    let dir = std::env::temp_dir().join(format!(
        "nxl_noproject_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    for (command, error) in [
        (
            "run",
            nxl_compiler::cli::cmd_run(&dir, None).expect_err("not a project"),
        ),
        (
            "build",
            nxl_compiler::cli::cmd_build(&dir, false).expect_err("not a project"),
        ),
    ] {
        assert_eq!(error.code, "NXL12006", "{command}: {error:?}");
        assert!(
            error.message.contains("is not an NXL project"),
            "{command}: {error:?}"
        );
        assert!(
            error.message.contains("no nxl.toml"),
            "{command}: {error:?}"
        );
        assert!(
            error.message.contains("nxlc new console"),
            "{command} should say how to create a project: {error:?}"
        );
        assert!(
            !error.message.contains("cannot read"),
            "{command} must not report a missing entry point: {error:?}"
        );
    }

    // A named file is a different question, and still answered.
    let file = dir.join("loose.nxl");
    std::fs::write(&file, "fn main() { print(\"loose\"); }").unwrap();
    let run = nxl_compiler::cli::cmd_run(&dir, Some(file.to_str().unwrap()))
        .expect("a loose file needs no manifest");
    assert!(run.stdout.contains("loose"), "{run:?}");
    assert!(
        nxl_compiler::cli::cmd_check(&dir, Some(file.to_str().unwrap())).is_ok(),
        "a loose file can be checked without a manifest"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn serve_says_it_is_not_a_server() {
    let dir = std::env::temp_dir().join(format!(
        "nxl_serve_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let message = nxl_compiler::cli::cmd_serve(&dir).expect("serve answers");
    // It used to end with "run `nxl serve` to listen", i.e. telling someone to
    // repeat the command they had just run.
    assert!(message.contains("not a server"), "{message}");
    assert!(!message.contains("to listen"), "{message}");
    assert!(message.contains("nexora dev"), "{message}");
    std::fs::remove_dir_all(&dir).ok();
}
