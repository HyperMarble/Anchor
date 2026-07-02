// Periodic sweep of expired locks.
package main

import (
	"context"
	"time"
)

const cleanupInterval = 30 * time.Second

func (m *LockManager) cleanupExpired() {
	now := time.Now()
	m.mu.Lock()
	defer m.mu.Unlock()
	removed := 0
	for k, v := range m.locks {
		if now.After(v.ExpiresAt) {
			delete(m.locks, k)
			removed++
		}
	}
	if removed > 0 {
		m.persistLocked()
	}
}

func (m *LockManager) RunCleanup(ctx context.Context) {
	ticker := time.NewTicker(cleanupInterval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			m.cleanupExpired()
		}
	}
}
