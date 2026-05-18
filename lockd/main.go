// anchor-lockd: symbol-level write lock daemon over Unix socket.
package main

import (
	"context"
	"flag"
	"log"
	"net"
	"os"
	"os/signal"
	"syscall"

	"golang.org/x/sync/errgroup"
)

func main() {
	socketPath := flag.String("socket", "/tmp/anchor.lock.sock", "Unix socket path")
	flag.Parse()

	os.Remove(*socketPath) // clean up any leftover socket from previous run

	ln, err := net.Listen("unix", *socketPath)
	if err != nil {
		log.Fatalf("listen: %v", err)
	}
	if err := os.Chmod(*socketPath, 0600); err != nil {
		log.Fatalf("chmod socket: %v", err)
	}
	defer os.Remove(*socketPath)

	log.Printf("anchor-lockd listening on %s", *socketPath)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	g, ctx := errgroup.WithContext(ctx)
	mgr := NewLockManager()

	g.Go(func() error {
		return RunServer(ctx, ln, mgr)
	})

	g.Go(func() error {
		mgr.RunCleanup(ctx)
		return nil
	})

	g.Go(func() error {
		quit := make(chan os.Signal, 1)
		signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
		select {
		case sig := <-quit:
			log.Printf("received %s, shutting down", sig)
			cancel()
		case <-ctx.Done():
		}
		return nil
	})

	if err := g.Wait(); err != nil {
		log.Fatalf("server error: %v", err)
	}
	log.Println("anchor-lockd stopped")
}
