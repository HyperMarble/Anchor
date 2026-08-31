//go:build windows

package main

import (
	"strings"
	"testing"
)

func TestNormalizeEndpointUserMatchesRustClient(t *testing.T) {
	tests := map[string]string{
		"Caleb Chandrasekar": "caleb-chandrasekar",
		"DOMAIN\\Build_User": "domain-build-user",
		"---":                "local",
	}
	for input, want := range tests {
		if got := normalizeEndpointUser(input); got != want {
			t.Errorf("normalizeEndpointUser(%q) = %q, want %q", input, got, want)
		}
	}
	if got := normalizeEndpointUser(strings.Repeat("A", 80)); len(got) != 64 {
		t.Fatalf("normalized user length = %d, want 64", len(got))
	}
}

func TestDefaultEndpointIsWindowsNamedPipe(t *testing.T) {
	if endpoint := defaultEndpoint(); !strings.HasPrefix(endpoint, `\\.\pipe\anchor-lockd-`) {
		t.Fatalf("unexpected default endpoint %q", endpoint)
	}
}
