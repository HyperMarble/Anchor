package tests

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

// startDaemon builds and starts anchor-lockd, returning its platform endpoint.
func startDaemon(t *testing.T) (endpoint string, cleanup func()) {
	t.Helper()
	bin := filepath.Join(t.TempDir(), "anchor-lockd"+testBinarySuffix())
	build := exec.Command("go", "build", "-o", bin, ".")
	build.Dir = ".."
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("could not build anchor-lockd: %v\n%s", err, out)
	}

	endpoint = testEndpoint(t)
	cmd := exec.Command(bin, "--socket", endpoint, "--state", "")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		t.Fatalf("start daemon: %v", err)
	}

	deadline := time.Now().Add(2 * time.Second)
	ready := false
	for time.Now().Before(deadline) {
		conn, err := dialEndpoint(endpoint, 50*time.Millisecond)
		if err == nil {
			_ = conn.Close()
			ready = true
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	if !ready {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		t.Fatalf("daemon did not become ready at %s", endpoint)
	}

	return endpoint, func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
		cleanupTestEndpoint(endpoint)
	}
}

func send(t *testing.T, endpoint string, req map[string]any) map[string]any {
	t.Helper()
	conn, err := dialEndpoint(endpoint, 2*time.Second)
	if err != nil {
		t.Fatalf("dial: %v", err)
	}
	defer conn.Close()
	if err := json.NewEncoder(conn).Encode(req); err != nil {
		t.Fatalf("encode: %v", err)
	}
	var resp map[string]any
	if err := json.NewDecoder(conn).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	return resp
}

func TestIntegrationPing(t *testing.T) {
	sock, cleanup := startDaemon(t)
	defer cleanup()

	resp := send(t, sock, map[string]any{"op": "ping"})
	if resp["pong"] != true {
		t.Errorf("expected pong:true, got %v", resp)
	}
}

func TestIntegrationAcquireAndRelease(t *testing.T) {
	sock, cleanup := startDaemon(t)
	defer cleanup()

	resp := send(t, sock, map[string]any{
		"op": "acquire", "symbol": "LockManager", "path": "src/lock.rs", "agent": "agent-1", "ttl": 300,
	})
	if resp["ok"] != true {
		t.Fatalf("acquire failed: %v", resp)
	}

	resp = send(t, sock, map[string]any{
		"op": "release", "symbol": "LockManager", "path": "src/lock.rs", "agent": "agent-1",
	})
	if resp["ok"] != true {
		t.Fatalf("release failed: %v", resp)
	}
}

func TestIntegrationConflictThenSuccess(t *testing.T) {
	sock, cleanup := startDaemon(t)
	defer cleanup()

	send(t, sock, map[string]any{
		"op": "acquire", "symbol": "LockManager", "path": "src/lock.rs", "agent": "agent-1", "ttl": 300,
	})

	resp := send(t, sock, map[string]any{
		"op": "acquire", "symbol": "LockManager", "path": "src/lock.rs", "agent": "agent-2", "ttl": 300,
	})
	if resp["code"] != "locked" || resp["owner"] != "agent-1" {
		t.Fatalf("expected locked by agent-1, got %v", resp)
	}

	send(t, sock, map[string]any{
		"op": "release", "symbol": "LockManager", "path": "src/lock.rs", "agent": "agent-1",
	})

	resp = send(t, sock, map[string]any{
		"op": "acquire", "symbol": "LockManager", "path": "src/lock.rs", "agent": "agent-2", "ttl": 300,
	})
	if resp["ok"] != true {
		t.Fatalf("agent-2 should acquire after agent-1 released: %v", resp)
	}
}

func TestIntegrationPathTraversalRejected(t *testing.T) {
	sock, cleanup := startDaemon(t)
	defer cleanup()

	resp := send(t, sock, map[string]any{
		"op": "acquire", "symbol": "LockManager", "path": "../../../etc/passwd", "agent": "agent-1", "ttl": 300,
	})
	if resp["code"] != "invalid_path" {
		t.Fatalf("expected invalid_path, got %v", resp)
	}
}

func TestIntegrationUnknownOp(t *testing.T) {
	sock, cleanup := startDaemon(t)
	defer cleanup()

	resp := send(t, sock, map[string]any{"op": "explode"})
	if resp["code"] != "unknown_op" {
		t.Fatalf("expected unknown_op, got %v", resp)
	}
}
