# Task 3 — Release only acquired locks after failed batch locked writes

## Status

Real bug, but tests must be careful. Do not use sleeps or concurrency.

## Core bug

`batch_replace_locked` attempts to acquire locks for every path. If any acquisition fails, it currently releases locks by iterating over every input path.

That can release a lock that was already held before the batch started.

## Suggested task title

```markdown
# Release only acquired locks after failed batch locked writes
```

## instruction.md

```markdown
# Release only acquired locks after failed batch locked writes

Anchor’s batch locked write flow should not interfere with locks owned by other operations.

When `batch_replace_locked` tries to lock several files and one of them is already locked by another operation, the batch should fail cleanly and release only the locks it acquired during that batch attempt.

It should not release a lock that was already held before the batch started.

## Expected behavior

If a batch write fails while acquiring locks, any locks acquired earlier in that same batch attempt should be released.

Locks that were already held by another operation before the batch started should remain active.

After the failed batch, the originally blocked file should still report as locked, and a second independent attempt to acquire that same lock should still be blocked until the original owner releases it.

Successful batch writes should still release all locks acquired by the batch after the write phase completes.

## Cases that must be handled

A failed batch should release locks acquired earlier in the same batch attempt.

A failed batch should not release the pre-existing lock that caused the failure.

A failed batch should leave the lock manager in a usable state so later independent locks can still be acquired and released.

A successful batch should continue to release all locks it acquired.

A failed batch involving dependency-aware symbol locks should preserve locks owned by the external blocker.

A failed batch should report the blocked path as failed without writing partial changes to files that were not safely locked.

## Constraints

Do not change the public API of `batch_replace_locked`.

Do not weaken dependency-aware locking.

Do not make cleanup depend only on the input path list. Cleanup should release only locks acquired by the current batch attempt.

Do not release locks owned by another primary symbol or another operation.

Do not introduce sleeps, timing assumptions, or thread scheduling assumptions into the tests.

The behavior should be verified through the lock manager’s public behavior and file contents, not by inspecting internal lock maps.
```

## reference_plan.md

```markdown
# Reference plan

## Root cause

`batch_replace_locked` records acquired symbols, but when acquisition fails it releases every input path rather than only the locks acquired by that batch. If one of the input paths was already locked before the batch call, the cleanup path can release the external lock.

## Intended fix

Track the primary lock acquisitions made by the current batch. On failure, release only those acquired locks. Do not release the path that caused the blocked result unless it was actually acquired by this batch.

The successful path should still release all locks acquired by the batch after writes complete.

## Test plan

Use `LockManager` and `CodeGraph` through public APIs. Pre-lock one file, run a batch including an unlocked file and the pre-locked file, then verify:

- the batch fails,
- the lock acquired earlier in the batch is released,
- the external pre-existing lock remains active,
- acquiring the blocked file again is still blocked,
- after explicitly releasing the original lock, acquisition succeeds,
- successful batch writes still release locks.

## Difficulty notes

The task is fair because the user-visible behavior is lock ownership safety. The agent must notice that cleanup by input path is too broad and must use acquisition ownership rather than path presence.
```

## Correct test split

### pass_to_pass

These likely already pass at the base commit:

```json
[
  "successful_batch_releases_all_batch_locks"
]
```

Only include as pass-to-pass after confirming locally.

### fail_to_pass

Use these for the bug:

```json
[
  "failed_batch_releases_locks_acquired_by_that_batch",
  "failed_batch_does_not_release_preexisting_file_lock",
  "failed_batch_keeps_original_blocker_active",
  "failed_batch_allows_blocked_file_after_original_release",
  "failed_batch_does_not_write_unlocked_paths_after_lock_failure"
]
```

## Suggested selected test file

```text
tests/integration_batch_lock_cleanup.rs
```

## Testing notes

Avoid the earlier duplicate-path case unless you verify the desired behavior carefully. Duplicate paths make the spec less clean because it is unclear whether the operation should deduplicate, fail, or treat duplicates as one requested file.

Use direct lock operations:

```text
LockManager::new
LockManager::try_acquire
LockManager::is_locked
LockManager::status
LockManager::release
batch_replace_locked
```

Create temporary files and verify content does not change when batch locking fails.

## Risk review

Medium risk.

Main risks:

- If the test uses `acquire_with_wait`, it may wait too long. Prefer immediate `try_acquire` for setting up blockers.
- Do not use sleeps.
- Do not test private lock maps.
- Avoid duplicate-path semantics unless absolutely necessary.

---
