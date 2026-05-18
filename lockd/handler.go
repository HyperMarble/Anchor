// Per-connection request/response loop.
package main

import (
	"encoding/json"
	"io"
	"net"
	"time"
)

const (
	idleTimeout    = 30 * time.Second
	maxRequestSize = 1 << 20 // 1MB — stops memory exhaustion from oversized payloads
)

func handleConn(conn net.Conn, mgr *LockManager) {
	defer conn.Close()
	dec := json.NewDecoder(io.LimitReader(conn, maxRequestSize))
	enc := json.NewEncoder(conn)
	for {
		// reset deadline before every read — kills idle/stuck connections
		conn.SetReadDeadline(time.Now().Add(idleTimeout))
		var req Request
		if err := dec.Decode(&req); err != nil {
			return // disconnected, deadline exceeded, or malformed JSON
		}
		enc.Encode(dispatch(req, mgr)) //nolint:errcheck
	}
}

func dispatch(req Request, mgr *LockManager) Response {
	switch req.Op {
	case "acquire":
		return mgr.Acquire(req)
	case "release":
		return mgr.Release(req)
	case "check":
		return mgr.Check(req)
	case "list":
		return mgr.List()
	case "ping":
		return pongResp()
	default:
		return failResp("unknown_op")
	}
}
