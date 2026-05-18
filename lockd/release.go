// Release removes a lock held by the requesting agent.
package main

import "time"

func (m *LockManager) Release(req Request) Response {
	if err := validateSymbol(req.Symbol); err != nil {
		return failDetail("invalid_symbol", err.Error())
	}
	if err := validatePath(req.Path); err != nil {
		return failDetail("invalid_path", err.Error())
	}
	if err := validateAgent(req.Agent); err != nil {
		return failDetail("invalid_agent", err.Error())
	}

	key := LockKey{Symbol: req.Symbol, Path: req.Path}

	m.mu.Lock()
	defer m.mu.Unlock()

	existing, held := m.locks[key]
	switch {
	case !held || time.Now().After(existing.ExpiresAt):
		return failResp("not_locked")
	case existing.Owner != req.Agent:
		return failResp("not_owner")
	default:
		delete(m.locks, key)
		return okResp()
	}
}
