# Bytecode and Virtual Machine Design

## 1. Purpose

This module defines the bytecode format and virtual machine architecture for NEXORA. The goal is to provide a compact and efficient runtime representation for NXL programs while keeping the design compatible with the NIR and future backends.

## 2. Design Goals

- compact instruction encoding
- deterministic execution
- easy debugging
- support for function calls
- safe runtime values and errors
- compatibility with interpreter and native backends
- support for async and concurrency primitives later on

## 3. Core VM Architecture

The VM is designed around a stack-based execution model with the following major parts:

- instruction stream
- value stack
- call stack
- frame metadata
- object and heap registry
- runtime error handling
- debug metadata table

## 4. Bytecode Format

### 4.1 OpCode Structure

Each instruction is encoded as:

```text
opcode | operand payload (optional)
```

Example:

```text
0x01  LOAD_CONST  5
0x02  LOAD_NAME   "x"
0x03  ADD
0x04  RETURN
```

## 5. Proposed Instruction Set

### Loading and constants

- `LOAD_CONST`
- `LOAD_NAME`
- `STORE_NAME`
- `LOAD_GLOBAL`

### Arithmetic and logic

- `ADD`
- `SUB`
- `MUL`
- `DIV`
- `MOD`
- `EQ`
- `NEQ`
- `LT`
- `LTE`
- `GT`
- `GTE`
- `AND`
- `OR`
- `NOT`

### Control flow

- `JUMP`
- `JUMP_IF_FALSE`
- `CALL`
- `RETURN`
- `HALT`

### Data structure operations

- `NEW_ARRAY`
- `NEW_MAP`
- `INDEX`
- `SET_INDEX`
- `GET_FIELD`
- `SET_FIELD`

### Function and frame operations

- `MAKE_FUNCTION`
- `CALL_FUNCTION`
- `ENTER_FRAME`
- `EXIT_FRAME`

### Runtime safety

- `THROW`
- `TRY`
- `CATCH`

## 6. Runtime Values

Runtime values should include:

- Integer
- Float
- Boolean
- String
- Array
- Map
- Null
- Function object
- Closure
- Native reference

## 7. Stack and Frame Model

```text
CallStackFrame
├── function id
├── locals table
├── instruction pointer
├── return address
├── stack base
└── debug metadata
```

Execution uses:

- operand stack for temporary values
- call stack for function frames
- environment chain for variable lookup

## 8. Function Calls

Function calls follow this pattern:

```text
... arguments on stack
CALL function_id, arg_count
```

The VM pushes a new frame, sets local variables, executes the function body, and restores the caller state on return.

## 9. Error Handling

Runtime errors should be structured as typed runtime exceptions with:

- error code
- message
- source location
- optional nested cause

Examples:

- `TypeError`
- `UndefinedVariable`
- `IndexOutOfRange`
- `InvalidOperation`
- `StackOverflow`

## 10. Debug Information

Bytecode must support optional metadata:

- source file
- line number
- column number
- function names
- block labels

This makes debugging and tooling support possible without changing execution semantics.

## 11. Bytecode Versioning

Bytecode should include a version identifier and compatibility checks on load. This allows VM evolution without breaking older compiled modules.

## 12. Implementation Considerations

The placeholder VM architecture does not yet implement every instruction or optimization. It is designed as a solid foundation for the next implementation phase while remaining aligned with NIR and future runtime targets.

## 13. Summary

The NEXORA bytecode and VM design outlines a compact, typed, debuggable execution environment for NXL programs. It is intentionally designed to be compatible with NIR and extensible for future concurrency, safety, and backend work.
