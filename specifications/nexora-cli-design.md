# NEXORA CLI Design

## 1. Purpose

The NEXORA CLI provides the command-line interface for creating, building, managing, running, testing, and debugging NEXORA applications. It is the developer-facing entry point for the platform.

## 2. Primary Commands

The CLI is expected to support the following commands:

```bash
nxl
nxl new
nxl init
nxl run
nxl build
nxl test
nxl fmt
nxl lint
nxl repl
nxl debug
```

## 3. Command Responsibilities

### `nxl`
Shows the main CLI banner and help output.

### `nxl new`
Creates a new NEXORA project skeleton.

Typical behavior:

- create project dir
- generate config files
- install default project structure
- create initial app entry point
- create package metadata

### `nxl init`
Initializes an existing directory as a NEXORA project.

### `nxl run`
Runs a NEXORA source file or app entry point.

### `nxl build`
Compiles a project or module into a target artifact.

### `nxl test`
Runs the test suite for the project.

### `nxl fmt`
Formats NXL source code according to project formatting rules.

### `nxl lint`
Checks code quality and rule compliance.

### `nxl repl`
Starts the interactive REPL for quick testing.

### `nxl debug`
Runs a debugging or tracing mode for source execution.

## 4. Project Structure Expectations

A generated project should include at least:

```text
project/
├── src/
├── tests/
├── package.nxl
├── nexora.toml
├── README.md
└── .nexora/
```

## 5. Configuration Model

The CLI should read project metadata from a package configuration file. The default configuration should include:

- project name
- language version
- runtime target
- dependencies
- scripts
- environment settings

## 6. Developer Experience Principles

- fast command response
- clear error messages
- project scaffolding by convention
- consistent formatting and lint rules
- debug-friendly output
- support for hot reloading and iterative development

## 7. Execution Lifecycle

```text
nxl command
    ↓
project resolution
    ↓
config parsing
    ↓
compile/run/build/test
    ↓
diagnostics output
```

## 8. Summary

The NEXORA CLI is the primary developer interface for the ecosystem. It provides project scaffolding, execution, formatting, testing, debugging, and build support while keeping the command surface simple and consistent.
