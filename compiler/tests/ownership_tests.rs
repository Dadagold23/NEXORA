//! Module 2.4 regression tests: ownership and borrowing.
//!
//! NXL keeps value-copy semantics for ordinary assignment. Ownership transfer
//! is explicit (`move x`) and references are explicit (`borrow x` / `ref x`
//! for `&T`, `borrow mut x` / `ref mut x` for `&mut T`).
//!
//! The model implemented here is deliberately lexical and narrower than Rust:
//!
//! * a borrow lasts until the end of the enclosing block;
//! * an immutable borrow permits further immutable borrows and reads of the
//!   owner, but not writes, moves, or mutable borrows;
//! * a mutable borrow is exclusive for its whole scope;
//! * a reference may be read (`*r`), written (`*r = v`), updated in place
//!   (`(*r).field = v`, `(*r)[i] = v`), and passed to a function, but it may
//!   not be returned, stored in a field or enum payload, captured by a closure
//!   or `async` block, or created from a field/index/temporary.
//!
//! These are intentional limitations: NXL has no lifetime parameters, so any
//! rule that would need one (returning `&T`, storing `&T`, self-referential
//! structures) is rejected with a diagnostic instead of being guessed.
//!
//! Passing these tests shows the *implemented rules* are enforced. It is not a
//! proof of memory safety: the compiler reasons about lexical scopes and
//! binding names, not about aliasing through aggregates or FFI.

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

fn run(name: &str, source: &str) -> Vec<String> {
    let module = compile_ok(name, source);
    module
        .verify()
        .unwrap_or_else(|diagnostic| panic!("{name} failed verification: {}", diagnostic.message));
    nxl_compiler::vm::execute(&module)
        .unwrap_or_else(|diagnostic| panic!("{name} failed at runtime: {}", diagnostic.message))
        .output
}

// ---------------------------------------------------------------------------
// Value semantics and explicit ownership transfer
// ---------------------------------------------------------------------------

#[test]
fn assignment_remains_a_value_copy() {
    let output = run(
        "copy.nxl",
        r#"
            let a = 1;
            let b = a;
            print(a + b);
        "#,
    );
    assert_eq!(output, ["2"]);
}

#[test]
fn move_transfers_and_invalidates_the_source() {
    let code = compile_err(
        "use_after_move.nxl",
        r#"
            let a = 1;
            let b = move a;
            print(b);
            print(a);
        "#,
    );
    assert_eq!(code, "NXL2016");
}

#[test]
fn reassignment_restores_a_moved_binding() {
    let output = run(
        "move_reassign.nxl",
        r#"
            let mut a = 1;
            let b = move a;
            a = 2;
            print(a);
            print(b);
        "#,
    );
    assert_eq!(output, ["2", "1"]);
}

