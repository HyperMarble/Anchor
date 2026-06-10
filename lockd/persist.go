// Lock state persistence: snapshots the lock table to disk on every mutation
// and restores it on startup, so a daemon restart does not silently drop the
// locks coordinating live agent sessions. Writes are atomic (temp + rename).
package main

import (
	"encoding/json"
	"log"
	"os"
	"time"
)

// persistedLock is the on-disk form of one lock table entry.
type persistedLock struct {
	Symbol     string    `json:"symbol"`
	Path       string    `json:"path"`
	Owner      string    `json:"owner"`
	AcquiredAt time.Time `json:"acquired_at"`
	ExpiresAt  time.Time `json:"expires_at"`
}

// SetPersistPath enables persistence and restores any previous snapshot,
// dropping entries that expired while the daemon was down.
func (m *LockManager) SetPersistPath(path string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.persistPath = path

	raw, err := os.ReadFile(path)
	if err != nil {
		if !os.IsNotExist(err) {
			log.Printf("lock state restore failed: %v", err)
		}
		return
	}
	var entries []persistedLock
	if err := json.Unmarshal(raw, &entries); err != nil {
		log.Printf("lock state restore failed (corrupt snapshot): %v", err)
		return
	}

	now := time.Now()
	restored := 0
	for _, entry := range entries {
		if now.After(entry.ExpiresAt) {
			continue
		}
		key := LockKey{Symbol: entry.Symbol, Path: entry.Path}
		m.locks[key] = &LockEntry{
			Owner:      entry.Owner,
			AcquiredAt: entry.AcquiredAt,
			ExpiresAt:  entry.ExpiresAt,
		}
		restored++
	}
	if restored > 0 {
		log.Printf("restored %d active lock(s) from %s", restored, path)
	}
}

// persistLocked snapshots the lock table. Callers must hold m.mu.
func (m *LockManager) persistLocked() {
	if m.persistPath == "" {
		return
	}
	entries := make([]persistedLock, 0, len(m.locks))
	for key, entry := range m.locks {
		entries = append(entries, persistedLock{
			Symbol:     key.Symbol,
			Path:       key.Path,
			Owner:      entry.Owner,
			AcquiredAt: entry.AcquiredAt,
			ExpiresAt:  entry.ExpiresAt,
		})
	}
	raw, err := json.Marshal(entries)
	if err != nil {
		log.Printf("lock state snapshot failed: %v", err)
		return
	}
	tmp := m.persistPath + ".tmp"
	if err := os.WriteFile(tmp, raw, 0o600); err != nil {
		log.Printf("lock state snapshot failed: %v", err)
		return
	}
	if err := os.Rename(tmp, m.persistPath); err != nil {
		log.Printf("lock state snapshot failed: %v", err)
	}
}
