package tests

import (
	"encoding/json"
	"fmt"
	"net"
	"os"
	"os/exec"
	"testing"
	"time"
)

// startDaemon builds and starts anchor-lockd, returns the socket path and a cleanup func.
func startDaemon(t *testing.T) (socketPath string, cleanup func()) {
	t.Helper()
	bin := "/tmp/anchor-lockd"
	if _, err := os.Stat(bin); err != nil {
		build := exec.Command("go", "build", "-o", bin, ".")
		build.Dir = "/Volumes/Hak_SSD/Anchor/lockd"
		if out, err := build.CombinedOutput(); err != nil {
			t.Skipf("could not build anchor-lockd: %v\n%s", err, out)
		}
	}

	sock := fmt.Sprintf("/tmp/anchor-lockd-test-%d.sock", os.Getpid())
	cmd := exec.Command(bin, "--socket", sock)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		t.Fatalf("start daemon: %v", err)
	}

	// wait for socket to appear
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if _, err := os.Stat(sock); err == nil {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}

	return sock, func() {
		cmd.Process.Kill()
		cmd.Wait()
		os.Remove(sock)
	}
}

func send(t *testing.T, sock string, req map[string]any) map[string]any {
	t.Helper()
	conn, err := net.Dial("unix", sock)
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
