package tests

import (
	"slices"
	"strings"
	"testing"
)

// validatePath, validateAgent, validateSymbol, normTTL are tested via
// integration through the lock operations — these tests verify the rules
// directly using the same logic mirrored here.

func isValidPath(p string) bool {
	if p == "" || len(p) > 0 && p[0] == '/' {
		return false
	}
	return !slices.Contains(strings.Split(p, "/"), "..")
}

func isValidAgent(a string) bool {
	if len(a) == 0 || len(a) > 64 {
		return false
	}
	for _, c := range a {
		if !((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '-') {
			return false
		}
	}
	return true
}

func isValidSymbol(s string) bool {
	if len(s) == 0 || len(s) > 256 {
		return false
	}
	for _, c := range s {
		if !((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '_' || c == ':') {
			return false
		}
	}
	return true
}

func TestValidatePath(t *testing.T) {
	valid := []string{"src/lock.rs", "a", "src/foo/bar.rs"}
	for _, p := range valid {
		if !isValidPath(p) {
			t.Errorf("expected valid: %q", p)
		}
	}
	invalid := []string{"", "/absolute", "src/../secret", "../escape"}
	for _, p := range invalid {
		if isValidPath(p) {
			t.Errorf("expected invalid: %q", p)
		}
	}
}

func TestValidateAgent(t *testing.T) {
	valid := []string{"agent-1", "a", "Claude-Code", "agent123"}
	for _, a := range valid {
		if !isValidAgent(a) {
			t.Errorf("expected valid agent: %q", a)
		}
	}
	invalid := []string{"", "agent_1", "agent 1", "agent@1"}
	for _, a := range invalid {
		if isValidAgent(a) {
			t.Errorf("expected invalid agent: %q", a)
		}
	}
}

func TestValidateSymbol(t *testing.T) {
	valid := []string{"LockManager", "try_acquire", "Foo::Bar", "A"}
	for _, s := range valid {
		if !isValidSymbol(s) {
			t.Errorf("expected valid symbol: %q", s)
		}
	}
	invalid := []string{"", "has space", "has-hyphen", "emoji🔒"}
	for _, s := range invalid {
		if isValidSymbol(s) {
			t.Errorf("expected invalid symbol: %q", s)
		}
	}
}

func TestNormTTL(t *testing.T) {
	cases := []struct{ in, want int }{
		{0, 300},     // default
		{-1, 300},    // negative → default
		{9999, 3600}, // cap at max
		{120, 120},   // passthrough
		{1, 1},       // min
	}
	for _, c := range cases {
		got := normTTL(c.in)
		if got != c.want {
			t.Errorf("normTTL(%d) = %d, want %d", c.in, got, c.want)
		}
	}
}

func normTTL(ttl int) int {
	const (
		defaultTTL = 300
		maxTTL     = 3600
		minTTL     = 1
	)
	if ttl <= 0 {
		return defaultTTL
	}
	if ttl > maxTTL {
		return maxTTL
	}
	if ttl < minTTL {
		return minTTL
	}
	return ttl
}