#[test]
fn moving_a_borrowed_value_is_rejected() {
    let code = compile_err(
        "move_borrowed.nxl",
        r#"
            let a = 1;
            let r = borrow a;
            let b = move a;
            print(*r);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn moved_value_cannot_be_borrowed() {
    let code = compile_err(
        "borrow_moved.nxl",
        r#"
            let a = 1;
            let b = move a;
            let r = borrow a;
        "#,
    );
    assert_eq!(code, "NXL2016");
}

#[test]
fn function_parameters_receive_copies_unless_moved() {
    let output = run(
        "parameter_copy.nxl",
        r#"
            fn increment(value: Int) -> Int {
                return value + 1;
            }
            let x = 1;
            let y = increment(x);
            print(x);
            print(y);
        "#,
    );
    assert_eq!(output, ["1", "2"]);
}

// ---------------------------------------------------------------------------
// Immutable borrowing
// ---------------------------------------------------------------------------

#[test]
fn immutable_borrow_reads_through_deref() {
    let output = run(
        "borrow_read.nxl",
        r#"
            let x = 10;
            let r = borrow x;
            print(*r);
        "#,
    );
    assert_eq!(output, ["10"]);
}

#[test]
fn ref_is_an_immutable_alias() {
    let output = run(
        "ref_alias.nxl",
        r#"
            let x = 7;
            let r = ref x;
            print(*r + x);
        "#,
    );
    assert_eq!(output, ["14"]);
}

#[test]
fn immutable_borrow_permits_owner_reads() {
    let output = run(
        "borrow_owner_read.nxl",
        r#"
            let x = 3;
            let r = borrow x;
            print(x + *r);
        "#,
    );
    assert_eq!(output, ["6"]);
}

#[test]
fn several_immutable_borrows_coexist() {
    let output = run(
        "two_borrows.nxl",
        r#"
            let x = 3;
            let a = borrow x;
            let b = borrow x;
            print(*a + *b);
        "#,
    );
    assert_eq!(output, ["6"]);
}

#[test]
fn assigning_the_owner_while_borrowed_is_rejected() {
    let code = compile_err(
        "assign_borrowed.nxl",
        r#"
            let mut x = 1;
            let r = borrow x;
            x = 2;
            print(*r);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

// ---------------------------------------------------------------------------
// Mutable borrowing
// ---------------------------------------------------------------------------

#[test]
fn mutable_borrow_writes_are_visible_to_the_owner() {
    let output = run(
        "borrow_mut_write.nxl",
        r#"
            let mut x = 1;
            {
                let r = borrow mut x;
                *r = 5;
                print(*r);
            }
            print(x);
        "#,
    );
    assert_eq!(output, ["5", "5"]);
}

#[test]
fn mutable_borrow_supports_compound_assignment() {
    let output = run(
        "borrow_mut_compound.nxl",
        r#"
            let mut x = 10;
            {
                let r = borrow mut x;
                *r += 5;
                *r *= 2;
            }
            print(x);
        "#,
    );
    assert_eq!(output, ["30"]);
}

#[test]
fn mutable_borrow_while_immutably_borrowed_is_rejected() {
    let code = compile_err(
        "mut_after_immut.nxl",
        r#"
            let mut x = 1;
            let a = borrow x;
            let b = borrow mut x;
            print(*a);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn two_mutable_borrows_conflict() {
    let code = compile_err(
        "two_mutable.nxl",
        r#"
            let mut x = 1;
            let a = borrow mut x;
            let b = borrow mut x;
            print(*a);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn reading_the_owner_while_mutably_borrowed_is_rejected() {
    let code = compile_err(
        "read_while_mut.nxl",
        r#"
            let mut x = 1;
            let r = borrow mut x;
            print(x);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn mutably_borrowing_an_immutable_binding_is_rejected() {
    let code = compile_err(
        "mut_immutable.nxl",
        r#"
            let x = 1;
            let r = borrow mut x;
            print(*r);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn writing_through_an_immutable_reference_is_rejected() {
    let code = compile_err(
        "write_immutable_ref.nxl",
        r#"
            let x = 1;
            let r = borrow x;
            *r = 2;
            print(x);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn rebinding_a_reference_is_rejected() {
    let code = compile_err(
        "rebind_ref.nxl",
        r#"
            let x = 1;
            let y = 2;
            let mut r = borrow x;
            r = borrow y;
            print(*r);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn borrows_end_with_their_block() {
    let output = run(
        "borrow_scope.nxl",
        r#"
            let mut x = 1;
            {
                let r = borrow x;
                print(*r);
            }
            {
                let m = borrow mut x;
                *m = 4;
            }
            print(x);
        "#,
    );
    assert_eq!(output, ["1", "4"]);
}

// ---------------------------------------------------------------------------
// References and functions
// ---------------------------------------------------------------------------

#[test]
fn mutable_reference_parameter_writes_back() {
    let output = run(
        "mut_ref_parameter.nxl",
        r#"
            fn bump(r: &mut Int) {
                *r = *r + 1;
            }
            let mut x = 1;
            {
                bump(borrow mut x);
            }
            print(x);
        "#,
    );
    assert_eq!(output, ["2"]);
}

#[test]
fn immutable_reference_parameter_reads() {
    let output = run(
        "ref_parameter.nxl",
        r#"
            fn double(r: &Int) -> Int {
                return *r * 2;
            }
            let x = 4;
            print(double(borrow x));
        "#,
    );
    assert_eq!(output, ["8"]);
}

#[test]
fn reference_bindings_pass_to_reference_parameters() {
    let output = run(
        "ref_passthrough.nxl",
        r#"
            fn read(r: &Int) -> Int {
                return *r;
            }
            let x = 9;
            let r = borrow x;
            print(read(r));
        "#,
    );
    assert_eq!(output, ["9"]);
}

#[test]
fn writing_a_field_through_a_mutable_reference() {
    let output = run(
        "ref_field_write.nxl",
        r#"
            struct Point { x: Int; y: Int }
            let mut p = Point { x: 1, y: 2 };
            {
                let r = borrow mut p;
                (*r).x = 10;
                print((*r).x);
            }
            print(p.x);
            print(p.y);
        "#,
    );
    assert_eq!(output, ["10", "10", "2"]);
}

#[test]
fn writing_an_element_through_a_mutable_reference() {
    let output = run(
        "ref_index_write.nxl",
        r#"
            let mut xs = [1, 2, 3];
            {
                let r = borrow mut xs;
                (*r)[1] = 9;
                print((*r)[1]);
            }
            print(xs[1]);
            print(xs[0] + xs[2]);
        "#,
    );
    assert_eq!(output, ["9", "9", "4"]);
}

#[test]
fn writing_a_field_through_an_immutable_reference_is_rejected() {
    let code = compile_err(
        "ref_field_write_immutable.nxl",
        r#"
            struct Point { x: Int }
            let p = Point { x: 1 };
            let r = borrow p;
            (*r).x = 2;
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn nested_paths_through_a_reference_are_rejected() {
    // `(*r).inner.x = v` would need path-sensitive write-back; rejecting it
    // is better than silently dropping the update.
    let code = compile_err(
        "ref_nested_write.nxl",
        r#"
            struct Inner { x: Int }
            struct Outer { inner: Inner }
            let mut o = Outer { inner: Inner { x: 1 } };
            let r = borrow mut o;
            (*r).inner.x = 2;
        "#,
    );
    assert_eq!(code, "NXL2007");
}

// ---------------------------------------------------------------------------
// Escape rejection and intentionally unsupported positions
// ---------------------------------------------------------------------------

#[test]
fn returning_a_reference_annotation_is_rejected() {
    let code = compile_err(
        "return_ref_annotation.nxl",
        r#"
            fn leak(r: &Int) -> &Int {
                return r;
            }
        "#,
    );
    assert_eq!(code, "NXL2017");
}

#[test]
fn returning_a_local_borrow_is_rejected() {
    let code = compile_err(
        "return_local_borrow.nxl",
        r#"
            fn leak() {
                let x = 1;
                let r = borrow x;
                return r;
            }
        "#,
    );
    assert_eq!(code, "NXL2017");
}

#[test]
fn reference_typed_fields_are_rejected() {
    let code = compile_err(
        "ref_field.nxl",
        r#"
            struct Holder {
                value: &Int
            }
        "#,
    );
    assert_eq!(code, "NXL2017");
}

#[test]
fn closures_cannot_capture_references() {
    let code = compile_err(
        "ref_closure.nxl",
        r#"
            let x = 1;
            let r = borrow x;
            let f = fn () -> Int { return *r };
            print(f());
        "#,
    );
    assert_eq!(code, "NXL2017");
}

#[test]
fn nested_reference_annotations_are_unsupported() {
    // References are lexical and non-owning. A container of references would
    // need a lifetime to describe how long its elements stay valid, which NXL
    // deliberately does not have, so the annotation is rejected.
    let code = compile_err(
        "nested_ref_annotation.nxl",
        r#"
            let x = 1;
            let xs: [&Int] = [];
            print(*xs[0]);
        "#,
    );
    assert_eq!(code, "NXL2004");
}

#[test]
fn closures_cannot_capture_moved_values() {
    let code = compile_err(
        "closure_moved.nxl",
        r#"
            let a = 1;
            let b = move a;
            let f = fn () -> Int { return a };
            print(b);
        "#,
    );
    assert_eq!(code, "NXL2016");
}

#[test]
fn async_blocks_cannot_capture_references() {
    let code = compile_err(
        "ref_async.nxl",
        r#"
            let x = 1;
            let r = borrow x;
            let t = async { return *r; };
            print(await t);
        "#,
    );
    assert_eq!(code, "NXL2017");
}

#[test]
fn array_literals_cannot_hold_references() {
    let code = compile_err(
        "ref_array_literal.nxl",
        r#"
            let x = 1;
            let xs = [borrow x];
        "#,
    );
    assert_eq!(code, "NXL2017");
}

#[test]
fn references_cannot_be_stored_into_an_element() {
    let code = compile_err(
        "ref_element_store.nxl",
        r#"
            let mut xs = [0];
            let x = 1;
            xs[0] = borrow x;
        "#,
    );
    assert_eq!(code, "NXL2017");
}

#[test]
fn borrowing_a_field_is_not_supported() {
    let code = compile_err(
        "borrow_field.nxl",
        r#"
            struct Box {
                value: Int
            }
            let b = Box { value: 1 };
            let r = borrow b.value;
            print(*r);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn borrowing_a_temporary_is_not_supported() {
    let code = compile_err(
        "borrow_temporary.nxl",
        r#"
            let r = borrow 5;
            print(*r);
        "#,
    );
    assert_eq!(code, "NXL2015");
}

#[test]
fn dereferencing_a_non_reference_is_rejected() {
    let code = compile_err(
        "deref_int.nxl",
        r#"
            let x = 1;
            print(*x);
        "#,
    );
    assert_eq!(code, "NXL2001");
}

#[test]
fn borrow_instructions_are_emitted() {
    let module = compile_ok(
        "borrow_lowering.nxl",
        r#"
            let mut x = 1;
            let r = borrow mut x;
            *r = 3;
        "#,
    );
    module.verify().expect("borrow program must verify");
    let instructions: Vec<&Instruction> = module
        .functions
        .iter()
        .flat_map(|function| function.basic_blocks.iter())
        .flat_map(|block| block.instructions.iter())
        .collect();
    assert!(instructions
        .iter()
        .any(|instruction| matches!(instruction, Instruction::Allocate { .. })));
    assert!(instructions
        .iter()
        .any(|instruction| matches!(instruction, Instruction::Borrow { mutable: true, .. })));
    assert!(instructions
        .iter()
        .any(|instruction| matches!(instruction, Instruction::StoreMemory { .. })));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::Borrow {
            ty: Type::Reference(_, true),
            ..
        }
    )));
}

// ---------------------------------------------------------------------------
// Module 2.5 — assignment place type checking
// ---------------------------------------------------------------------------

#[test]
fn struct_field_assignment_must_match_the_field_type() {
    assert_eq!(
        compile_err(
            "field_assignment_mismatch.nxl",
            r#"
                struct Point { x: Int }
                let mut p = Point { x: 1 };
                p.x = "text";
                print(p.x);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn generic_struct_field_assignment_must_match_the_instantiated_type() {
    assert_eq!(
        compile_err(
            "generic_field_assignment_mismatch.nxl",
            r#"
                struct Box<T> { value: T }
                let mut b: Box<Int> = Box<Int> { value: 1 };
                b.value = "text";
                print(b.value);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn array_element_assignment_must_match_the_element_type() {
    assert_eq!(
        compile_err(
            "element_assignment_mismatch.nxl",
            r#"
                let mut xs = [1, 2];
                xs[0] = "text";
                print(xs[0]);
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn assignment_through_a_mutable_reference_field_must_match_the_field_type() {
    assert_eq!(
        compile_err(
            "reference_field_assignment_mismatch.nxl",
            r#"
                struct Box<T> { value: T }
                let mut a = Box { value: 5 };
                let r = borrow mut a;
                (*r).value = "text";
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn write_through_a_mutable_reference_is_checked_at_the_assignment() {
    // The mismatch is reported as a type error where it is written, instead
    // of surfacing later as a verifier failure.
    assert_eq!(
        compile_err(
            "reference_write_mismatch.nxl",
            r#"
                let mut n = 1;
                let r = borrow mut n;
                *r = "text";
            "#
        ),
        "NXL2001"
    );
}

#[test]
fn matching_place_assignments_still_compile_and_run() {
    let output = run(
        "place_assignments.nxl",
        r#"
            struct Point { x: Int }
            struct Box<T> { value: T }
            let mut p = Point { x: 1 };
            p.x = 2;
            let mut b: Box<Int> = Box<Int> { value: 3 };
            b.value = 4;
            let mut xs = [5, 6];
            xs[0] = 7;
            {
                let r = borrow mut b;
                (*r).value = 8;
            }
            print(p.x + b.value + xs[0]);
        "#,
    );
    assert_eq!(output, vec!["17"]);
}

#[test]
fn generic_reference_argument_infers_from_a_local_borrow() {
    let output = run(
        "generic_reference_argument.nxl",
        r#"
            fn read_value<T>(x: &T) -> T { return *x; }
            let n = 41;
            print(read_value(borrow n) + 1);
        "#,
    );
    assert_eq!(output, vec!["42"]);
}

#[test]
fn assigning_a_reference_into_a_field_still_reports_escape() {
    // The reference-escape rule keeps precedence over the place type check.
    assert_eq!(
        compile_err(
            "reference_into_field.nxl",
            r#"
                struct Holder { value: Int }
                let mut h = Holder { value: 1 };
                let x = 2;
                h.value = borrow x;
                print(h.value);
            "#
        ),
        "NXL2017"
    );
}
