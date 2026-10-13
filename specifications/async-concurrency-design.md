# Async and Concurrency Design

## 1. Purpose

This module defines the NEXORA async runtime and concurrency model. The system is designed to support high-performance, safe, and scalable application execution across web, cloud, database, distributed, and real-time workloads.

## 2. Goals

- first-class async execution
- safe concurrency primitives
- task scheduling and cancellation
- shared-memory safety
- channel and actor communication
- low-overhead runtime design
- support for future distributed systems

## 3. Core Concepts

The platform introduces the following primitives:

- `async` functions
- `await` points
- `spawn` for task creation
- `channel` communication
- `actor` model for isolated state
- `parallel` execution boundaries
- cancellation signals

## 4. Runtime Architecture

```text
Async Runtime
├── scheduler
├── executor threads or workers
├── task queue
├── await registry
├── cancellation manager
├── channel registry
├── actor mailbox system
└── runtime metrics and observability
```

## 5. Task Model

Tasks are lightweight units of execution with:

- task id
- state (pending, running, waiting, completed, cancelled)
- parent task metadata
- cancellation token
- stack or coroutine context
- runtime priority and scheduling hints

## 6. Scheduling Model

The runtime should support:

- cooperative scheduling for async tasks
- work-stealing or worker-based distribution for parallelism
- backpressure management for queue-heavy workloads
- low-latency wakeups on IO completion and timer events

## 7. Async Semantics

```nxl
async fn fetchUser(id: Int) -> User {
    let client = await http.connect()
    let user = await client.get(id)
    return user
}
```

The runtime guarantees that:

- `await` yields control efficiently
- task state remains structured
- cancellation is observable and safe
- nested async operations remain predictable

## 8. Channel System

Channels support message-based communication between tasks and actors.

```nxl
let ch = channel<Int>()
spawn async {
    ch.send(42)
}
let value = await ch.recv()
```

Channels provide:

- sender/receiver endpoints
- bounded or unbounded capacity
- backpressure-aware send semantics
- cancellation-aware receive operations

## 9. Actor Model

Actors provide isolated state and mailbox-driven communication:

```nxl
actor Counter {
    let value: Int = 0

    fn increment() {
        self.value += 1
    }
}
```

Actor guarantees:

- messages are processed serially
- state is isolated inside the actor
- actor restart/cancellation is explicit
- message handling is non-blocking where supported

## 10. Synchronization Primitives

The runtime should support:

- mutexes
- rwlocks
- semaphores
- barriers
- condition variables
- atomic operations for selected primitives

These are provided only when the safety and ownership rules permit them.

## 11. Cancellation and Error Handling

Cancellation must be:

- explicit and observable
- safe for async tasks and actors
- integrated with runtime exceptions and structured errors

The runtime should provide cancellation propagation and cleanup hooks for awaiting tasks and actor mailbox work.

## 12. Distributed and Parallel Considerations

The async runtime is designed to evolve toward:

- distributed scheduling
- remote actors or process boundaries
- network-aware task placement
- cluster-level orchestration for NEXORA services

## 13. Summary

The async and concurrency design for NEXORA is built around structured scheduling, explicit communication, actor isolation, and safe resource boundaries. It is designed to scale from local task execution to larger distributed systems without breaking the language’s safety and portability goals.
