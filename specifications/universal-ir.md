# NEXORA Universal Intermediate Representation (NIR)

## 1. Purpose

NIR is the canonical intermediate representation for all NXL programs. It decouples language syntax and semantic analysis from target execution environments, enabling NXL code to target native, WASM, web, mobile, desktop, and embedded backends.

## 2. Design Goals

- Target-independent representation of program logic
- Explicit type and memory semantics
- Support for native, web, and foreign ABI backends
- Optimizable representation for future compiler passes
- Strong support for concurrency and async behavior
- Clear boundaries between safe and unsafe code

## 3. Core Principles

NIR should be:

- typed
- structured
- explicit about control flow
- capable of representing modules and functions
- resilient to source-language variations
- extensible for future runtime backends

## 4. Structural Model

A program in NIR is composed of:

- modules
- imports and exports
- type definitions
- functions and methods
- globals and constants
- control-flow graphs
- data structures
- metadata for source locations and diagnostics

## 5. High-Level IR Shape

```text
Module
 ├── name
 ├── imports
 ├── exports
 ├── types
 ├── functions
 ├── globals
 └── metadata

Function
 ├── name
 ├── signature
 ├── locals
 ├── control flow graph
 ├── basic blocks
 ├── instructions
 └── metadata
```

## 6. Recommended Execution Semantics

NIR should support at least these concepts:

- values and references
- function calls
- branching and loops
- aggregate data types
- async boundaries
- memory access and lifetime markers
- foreign interop calls
- exception/error paths

## 7. Type Model in NIR

NIR should represent:

- primitive types
- structs and enums
- arrays and collections
- function types
- references and pointers
- generics and instantiated specializations
- optional and result types
- database and network-aware types

## 8. Memory and Safety Metadata

NIR should reflect safety boundaries across:

- owned values
- borrowed references
- temporary allocations
- unsafe regions
- resource cleanup
- lifetime markers

This lets the runtime and compiler enforce safe behavior without requiring the source language to be lowered to a single target-specific representation too early.

## 9. Async and Concurrency Representation

NIR should define concurrency concepts as first-class constructs:

- task boundaries
- await points
- synchronization primitives
- actor/message boundaries
- channel operations
- cancellation signals

## 10. Foreign ABI Model

NIR must support explicit interop definitions for:

- native libraries
- C ABI calls
- JS/wasm host boundaries
- Python object integration
- Rust FFI contracts
- service APIs and RPC interfaces

## 11. Optimization and Codegen Pipeline

The IR is intended to support:

- inlining and simplification
- dead-code elimination
- type specialization
- backend-specific lowering
- target-specific machine code generation

## 12. NIR Versioning

NIR requires its own stability contract because language and runtime versions are not always synchronized. The project should manage separate version numbers for:

- source language
- compiler frontend
- NIR format
- runtime execution model
- backend generation

## 13. Summary

NIR is the architectural anchor for NEXORA. It defines the portable, typed execution core that allows source programs to be reasoned about uniformly before code generation or loading into a runtime.
