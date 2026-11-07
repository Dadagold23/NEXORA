//! Module 2.3 regression tests: type inference.
//!
//! The compiler infers local binding types, unannotated return types, generic
//! arguments at call/construction sites, field and element reads, and the
//! result of conditional expressions. Inference never guesses: a program that
//! does not determine one consistent type is rejected.

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

fn call_to<'a>(module: &'a nxl_compiler::nir::Module, callee: &'a str) -> &'a Instruction {
    module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find(|instruction| {
            matches!(
                instruction,
                Instruction::Call { callee: name, .. } if name == callee
            )
        })
        .unwrap_or_else(|| panic!("call to '{callee}'"))
}

fn signature_return(module: &nxl_compiler::nir::Module, name: &str) -> Type {
    module
        .signatures
        .iter()
        .find(|signature| signature.name == name)
        .map(|signature| signature.return_type)
        .unwrap_or_else(|| panic!("signature '{name}'"))
}

fn run(source: &str) -> Vec<String> {
    let module = compile_ok("run.nxl", source);
    module.verify().expect("module must verify");
    nxl_compiler::vm::execute(&module)
        .expect("the VM should execute the inferred program")
        .output
}

// ---------------------------------------------------------------------------
// Local bindings and expression propagation
// ---------------------------------------------------------------------------

#[test]
fn local_bindings_propagate_operand_types() {
    let module = compile_ok(
        "locals.nxl",
        r#"
            let base = 10;
            let doubled = base * 2;
            let text = "value";
            let flag = doubled > base;
            print(doubled);
            print(text);
            print(flag);
        "#,
    );
    module.verify().expect("inferred locals must verify");
    let binary = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::Binary { ty, .. } => Some(*ty),
            _ => None,
        })
        .expect("binary instruction");
    assert_eq!(binary, Type::Int);
    assert_eq!(run("let x = 1 + 2; print(x);"), vec!["3"]);
}

