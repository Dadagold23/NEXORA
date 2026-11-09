//! Module 2.1 baseline for the NXL type system.
//!
//! These tests record what the compiler actually does before the type-system
//! work in Modules 2.2-2.5. Positive cases lock in behaviour that must keep
//! working. The `baseline_*_limitation` cases document current gaps that those
//! modules are expected to change; they are named so the flip is deliberate.

use nxl_compiler::{
    compile_to_nir,
    nir::{Instruction, Type},
    source::SourceFile,
};

fn compile(name: &str, source: &str) -> Result<nxl_compiler::nir::Module, String> {
    compile_to_nir(&SourceFile::new(name, source)).map_err(|diagnostic| diagnostic.code)
}

fn compile_ok(name: &str, source: &str) -> nxl_compiler::nir::Module {
    compile_to_nir(&SourceFile::new(name, source)).unwrap_or_else(|diagnostic| {
        panic!(
            "{name} should lower, got {}: {}",
            diagnostic.code, diagnostic.message
        )
    })
}

// ---------------------------------------------------------------------------
// Primitive types
// ---------------------------------------------------------------------------

#[test]
fn baseline_primitive_annotations_are_preserved() {
    let module = compile_ok(
        "primitives.nxl",
        r#"
            fn sample(a: Int, b: Float, c: Bool, d: String) -> Bool {
                return c
            }
            print(sample(1, 1.5, true, "text"))
        "#,
    );
    let signature = module
        .signatures
        .iter()
        .find(|signature| signature.name == "sample")
        .expect("sample signature");
    let types = signature
        .parameters
        .iter()
        .map(|(_, ty)| *ty)
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        vec![Type::Int, Type::Float, Type::Bool, Type::String]
    );
    assert_eq!(signature.return_type, Type::Bool);
    module.verify().expect("primitive program must verify");
}

#[test]
fn baseline_no_implicit_numeric_conversion() {
    assert_eq!(
        compile("int_to_float.nxl", "let x: Float = 1;"),
        Err("NXL2001".into())
    );
    assert_eq!(
        compile("float_to_int.nxl", "let x: Int = 1.5;"),
        Err("NXL2001".into())
    );
    assert_eq!(
        compile("mixed_numeric.nxl", "let x = 1 + 2.5;"),
        Err("NXL2001".into())
    );
}

