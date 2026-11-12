---
date: 2026-11-12
status: proposed
---

# Drain the composition dispatcher before thread exit

## Context

The Windows exit gate can fail after native owner cleanup finishes.
The Composition controller remains in thread-local storage without explicit shutdown.
Microsoft's [current-thread contract](https://learn.microsoft.com/en-us/windows/win32/api/dispatcherqueue/nf-dispatcherqueue-createdispatcherqueuecontroller) requires ShutdownQueueAsync and continued message pumping before thread exit.

## Decision

Release native windows and graphics owners before shutting down the owned dispatcher.
Pump messages until shutdown completes, with a 750-millisecond deadline and a typed failure.
Run teardown after successful and failed runtime sessions.
Preserve the primary failure and report any cleanup failure.
Keep the one-second process exit gate.

## Alternatives

Releasing the thread-local controller alone violates the queue contract.
A blocking wait prevents the current thread from dispatching shutdown work.
Stopping the queue before releasing graphics can strand their cleanup callbacks.

## Consequences

Runtime teardown owns a bounded message pump.
Reconsider if Composition moves to a dedicated thread or another component owns the dispatcher.

## Confirmation

Require queued-work, shutdown-callback, failure-path, and native process-exit checks.
Queued-work, shutdown-callback, and failure-path checks pass on Windows.
The hosted process-exit check and owner acceptance remain pending.
