// Acquire attempts to lock a symbol for the requesting agent.
package main

import "time"

func (m *LockManager) Acquire(req Request) Response {
	if err := validateSymbol(req.Symbol); err != nil {
		return failDetail("invalid_symbol", err.Error())
	}
	if err := validatePath(req.Path); err != nil {
		return failDetail("invalid_path", err.Error())
	}
	if err := validateAgent(req.Agent); err != nil {
		return failDetail("invalid_agent", err.Error())
	}

	ttl := normTTL(req.TTL)
	key := LockKey{Symbol: req.Symbol, Path: req.Path}
	now := time.Now()
	expires := now.Add(time.Duration(ttl) * time.Second)

	m.mu.Lock()
	defer m.mu.Unlock()

	existing, held := m.locks[key]
	switch {
	case !held || now.After(existing.ExpiresAt):
		// not locked or stale — grant
		m.locks[key] = &LockEntry{Owner: req.Agent, AcquiredAt: now, ExpiresAt: expires}
		m.persistLocked()
		return okResp()

	case existing.Owner == req.Agent:
		// same agent re-acquiring — refresh TTL
		existing.ExpiresAt = expires
		m.persistLocked()
		return okResp()

	default:
		// held by someone else
		r := failResp("locked")
		r.Owner = existing.Owner
		r.ExpiresIn = int(time.Until(existing.ExpiresAt).Seconds())
		return r
	}
}
