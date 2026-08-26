//go:build !windows

package tests

import (
	"net"
	"path/filepath"
	"testing"
	"time"
)

func testEndpoint(t *testing.T) string {
	t.Helper()
	return filepath.Join(t.TempDir(), "anchor-lockd.sock")
}

func testBinarySuffix() string { return "" }

func dialEndpoint(endpoint string, timeout time.Duration) (net.Conn, error) {
	return net.DialTimeout("unix", endpoint, timeout)
}

func cleanupTestEndpoint(string) {}
