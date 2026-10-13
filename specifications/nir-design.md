# NIR Design Specification

## 1. Purpose

NIR (NEXORA Intermediate Representation) is the canonical type-checked representation of NXL source code. It is the bridge between the frontend language and the eventual execution backends.

## 2. Design Goals

- backend-agnostic representation
- typed program model
- optimization-ready structure
- cross-platform execution support
- explicit safety and memory semantics
- support for async and concurrency boundaries
- strong interoperability contracts with external runtimes

## 3. High-Level Structure

```text
Module
├── name
├── imports
├── exports
├── type_defs
├── functions
├── globals
├── metadata
└── entry_point
```

## 4. Core IR Entities

### Module
A Module contains all top-level declarations.

### Function
A Function contains:

- name
- signature
- return type
- local variables
- body blocks
- metadata

### Block
A Block contains instructions and control-flow edges.

### Instruction
Instructions represent operations such as:

- constants
- allocations
- arithmetic
- comparisons
- calls
- branches
- returns
- awaits
- sends/receives for channels and actors

## 5. Type Model in NIR

NIR must preserve the type semantics of NXL, including:

- primitive types
- arrays
- tuples
- maps
- enums
- structs
- references and borrowing markers
- generics
- optional and result types

## 6. Value Model

Values in NIR include:

- immediate scalar values
- heap-allocated objects
- references
- closures
- tasks or async handles
- channel endpoints
- foreign ABI handles

## 7. Control Flow Model

NIR uses a structured CFG representation, with explicit support for:

- branching
- loops
- function calls
- early returns
- exceptions and error paths

## 8. Safety and Memory Semantics

NIR should carry metadata for:

- ownership state
- borrow validity
- lifetime boundaries
- unsafe block markers
- resource cleanup requirements

This allows backend code generation and runtime tooling to enforce safe execution without losing portability.

## 9. Async and Concurrency Representation

NIR must explicitly model:

- task creation
- await points
- channel send and receive
- actor message dispatch
- cancellation and synchronization

## 10. ABI and Interop Representation

Foreign boundaries must be visible in NIR through explicit bridge declarations, including:

- function signatures
- memory layout requirements
- call conventions
- conversion layers
- error mapping rules

## 11. Target Backends

NIR is designed to target:

- NXL VM
- native machine code
- WASM
- web runtime
- desktop/mobile codegen
- embedded targets

This module defines the design only; actual backend implementation remains future work.

## 12. Summary

NIR is the architectural backbone of the NEXORA platform. It ensures that the language, runtime, and eventual code generators all reason about the same type-safe, platform-neutral execution model.
