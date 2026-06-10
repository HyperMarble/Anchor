// Central lock state — struct definitions and constructor only.
package main

import (
	"sync"
	"time"
)

// LockKey identifies what is being locked.
type LockKey struct {
	Symbol string
	Path   string
}

// LockEntry records who holds a lock and when it expires.
type LockEntry struct {
	Owner      string
	AcquiredAt time.Time
	ExpiresAt  time.Time
}

// LockManager holds all active locks behind a single RWMutex.
// Reads (check, list) hold RLock; writes (acquire, release) hold Lock.
type LockManager struct {
	mu          sync.RWMutex
	locks       map[LockKey]*LockEntry
	persistPath string
}

func NewLockManager() *LockManager {
	return &LockManager{locks: make(map[LockKey]*LockEntry)}
}