#[test]
fn baseline_incompatible_operands_are_rejected() {
    assert_eq!(
        compile("int_plus_string.nxl", r#"let x = 1 + "a";"#),
        Err("NXL2001".into())
    );
    assert_eq!(
        compile("bool_int_eq.nxl", "let x = true == 1;"),
        Err("NXL2001".into())
    );
}

// ---------------------------------------------------------------------------
// Function arguments and return values
// ---------------------------------------------------------------------------

#[test]
fn baseline_function_argument_types_are_enforced() {
    let valid = r#"
        fn add_one(x: Int) -> Int { return x + 1; }
        print(add_one(1));
    "#;
    compile_ok("arg_ok.nxl", valid);

    assert_eq!(
        compile(
            "arg_bad_type.nxl",
            r#"fn f(x: Int) -> Int { return x; } print(f("s"));"#
        ),
        Err("NXL2001".into())
    );
    assert_eq!(
        compile(
            "arg_bad_arity.nxl",
            "fn f(x: Int) -> Int { return x; } print(f(1, 2));"
        ),
        Err("NXL2009".into())
    );
}

#[test]
fn baseline_return_types_are_enforced() {
    assert_eq!(
        compile(
            "return_bad.nxl",
            r#"fn f() -> Int { return "s"; } print(f());"#
        ),
        Err("NXL2001".into())
    );
    assert_eq!(
        compile("return_missing.nxl", "fn f() -> Int { let x = 1; }"),
        Err("NXL2001".into())
    );
    let module = compile_ok("return_ok.nxl", "fn f() -> Int { return 1; } print(f());");
    module.verify().expect("valid return must verify");
}

// ---------------------------------------------------------------------------
// Structs and enums
// ---------------------------------------------------------------------------

#[test]
fn baseline_struct_construction_and_field_types() {
    let module = compile_ok(
        "struct_ok.nxl",
        r#"
            struct Point {
                x: Int
                y: Int
            }
            let point = Point { x: 1, y: 2 };
            print(point.x + point.y);
        "#,
    );
    module.verify().expect("struct program must verify");

    assert_eq!(
        compile(
            "struct_missing_field.nxl",
            r#"
                struct Point { x: Int, y: Int }
                let point = Point { x: 1 };
                print(point.x);
            "#
        ),
        Err("NXL1001".into()),
        "the parser needs newline-separated fields, so this malformed form is a parse error"
    );

    assert_eq!(
        compile(
            "struct_bad_field.nxl",
            r#"
                struct Point {
                    x: Int
                }
                let point = Point { x: "s" };
                print(point.x);
            "#
        ),
        Err("NXL2001".into())
    );
}

#[test]
fn baseline_struct_literal_missing_field_is_rejected() {
    assert_eq!(
        compile(
            "struct_missing_wellformed.nxl",
            r#"
                struct Point {
                    x: Int
                    y: Int
                }
                let point = Point { x: 1 };
                print(point.x);
            "#
        ),
        Err("NXL2001".into())
    );
}

#[test]
fn baseline_enum_variant_arity_is_enforced() {
    let module = compile_ok(
        "enum_ok.nxl",
        "enum Choice { None, Some(Int) } let value = Choice.Some(2); print(value);",
    );
    module.verify().expect("enum program must verify");
    assert_eq!(
        compile(
            "enum_bad_arity.nxl",
            "enum Choice { None, Some(Int) } let value = Choice.Some(); print(value);"
        ),
        Err("NXL2001".into())
    );
}

// ---------------------------------------------------------------------------
// Arrays and maps
// ---------------------------------------------------------------------------

#[test]
fn baseline_array_literals_and_indexing() {
    let module = compile_ok("array_ok.nxl", "let values = [1, 2, 3]; print(values[0]);");
    module.verify().expect("array program must verify");
    assert_eq!(
        compile("array_mixed.nxl", r#"let values = [1, "s"];"#),
        Err("NXL2001".into())
    );
    assert_eq!(
        compile(
            "array_immutable_index.nxl",
            "let values = [1, 2]; values[0] = 3;"
        ),
        Err("NXL2008".into())
    );
}

#[test]
fn baseline_generic_container_annotations_resolve() {
    // Map<...>, Option<...>, Task<...> and Result<...> are recognised as type
    // annotations today; construction/typing of their values is incomplete.
    assert_eq!(
        compile(
            "map_param_bad_arg.nxl",
            "fn f(m: Map<String, Int>) -> Int { return 1; } print(f(1));"
        ),
        Err("NXL2001".into())
    );
    assert_eq!(
        compile(
            "option_param_bad_arg.nxl",
            "fn f(x: Option<Int>) -> Int { return 1; } print(f(1));"
        ),
        Err("NXL2001".into())
    );
    assert_eq!(
        compile(
            "result_bad_return.nxl",
            "fn f() -> Result<Int, String> { return 1; } print(f());"
        ),
        Err("NXL2001".into())
    );
}

#[test]
fn baseline_map_and_set_literals_are_not_lowered_yet() {
    // Parsed by the grammar, rejected by the lowerer. This is the gap Module
    // 2.x must close (or document as intentionally unsupported).
    assert_eq!(
        compile("map_literal.nxl", r#"let m = {"a": 1};"#),
        Err("NXL2007".into())
    );
    assert_eq!(
        compile("set_literal.nxl", "let s = {1, 2};"),
        Err("NXL2007".into())
    );
}

// ---------------------------------------------------------------------------
// References, ownership and mutability
// ---------------------------------------------------------------------------

#[test]
fn baseline_reference_expressions_are_now_supported() {
    // Module 2.4 replaced the old "invalid unary operator" behaviour with an
    // explicit lexical borrow model. Referencing requires `*` to read (a
    // reference is never implicitly dereferenced) and reference annotations
    // parse, including `&mut T`.
    compile_ok("ref_expr.nxl", "let x = 1; let r = ref x; print(*r);");
    compile_ok("borrow_expr.nxl", "let x = 1; let r = borrow x; print(*r);");
    compile_ok(
        "ref_annotation.nxl",
        "fn f(x: &Int) -> Int { return *x; } let n = 2; print(f(borrow n));",
    );
    compile_ok(
        "ref_mut_annotation.nxl",
        "fn bump(x: &mut Int) { *x = *x + 1; } let mut n = 2; { bump(borrow mut n); } print(n);",
    );
    // A reference still cannot escape the borrow scope.
    assert_eq!(
        compile(
            "ref_return.nxl",
            "fn f(x: &Int) -> &Int { return x; } let n = 2; print(*f(borrow n));"
        ),
        Err("NXL2017".into())
    );
}

#[test]
fn baseline_mutability_is_enforced_for_bindings() {
    compile_ok("mutable_ok.nxl", "let mut x = 1; x = 2; print(x);");
    assert_eq!(
        compile("immutable_bad.nxl", "let x = 1; x = 2; print(x);"),
        Err("NXL2008".into())
    );
    assert_eq!(
        compile("undeclared_assign.nxl", "x = 2;"),
        Err("NXL2002".into())
    );
}

// ---------------------------------------------------------------------------
// Inference
// ---------------------------------------------------------------------------

#[test]
fn baseline_local_inference_propagates_expression_types() {
    let module = compile_ok("infer.nxl", "let x = 1; let y = x + 2; print(y);");
    module.verify().expect("inferred program must verify");
    let binary = module.functions[0]
        .basic_blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::Binary { ty, .. } => Some(*ty),
            _ => None,
        })
        .expect("x + 2 should lower to a binary instruction");
    assert_eq!(binary, Type::Int, "integer operands infer an Int result");
}

#[test]
fn baseline_unannotated_return_is_inferred() {
    // Module 2.3 infers the result type from the returned expressions.
    let module = compile_ok("return_infer.nxl", "fn f() { return 1; } print(f());");
    let signature = module
        .signatures
        .iter()
        .find(|signature| signature.name == "f")
        .expect("f signature");
    assert_eq!(signature.return_type, Type::Int);
    let call_return = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::Call {
                callee,
                return_type,
                ..
            } if callee == "f" => Some(*return_type),
            _ => None,
        })
        .expect("call to f");
    assert_eq!(call_return, Type::Int);
    module.verify().expect("inferred return must verify");

    // A caller is now held to the inferred type ...
    assert_eq!(
        compile(
            "return_typed.nxl",
            "fn f() { return 1; } let s: String = f(); print(s);"
        ),
        Err("NXL2001".into())
    );
    // ... and conflicting returns are rejected rather than inferred as dynamic.
    assert_eq!(
        compile(
            "return_conflict.nxl",
            "fn f(c: Bool) { if c { return 1; } return \"x\"; } print(f(true));"
        ),
        Err("NXL2001".into())
    );
}

