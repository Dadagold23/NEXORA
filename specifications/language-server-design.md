# NEXORA Language Server (NLS) Design

## 1. Purpose

NLS is the language tooling layer for NEXORA. It provides IDE and editor integration for comprehension, diagnostics, refactoring, and developer productivity across NXL projects.

## 2. Core Features

- autocomplete
- go-to-definition
- find references
- diagnostics
- refactoring
- formatting
- hover documentation
- symbol navigation

## 3. Service Architecture

```text
NLS
├── parser and AST integration
├── symbol index
├── type information provider
├── diagnostics engine
├── completion engine
├── definition resolver
├── refactoring engine
├── formatter and linter adapters
├── hover/documentation provider
└── IDE protocol adapter
```

## 4. Features

### Autocomplete
Suggest identifiers, members, keywords, and imports derived from the project symbol graph.

### Go-to-definition
Resolve symbols to their declarations and provide navigation across files and modules.

### Find references
Locate all uses of a symbol in the active project.

### Diagnostics
Surface semantic, syntax, and type-checking issues with precise source locations.

### Refactoring
Provide routine actions such as rename, extract function, and consistent symbol updates.

### Formatting
Keep source formatting consistent with NEXORA conventions.

### Hover documentation
Provide type, member, and function description information to the editor.

### Symbol navigation
Expose modules, functions, types, and symbols to IDEs and tooling.

## 5. Summary

The NEXORA Language Server is a foundational developer-tooling feature that makes NEXORA practical for large-scale teams, modern IDEs, and iterative application development.
