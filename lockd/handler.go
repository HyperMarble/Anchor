// Per-connection request/response loop.
package main

import (
	"encoding/json"
	"net"
)

func handleConn(conn net.Conn, mgr *LockManager) {
	defer conn.Close()
	dec := json.NewDecoder(conn)
	enc := json.NewEncoder(conn)
	for {
		var req Request
		if err := dec.Decode(&req); err != nil {
			return // client disconnected or malformed JSON
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
