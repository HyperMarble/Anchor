// Wire types for the anchor-lockd newline-delimited JSON protocol.
package main

// Request is one JSON line sent by a client.
type Request struct {
	Op     string `json:"op"` // acquire | release | check | list | ping
	Symbol string `json:"symbol,omitempty"`
	Path   string `json:"path,omitempty"`
	Agent  string `json:"agent,omitempty"`
	TTL    int    `json:"ttl,omitempty"` // seconds; 0 means use defaultTTL
}

// Response is one JSON line sent back to the client.
type Response struct {
	// acquire / release
	Ok     *bool  `json:"ok,omitempty"`
	Code   string `json:"code,omitempty"`
	Detail string `json:"detail,omitempty"`

	// acquire failure: who holds it
	Owner     string `json:"owner,omitempty"`
	ExpiresIn int    `json:"expires_in,omitempty"` // seconds remaining

	// check
	Locked *bool `json:"locked,omitempty"`

	// list
	Locks []LockInfo `json:"locks,omitempty"`

	// ping
	Pong *bool `json:"pong,omitempty"`
}

// LockInfo is one entry returned by the list operation.
type LockInfo struct {
	Symbol    string `json:"symbol"`
	Path      string `json:"path"`
	Owner     string `json:"owner"`
	ExpiresIn int    `json:"expires_in"`
}

func okResp() Response              { t := true; return Response{Ok: &t} }
func failResp(code string) Response { f := false; return Response{Ok: &f, Code: code} }
func failDetail(code, detail string) Response {
	f := false
	return Response{Ok: &f, Code: code, Detail: detail}
}
func pongResp() Response { t := true; return Response{Pong: &t} }
