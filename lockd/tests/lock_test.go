package tests

import (
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// --- minimal in-process mirror of the lock logic for white-box testing ---
// (keeps tests self-contained without importing the main package)

type lockKey struct{ symbol, path string }

type lockEntry struct {
	owner     string
	expiresAt time.Time
}

type lockMgr struct {
	mu    sync.RWMutex
	locks map[lockKey]*lockEntry
}

func newLockMgr() *lockMgr { return &lockMgr{locks: make(map[lockKey]*lockEntry)} }

func (m *lockMgr) acquire(symbol, path, agent string, ttl time.Duration) (ok bool, code, owner string) {
	key := lockKey{symbol, path}
	now := time.Now()
	m.mu.Lock()
	defer m.mu.Unlock()
	e, held := m.locks[key]
	switch {
	case !held || now.After(e.expiresAt):
		m.locks[key] = &lockEntry{owner: agent, expiresAt: now.Add(ttl)}
		return true, "", ""
	case e.owner == agent:
		e.expiresAt = now.Add(ttl)
		return true, "", ""
	default:
		return false, "locked", e.owner
	}
}

func (m *lockMgr) release(symbol, path, agent string) (ok bool, code string) {
	key := lockKey{symbol, path}
	m.mu.Lock()
	defer m.mu.Unlock()
	e, held := m.locks[key]
	switch {
	case !held || time.Now().After(e.expiresAt):
		return false, "not_locked"
	case e.owner != agent:
		return false, "not_owner"
	default:
		delete(m.locks, key)
		return true, ""
	}
}

func (m *lockMgr) check(symbol, path string) (locked bool, owner string) {
	key := lockKey{symbol, path}
	m.mu.RLock()
	defer m.mu.RUnlock()
	e, held := m.locks[key]
	if !held || time.Now().After(e.expiresAt) {
		return false, ""
	}
	return true, e.owner
}

// --- tests ---

func TestAcquireGrant(t *testing.T) {
	m := newLockMgr()
	ok, code, _ := m.acquire("LockManager", "src/lock.rs", "agent-1", 300*time.Second)
	if !ok || code != "" {
		t.Fatalf("expected grant, got code=%q", code)
	}
}

func TestAcquireBlockedByOther(t *testing.T) {
	m := newLockMgr()
	m.acquire("LockManager", "src/lock.rs", "agent-1", 300*time.Second)
	ok, code, owner := m.acquire("LockManager", "src/lock.rs", "agent-2", 300*time.Second)
	if ok || code != "locked" || owner != "agent-1" {
		t.Fatalf("expected locked by agent-1, got ok=%v code=%q owner=%q", ok, code, owner)
	}
}

func TestAcquireIdempotentSameOwner(t *testing.T) {
	m := newLockMgr()
	m.acquire("LockManager", "src/lock.rs", "agent-1", 300*time.Second)
	ok, code, _ := m.acquire("LockManager", "src/lock.rs", "agent-1", 300*time.Second)
	if !ok || code != "" {
		t.Fatal("same owner re-acquire should succeed")
	}
}

func TestAcquireStaleAutoEvict(t *testing.T) {
	m := newLockMgr()
	m.acquire("LockManager", "src/lock.rs", "agent-1", 1*time.Millisecond)
	time.Sleep(5 * time.Millisecond)
	ok, code, _ := m.acquire("LockManager", "src/lock.rs", "agent-2", 300*time.Second)
	if !ok || code != "" {
		t.Fatal("stale lock should be evicted and new owner granted")
	}
}

func TestReleaseSuccess(t *testing.T) {
	m := newLockMgr()
	m.acquire("LockManager", "src/lock.rs", "agent-1", 300*time.Second)
	ok, code := m.release("LockManager", "src/lock.rs", "agent-1")
	if !ok || code != "" {
		t.Fatalf("release failed: %q", code)
	}
	locked, _ := m.check("LockManager", "src/lock.rs")
	if locked {
		t.Fatal("symbol should be unlocked after release")
	}
}

func TestReleaseNotOwner(t *testing.T) {
	m := newLockMgr()
	m.acquire("LockManager", "src/lock.rs", "agent-1", 300*time.Second)
	ok, code := m.release("LockManager", "src/lock.rs", "agent-2")
	if ok || code != "not_owner" {
		t.Fatalf("expected not_owner, got ok=%v code=%q", ok, code)
	}
}

func TestReleaseNotLocked(t *testing.T) {
	m := newLockMgr()
	ok, code := m.release("LockManager", "src/lock.rs", "agent-1")
	if ok || code != "not_locked" {
		t.Fatalf("expected not_locked, got ok=%v code=%q", ok, code)
	}
}

func TestCheckReflectsState(t *testing.T) {
	m := newLockMgr()
	locked, _ := m.check("LockManager", "src/lock.rs")
	if locked {
		t.Fatal("should not be locked before acquire")
	}
	m.acquire("LockManager", "src/lock.rs", "agent-1", 300*time.Second)
	locked, owner := m.check("LockManager", "src/lock.rs")
	if !locked || owner != "agent-1" {
		t.Fatalf("expected locked by agent-1, got locked=%v owner=%q", locked, owner)
	}
}

func TestIndependentSymbolsNoConflict(t *testing.T) {
	m := newLockMgr()
	ok1, _, _ := m.acquire("SymbolA", "src/a.rs", "agent-1", 300*time.Second)
	ok2, _, _ := m.acquire("SymbolB", "src/b.rs", "agent-2", 300*time.Second)
	if !ok1 || !ok2 {
		t.Fatal("independent symbols should both be acquirable")
	}
}

// BenchmarkConcurrentAcquire measures lock throughput under N parallel agents
// each racing for distinct symbols — no contention, pure throughput.
func BenchmarkConcurrentAcquire(b *testing.B) {
	for _, agents := range []int{1, 2, 4, 8, 16} {
		b.Run(fmt.Sprintf("agents=%d", agents), func(b *testing.B) {
			m := newLockMgr()
			var ops atomic.Int64
			b.ResetTimer()
			b.RunParallel(func(pb *testing.PB) {
				id := int(ops.Add(1)) % agents
				sym := fmt.Sprintf("Symbol%d", id)
				agent := fmt.Sprintf("agent-%d", id)
				for pb.Next() {
					m.acquire(sym, "src/lib.rs", agent, 300*time.Second)
					m.release(sym, "src/lib.rs", agent)
				}
			})
		})
	}
}

// BenchmarkContentionAcquire measures worst case — all agents fighting for same symbol.
func BenchmarkContentionAcquire(b *testing.B) {
	for _, agents := range []int{2, 4, 8} {
		b.Run(fmt.Sprintf("agents=%d", agents), func(b *testing.B) {
			m := newLockMgr()
			var counter atomic.Int64
			b.ResetTimer()
			b.RunParallel(func(pb *testing.PB) {
				id := fmt.Sprintf("agent-%d", counter.Add(1))
				for pb.Next() {
					if ok, _, _ := m.acquire("SharedSymbol", "src/lib.rs", id, 300*time.Second); ok {
						m.release("SharedSymbol", "src/lib.rs", id)
					}
				}
			})
		})
	}
}