// ---------------------------------------------------------------------------
// Generics
// ---------------------------------------------------------------------------

#[test]
fn baseline_generic_function_infers_type_arguments() {
    // Module 2.3 infers `T` from the argument when no `<...>` is written.
    let module = compile_ok(
        "generic_fn_inferred_args.nxl",
        "fn id<T>(x: T) -> T { return x; } print(id(1));",
    );
    let call = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::Call {
                callee,
                type_arguments,
                return_type,
                ..
            } if callee == "id" => Some((type_arguments.clone(), *return_type)),
            _ => None,
        })
        .expect("call to id");
    assert_eq!(call.0, vec![Type::Int]);
    assert_eq!(call.1, Type::Int);
    module.verify().expect("inferred generic call must verify");
    compile_ok(
        "generic_fn_explicit.nxl",
        "fn id<T>(x: T) -> T { return x; } print(id<Int>(1));",
    );
}

#[test]
fn baseline_generic_struct_infers_type_arguments() {
    // Construction without `<...>` infers the argument from the field value.
    let module = compile_ok(
        "generic_struct_inferred_args.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            let boxed = Box { value: "anything" };
            let text: String = boxed.value;
            print(text);
        "#,
    );
    let field_ty = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::FieldGet { ty, .. } => Some(*ty),
            _ => None,
        })
        .expect("field read");
    assert_eq!(field_ty, Type::String);
    compile_ok(
        "generic_struct_annotation.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            let boxed: Box<Int> = Box { value: 1 };
            print(boxed.value);
        "#,
    );
    compile_ok(
        "generic_struct_explicit.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            let boxed = Box<Int> { value: 1 };
            print(boxed.value);
        "#,
    );
}

// ---------------------------------------------------------------------------
// HIR-to-NIR type preservation and known gaps
// ---------------------------------------------------------------------------

