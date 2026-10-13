# NEXORA Cross-Platform Strategy

## 1. Goal

NEXORA aims to run the same application model across different environments without requiring developers to rewrite logic for each target platform.

## 2. Target Environments

The initial platform targets include:

- native operating systems
- web browsers
- server environments
- mobile operating systems
- desktop environments
- embedded systems
- cloud service backends

## 3. Cross-Platform Architectural Rule

The source model must stay target-independent as much as possible. The compiler and runtime should lower programs to NIR first, then select the correct backend.

## 4. Platform Abstraction Layers

```text
Application Logic (NXL)
         ↓
       NIR
      / | \
     /  |  \
Native WASM Web/Server Mobile/Desktop Embedded
```

## 5. Runtime Backends

Expected backend categories:

- NXL native runtime
- Web runtime
- WASM execution host
- Mobile platform adapters
- Desktop platform adapters
- Embedded execution environments

## 6. UI Strategy

NUI provides a shared application model for UI composition and state while delegating rendering to specific platform adapters.

This allows:

- shared logic for business rules and state
- platform-specific rendering implementations
- a single design language for component structure

## 7. Device and OS Integration

Cross-platform execution must support:

- file access and permissions
- networking and sockets
- notifications and OS services
- hardware features where relevant
- cloud integrations

## 8. Performance Tradeoffs

NEXORA must balance:

- native speed for compute-heavy paths
- portability across environments
- consistent developer ergonomics
- safe execution boundaries

## 9. Packaging and Deployment

The platform should provide packaging targets for:

- CLI executables
- web applications
- mobile app bundles
- desktop installs
- containers or server artifacts
- cloud deployments

## 10. Summary

The cross-platform strategy is designed to make NEXORA a universal environment for building once and deploying across many operating contexts, while preserving tight platform integration where needed.
