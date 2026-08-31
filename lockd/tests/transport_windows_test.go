//go:build windows

package tests

import (
	"fmt"
	"net"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/Microsoft/go-winio"
)

func testEndpoint(t *testing.T) string {
	t.Helper()
	name := strings.NewReplacer("/", "-", "\\", "-", " ", "-").Replace(t.Name())
	return fmt.Sprintf(`\\.\pipe\anchor-lockd-test-%d-%s`, os.Getpid(), name)
}

func testBinarySuffix() string { return ".exe" }

func dialEndpoint(endpoint string, timeout time.Duration) (net.Conn, error) {
	return winio.DialPipe(endpoint, &timeout)
}

func cleanupTestEndpoint(string) {}