#[test]
fn field_reads_keep_the_declared_field_type() {
    let module = compile_ok(
        "field_read.nxl",
        r#"
            struct Point {
                x: Int
                label: String
            }
            let point = Point { x: 7, label: "seven" };
            let x: Int = point.x;
            let label: String = point.label;
            print(x);
            print(label);
        "#,
    );
    module.verify().expect("typed field reads must verify");
    let field_types = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| match instruction {
            Instruction::FieldGet { field, ty, .. } => Some((field.as_str(), *ty)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(field_types.contains(&("x", Type::Int)));
    assert!(field_types.contains(&("label", Type::String)));
}

#[test]
fn index_reads_propagate_the_array_element_type() {
    let module = compile_ok(
        "index_read.nxl",
        r#"
            struct Point {
                x: Int
            }
            let points = [Point { x: 1 }, Point { x: 2 }];
            let first = points[0];
            let value: Int = first.x;
            print(value);
        "#,
    );
    module.verify().expect("typed index reads must verify");
    let index_ty = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .find_map(|instruction| match instruction {
            Instruction::IndexGet { ty, .. } => Some(*ty),
            _ => None,
        })
        .expect("index read");
    assert!(
        matches!(index_ty, Type::Generic(_)),
        "the element type must be the struct, got {index_ty:?}"
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
        .expect("field read on element");
    assert_eq!(field_ty, Type::Int);
}

#[test]
fn wrong_annotation_on_a_field_read_is_rejected() {
    assert_eq!(
        compile_err(
            "field_read_wrong.nxl",
            r#"
                struct Point {
                    x: Int
                }
                let point = Point { x: 1 };
                let text: String = point.x;
                print(text);
            "#
        ),
        "NXL2001"
    );
}

// ---------------------------------------------------------------------------
// Return-type inference
// ---------------------------------------------------------------------------

#[test]
fn unannotated_return_types_are_inferred_by_name() {
    let module = compile_ok(
        "returns.nxl",
        r#"
            fn answer() { return 42; }
            fn greeting() { return "hello"; }
            let number: Int = answer();
            let text: String = greeting();
            print(number);
            print(text);
        "#,
    );
    module.verify().expect("inferred returns must verify");
    assert_eq!(signature_return(&module, "answer"), Type::Int);
    assert_eq!(signature_return(&module, "greeting"), Type::String);
    assert!(matches!(
        call_to(&module, "answer"),
        Instruction::Call {
            return_type: Type::Int,
            ..
        }
    ));
}

#[test]
fn return_inference_is_order_independent() {
    // `caller` is lowered before `callee`, yet it must see `Int`.
    let module = compile_ok(
        "forward_returns.nxl",
        r#"
            fn caller() { return callee(); }
            fn callee() { return 42; }
            let number: Int = caller();
            print(number);
        "#,
    );
    module.verify().expect("forward inference must verify");
    assert_eq!(signature_return(&module, "caller"), Type::Int);
    assert_eq!(signature_return(&module, "callee"), Type::Int);
    assert_eq!(
        run("fn a() { return b(); } fn b() { return 9; } print(a());"),
        vec!["9"]
    );
}

#[test]
fn return_inference_accepts_agreeing_branches() {
    let module = compile_ok(
        "branch_returns.nxl",
        r#"
            fn choose(flag: Bool) {
                if flag {
                    return 1;
                } else {
                    return 2;
                }
            }
            let number: Int = choose(true);
            print(number);
        "#,
    );
    module.verify().expect("agreeing branches must verify");
    assert_eq!(signature_return(&module, "choose"), Type::Int);
    assert_eq!(
        run("fn choose(flag: Bool) { if flag { return 1; } else { return 2; } } print(choose(false));"),
        vec!["2"]
    );
}

#[test]
fn disagreeing_return_types_are_rejected() {
    assert_eq!(
        compile_err(
            "branch_return_conflict.nxl",
            r#"
                fn choose(flag: Bool) {
                    if flag { return 1; }
                    return "no";
                }
                print(choose(true));
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn a_function_without_returns_infers_void() {
    let module = compile_ok(
        "void_return.nxl",
        "fn noop() { let x = 1; print(x); } noop();",
    );
    assert_eq!(signature_return(&module, "noop"), Type::Void);
}

// ---------------------------------------------------------------------------
// Generic inference
// ---------------------------------------------------------------------------

#[test]
fn generic_function_infers_from_arguments() {
    let module = compile_ok(
        "generic_call.nxl",
        r#"
            fn identity<T>(value: T) -> T { return value; }
            let number: Int = identity(5);
            let text: String = identity("five");
            print(number);
            print(text);
        "#,
    );
    module.verify().expect("inferred generic calls must verify");
    let inferred = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| match instruction {
            Instruction::Call {
                callee,
                type_arguments,
                ..
            } if callee == "identity" => Some(type_arguments.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(inferred, vec![vec![Type::Int], vec![Type::String]]);
}

#[test]
fn generic_inference_handles_multiple_parameters_in_order() {
    let module = compile_ok(
        "generic_pair.nxl",
        r#"
            fn second<A, B>(first: A, second: B) -> B { return second; }
            let text: String = second(1, "one");
            print(text);
        "#,
    );
    module
        .verify()
        .expect("multi-parameter inference must verify");
    let (arguments, return_type) = match call_to(&module, "second") {
        Instruction::Call {
            type_arguments,
            return_type,
            ..
        } => (type_arguments.clone(), *return_type),
        _ => unreachable!(),
    };
    assert_eq!(arguments, vec![Type::Int, Type::String]);
    assert_eq!(return_type, Type::String);
}

#[test]
fn generic_inference_penetrates_array_parameters() {
    let module = compile_ok(
        "generic_array.nxl",
        r#"
            fn first<T>(values: [T]) -> T { return values[0]; }
            let number: Int = first([1, 2, 3]);
            print(number);
        "#,
    );
    module.verify().expect("array generics must verify");
    assert_eq!(
        run("fn first<T>(values: [T]) -> T { return values[0]; } print(first([4, 5]));"),
        vec!["4"]
    );
}

#[test]
fn generic_inference_rejects_conflicting_arguments() {
    assert_eq!(
        compile_err(
            "generic_conflict.nxl",
            r#"
                fn same<T>(left: T, right: T) -> T { return left; }
                print(same(1, "one"));
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn generic_inference_reports_ambiguity() {
    // No argument constrains `T`, so the compiler must not guess.
    assert_eq!(
        compile_err(
            "generic_ambiguous.nxl",
            r#"
                fn make<T>() -> T { return 1; }
                print(make());
            "#
        ),
        "NXL2014"
    );
}

#[test]
fn generic_struct_inference_checks_agreeing_fields() {
    let module = compile_ok(
        "generic_struct.nxl",
        r#"
            struct Pair<T> {
                first: T
                second: T
            }
            let pair = Pair { first: 1, second: 2 };
            let total: Int = pair.first + pair.second;
            print(total);
        "#,
    );
    module
        .verify()
        .expect("inferred generic struct must verify");
    assert_eq!(
        compile_err(
            "generic_struct_conflict.nxl",
            r#"
                struct Pair<T> {
                    first: T
                    second: T
                }
                let pair = Pair { first: 1, second: "two" };
                print(pair.first);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn generic_enum_inference_requires_a_constraining_payload() {
    let module = compile_ok(
        "generic_enum.nxl",
        r#"
            enum Maybe<T> {
                Some(T),
                None
            }
            let some = Maybe.Some(1);
            let typed: Maybe<Int> = some;
            print(some);
        "#,
    );
    module.verify().expect("inferred enum must verify");
    assert_eq!(
        compile_err(
            "generic_enum_ambiguous.nxl",
            r#"
                enum Maybe<T> {
                    Some(T),
                    None
                }
                let none = Maybe.None();
                print(none);
            "#
        ),
        "NXL2014"
    );
}

// ---------------------------------------------------------------------------
// Collections and branch expressions
// ---------------------------------------------------------------------------

#[test]
fn collection_element_type_is_inferred_and_enforced() {
    let module = compile_ok(
        "arrays.nxl",
        "let numbers = [1, 2, 3]; let value: Int = numbers[1]; print(value);",
    );
    module.verify().expect("homogeneous array must verify");
    assert_eq!(
        run("let numbers = [1, 2, 3]; print(numbers[1]);"),
        vec!["2"]
    );
    assert_eq!(
        compile_err(
            "mixed_array.nxl",
            "let values = [1, \"two\"]; print(values[0]);"
        ),
        "NXL2001"
    );
}

#[test]
fn conditional_expression_infers_one_result_type() {
    let module = compile_ok(
        "conditional.nxl",
        r#"
            let number = true ? 10 : 20;
            let text = false ? "yes" : "no";
            print(number);
            print(text);
        "#,
    );
    module
        .verify()
        .expect("conditional expressions must verify");
    assert_eq!(
        run("let a = true ? 10 : 20; let b = false ? \"yes\" : \"no\"; print(a); print(b);"),
        vec!["10", "no"]
    );
    assert_eq!(
        compile_err(
            "conditional_mismatch.nxl",
            "let value = true ? 1 : \"one\"; print(value);"
        ),
        "NXL2001"
    );
}

#[test]
fn nested_conditional_expressions_are_consistent() {
    let module = compile_ok(
        "nested_conditional.nxl",
        r#"
            let value = true ? (false ? 1 : 2) : 3;
            let number: Int = value;
            print(number);
        "#,
    );
    module.verify().expect("nested conditionals must verify");
    assert_eq!(
        run("let value = true ? (false ? 1 : 2) : 3; print(value);"),
        vec!["2"]
    );
}

#[test]
fn conditional_condition_must_be_boolean_or_dynamic() {
    assert_eq!(
        compile_err(
            "conditional_condition.nxl",
            "let value = 1 ? 2 : 3; print(value);"
        ),
        "NXL2001"
    );
}
