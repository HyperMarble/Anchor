//go:build !windows

package main

import (
	"net"
	"os"
)

func defaultEndpoint() string {
	return "/tmp/anchor.lock.sock"
}

func defaultStatePath() string {
	return "/tmp/anchor.lockd.state.json"
}

func listenEndpoint(endpoint string) (net.Listener, func(), error) {
	_ = os.Remove(endpoint) // remove a stale socket left by an interrupted daemon
	ln, err := net.Listen("unix", endpoint)
	if err != nil {
		return nil, func() {}, err
	}
	if err := os.Chmod(endpoint, 0o600); err != nil {
		_ = ln.Close()
		_ = os.Remove(endpoint)
		return nil, func() {}, err
	}
	return ln, func() { _ = os.Remove(endpoint) }, nil
}
