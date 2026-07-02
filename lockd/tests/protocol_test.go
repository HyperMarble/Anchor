package tests

import (
	"encoding/json"
	"testing"
)

// mirrors the types in protocol.go so tests stay in a separate package
type request struct {
	Op     string `json:"op"`
	Symbol string `json:"symbol,omitempty"`
	Path   string `json:"path,omitempty"`
	Agent  string `json:"agent,omitempty"`
	TTL    int    `json:"ttl,omitempty"`
}

type response struct {
	Ok        *bool      `json:"ok,omitempty"`
	Code      string     `json:"code,omitempty"`
	Detail    string     `json:"detail,omitempty"`
	Owner     string     `json:"owner,omitempty"`
	ExpiresIn int        `json:"expires_in,omitempty"`
	Locked    *bool      `json:"locked,omitempty"`
	Locks     []lockInfo `json:"locks,omitempty"`
	Pong      *bool      `json:"pong,omitempty"`
}

type lockInfo struct {
	Symbol    string `json:"symbol"`
	Path      string `json:"path"`
	Owner     string `json:"owner"`
	ExpiresIn int    `json:"expires_in"`
}

func boolPtr(b bool) *bool { return &b }

func TestRequestRoundTrip(t *testing.T) {
	cases := []request{
		{Op: "acquire", Symbol: "LockManager", Path: "src/lock.rs", Agent: "agent-1", TTL: 300},
		{Op: "release", Symbol: "LockManager", Path: "src/lock.rs", Agent: "agent-1"},
		{Op: "check", Symbol: "LockManager", Path: "src/lock.rs"},
		{Op: "list"},
		{Op: "ping"},
	}
	for _, req := range cases {
		b, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("marshal %q: %v", req.Op, err)
		}
		var got request
		if err := json.Unmarshal(b, &got); err != nil {
			t.Fatalf("unmarshal %q: %v", req.Op, err)
		}
		if got.Op != req.Op {
			t.Errorf("op: want %q got %q", req.Op, got.Op)
		}
	}
}

func TestResponseOmitsEmptyFields(t *testing.T) {
	r := response{Ok: boolPtr(true)}
	b, _ := json.Marshal(r)
	var m map[string]any
	json.Unmarshal(b, &m)
	for _, field := range []string{"code", "detail", "owner", "locked", "locks", "pong"} {
		if _, exists := m[field]; exists {
			t.Errorf("field %q should be omitted when empty", field)
		}
	}
}

func TestPingResponseShape(t *testing.T) {
	r := response{Pong: boolPtr(true)}
	b, _ := json.Marshal(r)
	var m map[string]any
	json.Unmarshal(b, &m)
	if _, ok := m["pong"]; !ok {
		t.Error("pong field missing")
	}
	if _, ok := m["ok"]; ok {
		t.Error("ok field should be absent in pong response")
	}
}

func TestLockInfoFields(t *testing.T) {
	r := response{
		Locked: boolPtr(true),
		Locks:  []lockInfo{{Symbol: "Foo", Path: "src/foo.rs", Owner: "agent-1", ExpiresIn: 120}},
	}
	b, _ := json.Marshal(r)
	var got response
	if err := json.Unmarshal(b, &got); err != nil {
		t.Fatal(err)
	}
	if len(got.Locks) != 1 || got.Locks[0].ExpiresIn != 120 {
		t.Errorf("lock info round-trip failed: %+v", got.Locks)
	}
}
