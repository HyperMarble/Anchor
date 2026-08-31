//go:build windows

package main

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strings"

	"github.com/Microsoft/go-winio"
)

func defaultEndpoint() string {
	user := normalizeEndpointUser(os.Getenv("USERNAME"))
	return `\\.\pipe\anchor-lockd-` + user
}

func defaultStatePath() string {
	return filepath.Join(os.TempDir(), "anchor.lockd.state.json")
}

func normalizeEndpointUser(raw string) string {
	var out strings.Builder
	lastDash := false
	for _, r := range raw {
		var next rune
		switch {
		case r >= 'a' && r <= 'z':
			next = r
		case r >= 'A' && r <= 'Z':
			next = r + ('a' - 'A')
		case r >= '0' && r <= '9':
			next = r
		default:
			next = '-'
		}
		if next == '-' {
			if out.Len() == 0 || lastDash {
				continue
			}
			lastDash = true
		} else {
			lastDash = false
		}
		out.WriteRune(next)
		if out.Len() >= 64 {
			break
		}
	}
	user := strings.TrimSuffix(out.String(), "-")
	if user == "" {
		return "local"
	}
	return user
}

func listenEndpoint(endpoint string) (net.Listener, func(), error) {
	if !strings.HasPrefix(strings.ToLower(endpoint), `\\.\pipe\`) {
		return nil, func() {}, fmt.Errorf("Windows lockd endpoint must be a named pipe under \\\\.\\pipe\\")
	}
	// A protected DACL granting generic-all only to the pipe owner keeps other
	// local users from impersonating an agent or observing repository locks.
	ln, err := winio.ListenPipe(endpoint, &winio.PipeConfig{
		SecurityDescriptor: "D:P(A;;GA;;;OW)",
		MessageMode:        false,
		InputBufferSize:    64 * 1024,
		OutputBufferSize:   64 * 1024,
	})
	if err != nil {
		return nil, func() {}, err
	}
	return ln, func() {}, nil
}