#[test]
fn baseline_hir_types_survive_into_nir_signatures() {
    let module = compile_ok(
        "preserve.nxl",
        "fn combine(a: Int, b: String) -> Bool { return true; } print(combine(1, \"x\"));",
    );
    let signature = module
        .signatures
        .iter()
        .find(|signature| signature.name == "combine")
        .expect("combine signature");
    assert_eq!(signature.parameters[0].1, Type::Int);
    assert_eq!(signature.parameters[1].1, Type::String);
    assert_eq!(signature.return_type, Type::Bool);

    let call_return = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::Call {
                callee,
                return_type,
                ..
            } if callee == "combine" => Some(*return_type),
            _ => None,
        })
        .expect("call to combine");
    assert_eq!(call_return, Type::Bool);
    module.verify().expect("preserved types must verify");
}

#[test]
fn baseline_struct_field_reads_keep_their_static_type() {
    // Module 2.3 propagates the declared field type through `FieldGet`.
    let module = compile_ok(
        "field_type_ok.nxl",
        r#"
            struct Point {
                x: Int
            }
            let point = Point { x: 1 };
            let number: Int = point.x;
            print(number);
        "#,
    );
    module.verify().expect("typed field read must verify");
    let field_ty = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::FieldGet { ty, .. } => Some(*ty),
            _ => None,
        })
        .expect("field read");
    assert_eq!(field_ty, Type::Int);
    // Annotating the binding with the wrong type is now rejected.
    assert_eq!(
        compile(
            "field_type_gap.nxl",
            r#"
                struct Point {
                    x: Int
                }
                let point = Point { x: 1 };
                let text: String = point.x;
                print(text);
            "#
        ),
        Err("NXL2001".into())
    );
}

#[test]
fn baseline_conditional_expression_is_lowered() {
    let module = compile_ok("conditional.nxl", "let x = true ? 1 : 2; print(x);");
    module.verify().expect("conditional expression must verify");
    let inferred = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::Load { name, ty, .. } if name.starts_with("__conditional_") => Some(*ty),
            _ => None,
        })
        .expect("conditional result load");
    assert_eq!(inferred, Type::Int);
    // Branches must agree on a single result type.
    assert_eq!(
        compile(
            "conditional_branch_mismatch.nxl",
            "let x = true ? 1 : \"a\"; print(x);"
        ),
        Err("NXL2001".into())
    );
}

// ---------------------------------------------------------------------------
// Module 2.5 — structural type identity
// ---------------------------------------------------------------------------

#[test]
fn baseline_reference_annotations_round_trip_structural_types() {
    // A reference annotation is keyed by the identity of the type it points
    // at, so `&Box<Int>` agrees with the reference produced by `borrow`.
    let module = compile_ok(
        "reference_structural_type.nxl",
        r#"
            struct Box<T> { value: T }
            fn read(x: &Box<Int>) -> Int { return (*x).value; }
            let b: Box<Int> = Box<Int> { value: 6 };
            print(read(borrow b));
        "#,
    );
    module
        .verify()
        .expect("reference to a generic instance must verify");
    let signature = module
        .signatures
        .iter()
        .find(|signature| signature.name == "read")
        .expect("read signature");
    assert!(
        matches!(signature.parameters[0].1, Type::Reference(id, false) if id != 0),
        "reference parameter must keep a resolved target: {:?}",
        signature.parameters[0].1
    );
    assert_eq!(
        signature.parameter_annotations[0].as_deref(),
        Some("&Box<Int>")
    );
    let output = nxl_compiler::vm::execute(&module)
        .expect("the VM should run the reference program")
        .output;
    assert_eq!(output, vec!["6"]);
}

#[test]
fn baseline_reference_annotations_cover_arrays_and_mutable_targets() {
    compile_ok(
        "reference_array_annotation.nxl",
        r#"
            let xs = [1, 2];
            let r: &[Int] = borrow xs;
            print((*r)[0] + (*r)[1]);
        "#,
    );
    compile_ok(
        "reference_mutable_instance_annotation.nxl",
        r#"
            struct Box<T> { value: T }
            let mut b: Box<Int> = Box<Int> { value: 1 };
            {
                let r: &mut Box<Int> = borrow mut b;
                (*r).value = 5;
            }
            print(b.value);
        "#,
    );
    // The target still has to match: an `&Int` annotation cannot describe a
    // borrow of an array.
    assert_eq!(
        compile(
            "reference_annotation_mismatch.nxl",
            r#"
                let xs = [1, 2];
                let r: &Int = borrow xs;
                print(*r);
            "#
        ),
        Err("NXL2001".into())
    );
}

// ---------------------------------------------------------------------------
// Module 2.7 — Option/Optional identity and declared-name precedence
// ---------------------------------------------------------------------------

