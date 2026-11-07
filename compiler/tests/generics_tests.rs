//! Module 2.2 regression tests: explicit generic instantiation.
//!
//! The compiler accepts type arguments written by the programmer
//! (`identity<Int>(25)`, `Box<Int> { .. }`) and substitutes them into
//! signatures and struct fields. It deliberately does not infer them yet;
//! a call without explicit arguments is reported as NXL2014.

use nxl_compiler::{
    compile_to_nir,
    nir::{Instruction, Type},
    source::SourceFile,
};

fn compile_ok(name: &str, source: &str) -> nxl_compiler::nir::Module {
    compile_to_nir(&SourceFile::new(name, source)).unwrap_or_else(|diagnostic| {
        panic!(
            "{name} should lower, got {}: {}",
            diagnostic.code, diagnostic.message
        )
    })
}

fn compile_err(name: &str, source: &str) -> String {
    compile_to_nir(&SourceFile::new(name, source))
        .err()
        .unwrap_or_else(|| panic!("{name} was accepted but should be rejected"))
        .code
}

fn calls_to<'a>(module: &'a nxl_compiler::nir::Module, callee: &'a str) -> Vec<&'a Instruction> {
    module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .filter(|instruction| {
            matches!(instruction, Instruction::Call { callee: name, .. } if name == callee)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Generic functions
// ---------------------------------------------------------------------------

#[test]
fn identity_instantiates_with_int() {
    let module = compile_ok(
        "identity_int.nxl",
        r#"
            fn identity<T>(value: T) -> T { return value }
            let number = identity<Int>(25);
            print(number);
        "#,
    );
    module.verify().expect("identity<Int> program must verify");
}

#[test]
fn identity_instantiates_with_string() {
    let module = compile_ok(
        "identity_string.nxl",
        r#"
            fn identity<T>(value: T) -> T { return value }
            let text = identity<String>("hello");
            print(text);
        "#,
    );
    module
        .verify()
        .expect("identity<String> program must verify");
}

#[test]
fn generic_function_rejects_wrong_argument_type() {
    assert_eq!(
        compile_err(
            "identity_wrong_argument.nxl",
            r#"
                fn identity<T>(value: T) -> T { return value }
                print(identity<Int>("hello"));
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn generic_function_infers_type_arguments() {
    // Module 2.3: a call site without `<...>` infers the argument from the
    // erased parameter annotation (`value: T`).
    let module = compile_ok(
        "identity_inferred_arguments.nxl",
        r#"
            fn identity<T>(value: T) -> T { return value }
            print(identity(25));
        "#,
    );
    module.verify().expect("inferred generic call must verify");
    let call = calls_to(&module, "identity")
        .into_iter()
        .find_map(|instruction| match instruction {
            Instruction::Call {
                type_arguments,
                return_type,
                ..
            } => Some((type_arguments.clone(), *return_type)),
            _ => None,
        })
        .expect("identity call");
    assert_eq!(call.0, vec![Type::Int]);
    assert_eq!(call.1, Type::Int);
}

#[test]
fn generic_function_rejects_unknown_type_argument() {
    assert_eq!(
        compile_err(
            "identity_unknown_argument.nxl",
            r#"
                fn identity<T>(value: T) -> T { return value }
                print(identity<Missing>(25));
            "#
        ),
        "NXL2004"
    );
}

#[test]
fn generic_function_rejects_wrong_type_argument_count() {
    assert_eq!(
        compile_err(
            "identity_extra_argument.nxl",
            r#"
                fn identity<T>(value: T) -> T { return value }
                print(identity<Int, String>(25));
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn non_generic_function_rejects_type_arguments() {
    assert_eq!(
        compile_err(
            "plain_with_arguments.nxl",
            r#"
                fn plain(value: Int) -> Int { return value }
                print(plain<Int>(25));
            "#
        ),
        "NXL2014"
    );
}

#[test]
fn generic_call_records_instantiated_types_in_nir() {
    let module = compile_ok(
        "identity_nir.nxl",
        r#"
            fn identity<T>(value: T) -> T { return value }
            let number: Int = identity<Int>(25);
            print(number);
        "#,
    );
    let call = calls_to(&module, "identity")
        .into_iter()
        .find_map(|instruction| match instruction {
            Instruction::Call {
                type_arguments,
                return_type,
                ..
            } => Some((type_arguments.clone(), *return_type)),
            _ => None,
        })
        .expect("identity call");
    assert_eq!(call.0, vec![Type::Int]);
    assert_eq!(call.1, Type::Int, "return type is substituted, not Param");
    module
        .verify()
        .expect("instantiated call types must stay consistent");
}

#[test]
fn generic_types_flow_across_calls_and_returns() {
    let module = compile_ok(
        "identity_flow.nxl",
        r#"
            fn identity<T>(value: T) -> T { return value }
            fn increment(value: Int) -> Int { return value + 1 }
            let number: Int = identity<Int>(increment(identity<Int>(4)));
            let text: String = identity<String>("hello");
            print(number);
            print(text);
        "#,
    );
    module
        .verify()
        .expect("substituted returns must flow into other calls");
}

#[test]
fn multiple_type_parameters_substitute_in_order() {
    let module = compile_ok(
        "swap.nxl",
        r#"
            fn second<A, B>(first: A, other: B) -> B { return other }
            let text: String = second<Int, String>(1, "hello");
            print(text);
        "#,
    );
    module
        .verify()
        .expect("two-parameter generic call must verify");

    assert_eq!(
        compile_err(
            "swap_wrong_argument.nxl",
            r#"
                fn second<A, B>(first: A, other: B) -> B { return other }
                print(second<Int, String>(1, 2));
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn generic_container_parameters_are_substituted_structurally() {
    let module = compile_ok(
        "head.nxl",
        r#"
            fn head<T>(values: Array<T>) -> T { return values[0] }
            let number: Int = head<Int>([1, 2, 3]);
            print(number);
        "#,
    );
    module
        .verify()
        .expect("Array<T> should substitute to Array<Int>");
}

// ---------------------------------------------------------------------------
// Generic structs
// ---------------------------------------------------------------------------

#[test]
fn box_accepts_a_field_of_the_instantiated_type() {
    let module = compile_ok(
        "box_int.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            let boxed: Box<Int> = Box<Int> { value: 100 };
            print(boxed.value);
        "#,
    );
    module.verify().expect("Box<Int> program must verify");
}

#[test]
fn box_rejects_a_field_of_another_type() {
    assert_eq!(
        compile_err(
            "box_wrong_field.nxl",
            r#"
                struct Box<T> {
                    value: T
                }
                let boxed = Box<Int> { value: "hello" };
                print(boxed.value);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn box_infers_type_arguments_from_field_values() {
    // `Box { value: 1 }` infers `Box<Int>`, and the field read keeps `Int`.
    let module = compile_ok(
        "box_inferred_arguments.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            let boxed = Box { value: 1 };
            let number: Int = boxed.value;
            print(number);
        "#,
    );
    module.verify().expect("inferred Box must verify");
    let (struct_ty, field_ty) = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .fold(
            (None, None),
            |(struct_ty, field_ty), instruction| match instruction {
                Instruction::StructNew { ty, .. } => (Some(*ty), field_ty),
                Instruction::FieldGet { ty, .. } => (struct_ty, Some(*ty)),
                _ => (struct_ty, field_ty),
            },
        );
    assert!(
        matches!(struct_ty, Some(Type::Instance(_, _))),
        "inferred Box construction must carry an instance type: {struct_ty:?}"
    );
    assert_eq!(field_ty, Some(Type::Int));
}

#[test]
fn box_rejects_unknown_type_arguments() {
    assert_eq!(
        compile_err(
            "box_unknown_argument.nxl",
            r#"
                struct Box<T> {
                    value: T
                }
                let boxed = Box<Missing> { value: 1 };
                print(boxed.value);
            "#
        ),
        "NXL2004"
    );
}

#[test]
fn non_generic_struct_rejects_type_arguments() {
    assert_eq!(
        compile_err(
            "point_with_arguments.nxl",
            r#"
                struct Point {
                    x: Int
                }
                let point = Point<Int> { x: 1 };
                print(point.x);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn distinct_instantiations_are_not_interchangeable() {
    let module = compile_ok(
        "box_distinct.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            let number: Box<Int> = Box<Int> { value: 1 };
            let text: Box<String> = Box<String> { value: "hello" };
            print(number.value);
            print(text.value);
        "#,
    );
    let instance_types = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| match instruction {
            Instruction::StructNew { ty, .. } => Some(*ty),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(instance_types.len(), 2);
    assert!(matches!(instance_types[0], Type::Instance(_, _)));
    assert!(matches!(instance_types[1], Type::Instance(_, _)));
    assert_ne!(
        instance_types[0], instance_types[1],
        "Box<Int> and Box<String> must have different identities"
    );

    assert_eq!(
        compile_err(
            "box_cross_assign.nxl",
            r#"
                struct Box<T> {
                    value: T
                }
                let number: Box<Int> = Box<Int> { value: 1 };
                let text: Box<String> = number;
                print(text.value);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn generic_struct_field_flows_through_function_parameters() {
    let module = compile_ok(
        "box_argument.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            fn unwrap(box: Box<Int>) -> Int { return box.value }
            let boxed = Box<Int> { value: 42 };
            let result: Int = unwrap(boxed);
            print(result);
        "#,
    );
    module
        .verify()
        .expect("Box<Int> parameters must keep their instantiation");

    assert_eq!(
        compile_err(
            "box_argument_mismatch.nxl",
            r#"
                struct Box<T> {
                    value: T
                }
                fn unwrap(box: Box<Int>) -> Int { return box.value }
                let boxed = Box<String> { value: "hello" };
                print(unwrap(boxed));
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn nested_generic_structs_are_supported() {
    let module = compile_ok(
        "box_nested.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            let inner: Box<Int> = Box<Int> { value: 1 };
            let outer: Box<Box<Int>> = Box<Box<Int>> { value: inner };
            print(outer.value);
        "#,
    );
    module.verify().expect("Box<Box<Int>> program must verify");
}

#[test]
fn nested_generic_structs_validate_the_inner_argument() {
    assert_eq!(
        compile_err(
            "box_nested_mismatch.nxl",
            r#"
                struct Box<T> {
                    value: T
                }
                let inner: Box<String> = Box<String> { value: "hello" };
                let outer: Box<Box<Int>> = Box<Box<Int>> { value: inner };
                print(outer.value);
            "#
        ),
        "NXL2001"
    );
}

// ---------------------------------------------------------------------------
// End-to-end
// ---------------------------------------------------------------------------

#[test]
fn explicit_generics_run_on_the_vm() {
    let module = compile_ok(
        "generics_runtime.nxl",
        r#"
            fn identity<T>(value: T) -> T { return value }
            struct Box<T> {
                value: T
            }
            let number = identity<Int>(25);
            let boxed = Box<Int> { value: 100 };
            print(number);
            print(boxed.value);
        "#,
    );
    let output = nxl_compiler::vm::execute(&module)
        .expect("the VM should run explicit generic programs")
        .output;
    assert_eq!(output, vec!["25", "100"]);
}

#[test]
fn explicit_generics_work_across_project_modules() {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "nxl_generics_project_{}_{}",
        std::process::id(),
        id
    ));
    let module_dir = dir.join("src").join("app");
    std::fs::create_dir_all(&module_dir).unwrap();
    std::fs::write(
        module_dir.join("box.nxl"),
        r#"
            struct Box<T> {
                value: T
            }
            fn identity<T>(value: T) -> T { return value }
        "#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src").join("main.nxl"),
        r#"
            from app.box import Box, identity;
            let boxed: Box<Int> = Box<Int> { value: 100 };
            let number: Int = identity<Int>(25);
            print(boxed.value);
            print(number);
        "#,
    )
    .unwrap();

    let module = nxl_compiler::project::compile_project(&dir.join("src").join("main.nxl"))
        .expect("cross-module generics should compile");
    module.verify().expect("cross-module generics must verify");
    let output = nxl_compiler::vm::execute(&module)
        .expect("the VM should run cross-module generics")
        .output;
    assert_eq!(output, vec!["100", "25"]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn nested_generic_fields_survive_cross_module_qualification() {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "nxl_generics_nested_project_{}_{}",
        std::process::id(),
        id
    ));
    let module_dir = dir.join("src").join("app");
    std::fs::create_dir_all(&module_dir).unwrap();
    std::fs::write(
        module_dir.join("box.nxl"),
        r#"
            struct Box<T> {
                value: T
            }
            struct Pair<T> {
                first: Box<T>
            }
        "#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src").join("main.nxl"),
        r#"
            from app.box import Box, Pair;
            let inner: Box<Int> = Box<Int> { value: 7 };
            let pair: Pair<Int> = Pair<Int> { first: inner };
            print(pair.first.value);
        "#,
    )
    .unwrap();

    let module = nxl_compiler::project::compile_project(&dir.join("src").join("main.nxl"))
        .expect("nested generic fields should compile across modules");
    module
        .verify()
        .expect("nested generic fields must verify across modules");
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Generic bodies and generic enums
// ---------------------------------------------------------------------------

#[test]
fn generic_function_bodies_check_parameters_as_dynamic() {
    // The body is lowered once, with `T` erased. Arithmetic/comparison must
    // be accepted here and checked by the concrete call-site instantiation.
    let module = compile_ok(
        "generic_math.nxl",
        r#"
            fn add<T>(a: T, b: T) -> T { return a + b }
            fn same<T>(a: T, b: T) -> Bool { return a == b }
            fn less<T>(a: T, b: T) -> Bool { return a < b }
            print(add<Int>(1, 2));
            print(same<String>("a", "a"));
            print(less<Int>(1, 2));
        "#,
    );
    module
        .verify()
        .expect("erased generic bodies with operators must verify");
    let output = nxl_compiler::vm::execute(&module)
        .expect("the VM should run erased generic bodies")
        .output;
    assert_eq!(output, vec!["3", "true", "true"]);
}

#[test]
fn generic_enum_variant_instantiates_explicitly() {
    let module = compile_ok(
        "maybe.nxl",
        r#"
            enum Maybe<T> {
                Some(T),
                None
            }
            let some: Maybe<Int> = Maybe.Some<Int>(1);
            let none: Maybe<Int> = Maybe.None<Int>();
            print(some);
            print(none);
        "#,
    );
    module
        .verify()
        .expect("explicitly instantiated generic enum must verify");
    let instances = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| match instruction {
            Instruction::EnumNew { ty, .. } => Some(*ty),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(instances.len(), 2);
    assert!(
        instances
            .iter()
            .all(|ty| matches!(ty, Type::Instance(_, _))),
        "generic enum values must record their instantiation: {instances:?}"
    );
    assert_eq!(instances[0], instances[1]);
}

#[test]
fn generic_enum_rejects_wrong_payload_type() {
    assert_eq!(
        compile_err(
            "maybe_wrong_payload.nxl",
            r#"
                enum Maybe<T> {
                    Some(T),
                    None
                }
                let value: Maybe<Int> = Maybe.Some<Int>("hello");
                print(value);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn generic_enum_infers_type_arguments_from_payload() {
    let module = compile_ok(
        "maybe_inferred_arguments.nxl",
        r#"
            enum Maybe<T> {
                Some(T),
                None
            }
            let value = Maybe.Some(1);
            let typed: Maybe<Int> = value;
            print(typed);
        "#,
    );
    module.verify().expect("inferred Maybe must verify");
    let payload_ty = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::EnumNew { ty, .. } => Some(*ty),
            _ => None,
        })
        .expect("Maybe.Some construction");
    assert!(
        matches!(payload_ty, Type::Instance(_, _)),
        "inferred enum construction must carry an instance type: {payload_ty:?}"
    );
}

#[test]
fn generic_enum_rejects_wrong_type_argument_count() {
    assert_eq!(
        compile_err(
            "maybe_extra_arguments.nxl",
            r#"
                enum Maybe<T> {
                    Some(T),
                    None
                }
                let value = Maybe.Some<Int, String>(1);
                print(value);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn non_generic_enum_rejects_type_arguments() {
    assert_eq!(
        compile_err(
            "plain_enum_with_arguments.nxl",
            r#"
                enum Choice {
                    Some(Int),
                    None
                }
                let value = Choice.Some<Int>(1);
                print(value);
            "#
        ),
        "NXL2014"
    );
}

#[test]
fn hir_preserves_generic_signatures() {
    let source = SourceFile::new(
        "hir_generics.nxl",
        r#"
            struct Box<T> {
                value: T
            }
            fn identity<T>(value: T) -> T { return value }
        "#,
    );
    let program = nxl_compiler::parse_source(&source).expect("generic declarations should parse");
    let hir = nxl_compiler::hir::analyze(&program, "hir_generics.nxl")
        .expect("generic declarations should analyze");
    let signature = hir
        .functions
        .iter()
        .find(|signature| signature.name == "identity")
        .expect("identity signature");
    assert_eq!(signature.type_parameters, vec!["T".to_owned()]);
    assert!(
        matches!(signature.parameters[0].1, Type::Param(_)),
        "unsubstituted parameter should survive into HIR: {:?}",
        signature.parameters[0].1
    );
    assert!(
        matches!(signature.return_type, Type::Param(_)),
        "unsubstituted return should survive into HIR: {:?}",
        signature.return_type
    );
}

// ---------------------------------------------------------------------------
// Module 2.5 — generic, inference and ownership integration
// ---------------------------------------------------------------------------

fn run_program(name: &str, source: &str) -> Vec<String> {
    let module = compile_ok(name, source);
    module
        .verify()
        .unwrap_or_else(|diagnostic| panic!("{name} failed verification: {}", diagnostic.message));
    nxl_compiler::vm::execute(&module)
        .unwrap_or_else(|diagnostic| panic!("{name} failed at runtime: {}", diagnostic.message))
        .output
}

#[test]
fn generic_instance_argument_infers_the_type_parameter() {
    // The concrete arguments behind `Box<Int>` must be recoverable, so
    // `identity(Box<Int> { .. })` binds `T = Box<Int>` instead of reporting
    // "cannot infer ... from a dynamic value".
    let output = run_program(
        "instance_argument_inference.nxl",
        r#"
            struct Box<T> { value: T }
            fn identity<T>(value: T) -> T { return value }
            let boxed = identity(Box<Int> { value: 3 });
            let number: Int = boxed.value;
            print(number);
        "#,
    );
    assert_eq!(output, vec!["3"]);
}

#[test]
fn nested_generic_instance_argument_infers() {
    let output = run_program(
        "nested_instance_argument_inference.nxl",
        r#"
            struct Box<T> { value: T }
            fn identity<T>(value: T) -> T { return value }
            let boxed = identity(Box<Box<Int>> { value: Box<Int> { value: 3 } });
            let number: Int = boxed.value.value;
            print(number);
        "#,
    );
    assert_eq!(output, vec!["3"]);
}

#[test]
fn generic_instance_parameter_field_is_statically_typed() {
    // A `Box<Int>` parameter used to expose `T` as a dynamic field type, so
    // this function compiled and returned `1` where `String` was required.
    assert_eq!(
        compile_err(
            "instance_parameter_field.nxl",
            r#"
                struct Box<T> { value: T }
                fn bad(b: Box<Int>) -> String { return b.value; }
                let b: Box<Int> = Box<Int> { value: 1 };
                print(bad(b));
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn generic_instance_parameter_field_supports_arithmetic() {
    let output = run_program(
        "instance_parameter_arithmetic.nxl",
        r#"
            struct Box<T> { value: T }
            fn unwrap(b: Box<Int>) -> Int { return b.value; }
            let b: Box<Int> = Box<Int> { value: 4 };
            print(unwrap(b) + 1);
        "#,
    );
    assert_eq!(output, vec!["5"]);
}

#[test]
fn nested_instance_parameter_field_is_statically_typed() {
    // Nested instances (`Box<Box<Int>>`) must record their arguments too, so
    // `o.value` is a `Box<Int>` and `o.value.value` is an `Int` even inside a
    // parameter annotation.
    let output = run_program(
        "nested_instance_parameter.nxl",
        r#"
            struct Box<T> { value: T }
            fn good(o: Box<Box<Int>>) -> Int { return o.value.value; }
            let b: Box<Box<Int>> = Box<Box<Int>> { value: Box<Int> { value: 7 } };
            print(good(b));
        "#,
    );
    assert_eq!(output, vec!["7"]);
    assert_eq!(
        compile_err(
            "nested_instance_parameter_mismatch.nxl",
            r#"
                struct Box<T> { value: T }
                fn bad(o: Box<Box<Int>>) -> String { return o.value.value; }
                let b: Box<Box<Int>> = Box<Box<Int>> { value: Box<Int> { value: 1 } };
                print(bad(b));
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn inferred_generic_struct_argument_binds_the_parameter() {
    let output = run_program(
        "inferred_instance_argument.nxl",
        r#"
            struct Box<T> { value: T }
            fn unwrap<T>(b: Box<T>) -> T { return b.value; }
            let b = Box<Int> { value: 4 };
            print(unwrap(b) + 1);
        "#,
    );
    assert_eq!(output, vec!["5"]);
}

#[test]
fn reference_annotation_to_a_generic_instance_matches_the_borrow() {
    let output = run_program(
        "reference_to_instance.nxl",
        r#"
            struct Box<T> { value: T }
            let b: Box<Int> = Box<Int> { value: 8 };
            {
                let r: &Box<Int> = borrow b;
                print((*r).value);
            }
            print(b.value);
        "#,
    );
    assert_eq!(output, vec!["8", "8"]);
}

#[test]
fn reference_annotation_to_an_array_matches_the_borrow() {
    let output = run_program(
        "reference_to_array.nxl",
        r#"
            let xs = [1, 2];
            let r: &[Int] = borrow xs;
            print((*r)[0] + (*r)[1]);
        "#,
    );
    assert_eq!(output, vec!["3"]);
}

#[test]
fn reference_annotation_wrapping_a_generic_type_is_accepted() {
    let output = run_program(
        "reference_parameter_instance.nxl",
        r#"
            struct Box<T> { value: T }
            fn read<T>(x: &Box<T>) -> T { return (*x).value; }
            let b: Box<Int> = Box<Int> { value: 4 };
            print(read<Int>(borrow b));
        "#,
    );
    assert_eq!(output, vec!["4"]);
}

#[test]
fn reference_annotation_wrapping_an_array_is_accepted() {
    let output = run_program(
        "reference_parameter_array.nxl",
        r#"
            fn total(xs: &[Int]) -> Int {
                return (*xs)[0] + (*xs)[1];
            }
            let xs = [1, 2];
            print(total(borrow xs));
        "#,
    );
    assert_eq!(output, vec!["3"]);
}

#[test]
fn generic_inference_through_a_reference_parameter() {
    // `&T` constrains `T` to the referenced type, not to the reference.
    let output = run_program(
        "reference_parameter_inference.nxl",
        r#"
            fn read_value<T>(x: &T) -> T { return *x; }
            let n = 41;
            print(read_value(borrow n) + 1);
        "#,
    );
    assert_eq!(output, vec!["42"]);
}

#[test]
fn generic_parameter_accepts_a_reference_only_through_an_annotated_slot() {
    // A `&T` parameter names the reference, so the borrow keeps its
    // provenance through instantiation and the value round-trips.
    let output = run_program(
        "reference_type_argument.nxl",
        r#"
            fn id<T>(x: &T) -> T { return *x }
            let n = 1;
            print(id(borrow n));
        "#,
    );
    assert_eq!(output, vec!["1"]);
}

#[test]
fn unannotated_generic_parameter_rejects_a_reference_argument() {
    // `T` may be bound to `&Int`, but the slot does not name a reference, so
    // the callee's `return x` is checked against an owned `T` and the borrow
    // would leave the caller's tracking. Passing it is rejected (NXL2017).
    assert_eq!(
        compile_err(
            "unannotated_generic_reference.nxl",
            r#"
                fn id<T>(x: T) -> T { return x }
                let n = 1;
                let r = id(borrow n);
                print(*r);
            "#
        ),
        "NXL2017"
    );
}

#[test]
fn explicitly_annotated_generic_type_argument_rejects_a_reference() {
    // Writing the type argument out does not change the analysis: `T` is
    // bound to `&Int` in an unannotated slot, which is still untrackable.
    assert_eq!(
        compile_err(
            "explicit_generic_reference.nxl",
            r#"
                fn id<T>(x: T) -> T { return x }
                let n = 1;
                let r = id<&Int>(borrow n);
                print(*r);
            "#
        ),
        "NXL2017"
    );
}

#[test]
fn generic_function_cannot_construct_an_instance_from_its_own_parameters() {
    // Module 2.5 limitation (deferred D8): a generic body may not write
    // `Pair<A, B>` from its own type parameters. Tracked for a later module.
    assert_eq!(
        compile_err(
            "instance_from_type_parameters.nxl",
            r#"
                struct Pair<A, B> { first: A; second: B }
                fn make_pair<A, B>(a: A, b: B) -> Pair<A, B> {
                    return Pair<A, B> { first: a; second: b };
                }
                let p = make_pair(1, "two");
                print(p.first);
            "#
        ),
        "NXL2014"
    );
}
