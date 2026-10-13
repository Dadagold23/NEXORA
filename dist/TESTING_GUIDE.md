# TESTING GUIDE - P07 Backend (User Acceptance Testing)

## Prerequisites
- Rust toolchain (cargo) available
- Repository at C:\xampp\htdocs\nxl\compiler

## Level 1 - Automated (already passed)
cd compiler && cargo test --test compiler_tests -> expect 267 passed

## Level 2 - Engineering Verification
1. Build: cd compiler && cargo build --release
2. Check native emission:
`ash
cd compiler
cargo test --test compiler_tests 2>&1 | tail -2
`

## Level 3 - User Acceptance Testing

### Test A: Simple program
`ust
let x = 42; print(x);
`
Compile via API: compile_native should return target="native" and source containing:
- // NEXORA Rust backend
- n main() {
- println! or print handling (emits print() wrapper in subset)

### Test B: Function + arithmetic
`ust
fn add(a: Int, b: Int) -> Int { return a + b; }
let r = add(1,2); print(r);
`
Expect: n add(a: i64, b: i64) -> i64 { and n main() {

### Expected Results
All assertions above hold. No test failures.

## Approval
If all pass, reply: APPROVED to proceed to next module.
