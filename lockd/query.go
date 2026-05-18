// Check and List — read-only operations on the lock table.
package main

import "time"

func (m *LockManager) Check(req Request) Response {
	if err := validateSymbol(req.Symbol); err != nil {
		return failDetail("invalid_symbol", err.Error())
	}
	if err := validatePath(req.Path); err != nil {
		return failDetail("invalid_path", err.Error())
	}

	key := LockKey{Symbol: req.Symbol, Path: req.Path}
	now := time.Now()

	m.mu.RLock()
	defer m.mu.RUnlock()

	entry, held := m.locks[key]
	if !held || now.After(entry.ExpiresAt) {
		f := false
		return Response{Locked: &f}
	}

	t := true
	return Response{
		Locked:    &t,
		Owner:     entry.Owner,
		ExpiresIn: int(time.Until(entry.ExpiresAt).Seconds()),
	}
}

func (m *LockManager) List() Response {
	now := time.Now()

	m.mu.RLock()
	defer m.mu.RUnlock()

	var infos []LockInfo
	for k, v := range m.locks {
		if now.After(v.ExpiresAt) {
			continue
		}
		infos = append(infos, LockInfo{
			Symbol:    k.Symbol,
			Path:      k.Path,
			Owner:     v.Owner,
			ExpiresIn: int(time.Until(v.ExpiresAt).Seconds()),
		})
	}
	return Response{Locks: infos}
}