fn signature_parameter(module: &nxl_compiler::nir::Module, function: &str, index: usize) -> Type {
    module
        .signatures
        .iter()
        .find(|signature| signature.name == function)
        .unwrap_or_else(|| panic!("missing signature '{function}'"))
        .parameters[index]
        .1
}

#[test]
fn module_2_7_option_and_optional_are_one_identity() {
    let module = compile_ok(
        "option_optional_identity.nxl",
        r#"
            fn takes_option(x: Option<Int>) -> Int { return 1; }
            fn takes_optional(x: Optional<Int>) -> Int { return 2; }
            fn passes_through(x: Option<Int>) -> Optional<Int> { return x; }
        "#,
    );
    module.verify().expect("annotations must verify");
    let option = signature_parameter(&module, "takes_option", 0);
    let optional = signature_parameter(&module, "takes_optional", 0);
    assert!(matches!(option, Type::Optional(_)), "{option:?}");
    assert_eq!(option, optional, "Option and Optional must be one identity");
    assert_eq!(
        signature_parameter(&module, "passes_through", 0),
        option,
        "a return annotation must use the same identity"
    );
    // The return type is the canonical identity too, so `return x` agrees.
    let passes_through = module
        .signatures
        .iter()
        .find(|signature| signature.name == "passes_through")
        .expect("passes_through signature");
    assert_eq!(passes_through.return_type, option);

    // A different payload is still a different type.
    assert_eq!(
        compile(
            "option_mismatch.nxl",
            "fn f(x: Option<Int>) -> Option<String> { return x; }"
        ),
        Err("NXL2001".into())
    );
}

#[test]
fn module_2_7_nested_option_arguments_are_canonicalized() {
    let module = compile_ok(
        "option_nested_identity.nxl",
        r#"
            fn a(x: Option<Array<Int>>) -> Int { return 1; }
            fn b(x: Optional<Array<Int>>) -> Int { return 2; }
            fn c(x: Array<Option<Int>>) -> Int { return 3; }
            fn d(x: Array<Optional<Int>>) -> Int { return 4; }
            fn e(x: Option<Optional<Int>>) -> Int { return 5; }
            fn f(x: Optional<Option<Int>>) -> Int { return 6; }
        "#,
    );
    module.verify().expect("nested annotations must verify");
    assert_eq!(
        signature_parameter(&module, "a", 0),
        signature_parameter(&module, "b", 0),
        "the payload container spelling must not change identity"
    );
    assert_eq!(
        signature_parameter(&module, "c", 0),
        signature_parameter(&module, "d", 0),
        "a nested option inside an array must share identity"
    );
    assert_eq!(
        signature_parameter(&module, "e", 0),
        signature_parameter(&module, "f", 0),
        "deeply nested options must share identity"
    );
}

#[test]
fn module_2_7_option_identity_flows_through_generics() {
    // A type parameter nested in both spellings must resolve to one
    // identity, both as a parameter and as a return type.
    let module = compile_ok(
        "option_generic_flow.nxl",
        r#"
            fn consume<T>(value: Option<T>) -> Optional<T> { return value; }
            fn nested<T>(value: Optional<Array<T>>) -> Option<Array<T>> { return value; }
        "#,
    );
    module
        .verify()
        .expect("generic option annotations must verify");
    for function in ["consume", "nested"] {
        let signature = module
            .signatures
            .iter()
            .find(|signature| signature.name == function)
            .unwrap_or_else(|| panic!("missing signature '{function}'"));
        assert_eq!(
            signature.parameters[0].1, signature.return_type,
            "'{function}' must resolve both option spellings to one identity"
        );
    }
    // The negative control keeps the identity precise.
    assert_eq!(
        compile(
            "option_generic_mismatch.nxl",
            r#"
                fn bad(x: Option<Int>) -> Array<Int> { return x; }
            "#
        ),
        Err("NXL2001".into())
    );
}

#[test]
fn module_2_7_declared_type_shadows_builtin_container_name() {
    let module = compile_ok(
        "user_option.nxl",
        r#"
            struct Option<T> { value: T }
            let o: Option<Int> = Option<Int> { value: 5 };
            let v: Int = o.value;
            print(v);
        "#,
    );
    module
        .verify()
        .expect("a user type named Option must verify");
    let output = nxl_compiler::vm::execute(&module)
        .expect("the VM should run the user Option program")
        .output;
    assert_eq!(output, vec!["5"]);
}

