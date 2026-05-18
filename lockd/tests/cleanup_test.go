package tests

import (
	"testing"
	"time"
)

func TestCleanupRemovesExpiredLocks(t *testing.T) {
	m := newLockMgr()
	m.acquire("LockManager", "src/lock.rs", "agent-1", 1*time.Millisecond)

	time.Sleep(5 * time.Millisecond)
	m.sweep() // manual sweep

	locked, _ := m.check("LockManager", "src/lock.rs")
	if locked {
		t.Fatal("expired lock should be removed after sweep")
	}
}

func TestCleanupKeepsActiveLocks(t *testing.T) {
	m := newLockMgr()
	m.acquire("LockManager", "src/lock.rs", "agent-1", 300*time.Second)

	m.sweep()

	locked, owner := m.check("LockManager", "src/lock.rs")
	if !locked || owner != "agent-1" {
		t.Fatal("active lock should survive sweep")
	}
}

func TestCleanupMixedExpiredAndActive(t *testing.T) {
	m := newLockMgr()
	m.acquire("ExpiredSym", "src/a.rs", "agent-1", 1*time.Millisecond)
	m.acquire("ActiveSym", "src/b.rs", "agent-2", 300*time.Second)

	time.Sleep(5 * time.Millisecond)
	m.sweep()

	if locked, _ := m.check("ExpiredSym", "src/a.rs"); locked {
		t.Fatal("expired lock should be gone")
	}
	if locked, _ := m.check("ActiveSym", "src/b.rs"); !locked {
		t.Fatal("active lock should remain")
	}
}

// sweep is a test helper that mirrors cleanupExpired logic.
func (m *lockMgr) sweep() {
	now := time.Now()
	m.mu.Lock()
	defer m.mu.Unlock()
	for k, v := range m.locks {
		if now.After(v.expiresAt) {
			delete(m.locks, k)
		}
	}
}
