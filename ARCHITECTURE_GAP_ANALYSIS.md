# Architecture Gap Analysis

## Goal

Compare the prototype implementation against the architecture and production requirements described in the NEXORA specifications.

## Key gaps

### 1. Prototype implementation is not a compiler production pipeline

The repository contains a Python interpreter and parser pipeline, but no production codegen stage, no optimizer, and no native/WASM backend. The specification requires a long compiler pipeline with semantic analysis, HIR/NIR lowering, optimization, and real target backends.

### 2. Safety model is still aspirational

The project documents ownership, borrowing, lifetimes, and unsafe boundaries, but the actual implementation does not enforce them. The current interpreter accepts a small language subset without real safety guarantees.

### 3. Multiple subsystem docs exist without corresponding implementation

There are design docs for database, web framework, LSP, IDE, package manager, FFI, UI, security, and cloud systems, but none are implemented in the source tree.

### 4. CLI is minimal

The CLI supports only a basic `run` flow. It does not yet provide the broad build/test/lint/package workflow described by the specification.

### 5. No production build system

There is no package manifest, build toolchain, lockfile, installer packaging, or CI pipeline to move the prototype toward release readiness.

### 6. No real platform maturity policy

Cross-platform strategy exists in docs, but there are no toolchains, build scripts, or test evidence proving platform support.

### 7. No supply-chain or package trust model

The package-manager and registry specifications describe a mature trust and security model, but the code does not implement package signatures, dependency verification, or provenance handling.

### 8. No real security enforcement

The runtime and compiler do not yet implement the required security controls around FFI, unsafe execution, auth, database access, or secret management.

## Comparison summary

| Spec area | Prototype reality | Gap |
| --- | --- | --- |
| Lexer | Present | Small |
| Parser | Present | Small |
| Semantic/type analysis | Partial | Medium |
| Compiler backend | Absent | Large |
| NIR | Design only | Large |
| Runtime | Prototype interpreter | Large |
| VM | Not implemented | Large |
| Database engine | Not implemented | Large |
| Web framework | Not implemented | Large |
| Package manager | Not implemented | Large |
| Security | Planned/design only | Large |
| IDE/LSP | Not implemented | Large |
| Release pipeline | Not implemented | Large |

## Recommendation

The project should enter a disciplined architecture reconciliation and productionization phase. The immediate next step is to reconcile actual implementation against design requirements and define the minimal genuine roadmap to real compiler/runtime maturity.