#[test]
fn module_2_7_declared_result_shadows_builtin_container_name() {
    let module = compile_ok(
        "user_result.nxl",
        r#"
            struct Result<T, E> { value: T; error: E }
            let r: Result<Int, String> = Result<Int, String> { value: 9, error: "ok" };
            print(r.value);
        "#,
    );
    module
        .verify()
        .expect("a user type named Result must verify");
    let output = nxl_compiler::vm::execute(&module)
        .expect("the VM should run the user Result program")
        .output;
    assert_eq!(output, vec!["9"]);
}

// ---------------------------------------------------------------------------
// Module 2.7 — generic constraint validation and enforcement
// ---------------------------------------------------------------------------

#[test]
fn module_2_7_where_bound_is_enforced_with_impl_information() {
    let module = compile_ok(
        "where_bound_satisfied.nxl",
        r#"
            interface Greeter { fn greet() -> String; }
            struct Person { name: String }
            impl Greeter for Person { fn greet() -> String { return "hi"; } }
            fn keep<T>(value: T) -> T where T: Greeter { return value; }
            let p: Person = Person { name: "ada" };
            let q: Person = keep(p);
            print(q.name);
        "#,
    );
    module.verify().expect("a satisfied bound must verify");
    let output = nxl_compiler::vm::execute(&module)
        .expect("a satisfied bound must run")
        .output;
    assert_eq!(output, vec!["ada"]);
}

#[test]
fn module_2_7_unknown_bound_is_rejected() {
    assert_eq!(
        compile(
            "where_unknown_bound.nxl",
            r#"
                fn keep<T>(value: T) -> T where T: Missing { return value; }
                print(keep(1));
            "#
        ),
        Err("NXL2014".into())
    );
}

#[test]
fn module_2_7_non_trait_bound_is_rejected() {
    assert_eq!(
        compile(
            "where_non_trait_bound.nxl",
            r#"
                struct NotATrait { value: Int }
                fn keep<T>(value: T) -> T where T: NotATrait { return value; }
                print(keep(1));
            "#
        ),
        Err("NXL2014".into())
    );
}

#[test]
fn module_2_7_unknown_constrained_parameter_is_rejected() {
    assert_eq!(
        compile(
            "where_unknown_parameter.nxl",
            r#"
                interface Greeter { fn greet() -> String; }
                fn keep<T>(value: T) -> T where U: Greeter { return value; }
                print(keep(1));
            "#
        ),
        Err("NXL2014".into())
    );
}

#[test]
fn module_2_7_duplicate_constraint_is_rejected() {
    assert_eq!(
        compile(
            "where_duplicate_constraint.nxl",
            r#"
                interface First { fn one() -> Int; }
                interface Second { fn two() -> Int; }
                fn keep<T>(value: T) -> T where T: First, T: Second { return value; }
                print(keep(1));
            "#
        ),
        Err("NXL2014".into())
    );
}

#[test]
fn module_2_7_constraint_must_be_satisfied_at_instantiation() {
    assert_eq!(
        compile(
            "where_bound_unimplemented.nxl",
            r#"
                interface Greeter { fn greet() -> String; }
                struct Person { name: String }
                struct Other { value: Int }
                impl Greeter for Person { fn greet() -> String { return "hi"; } }
                fn keep<T>(value: T) -> T where T: Greeter { return value; }
                let o: Other = Other { value: 1 };
                let q: Other = keep(o);
                print(q.value);
            "#
        ),
        Err("NXL2014".into())
    );
}

#[test]
fn module_2_7_constraints_on_type_declarations_are_rejected() {
    assert_eq!(
        compile(
            "where_on_type_declaration.nxl",
            r#"
                interface Greeter { fn greet() -> String; }
                struct Box<T> where T: Greeter { value: T }
                let b: Box<Int> = Box<Int> { value: 1 };
                print(b.value);
            "#
        ),
        Err("NXL2014".into())
    );
}

#[test]
fn module_2_7_inline_bound_points_at_where_syntax() {
    let diagnostic = compile_to_nir(&SourceFile::new(
        "inline_bound.nxl",
        "fn keep<T: Greeter>(value: T) -> T { return value; }",
    ))
    .expect_err("inline bounds are not supported");
    assert_eq!(diagnostic.code, "NXL1001");
    assert!(
        diagnostic.message.contains("where"),
        "the diagnostic should name the supported form: {}",
        diagnostic.message
    );
}
