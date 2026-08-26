// anchor-lockd: symbol-level write lock daemon over local IPC.
package main

import (
	"context"
	"flag"
	"log"
	"os"
	"os/signal"
	"syscall"

	"golang.org/x/sync/errgroup"
)

func main() {
	endpoint := flag.String("socket", defaultEndpoint(), "local IPC endpoint (Unix socket or Windows named pipe)")
	statePath := flag.String("state", defaultStatePath(), "lock state snapshot path (empty disables persistence)")
	flag.Parse()

	ln, cleanupEndpoint, err := listenEndpoint(*endpoint)
	if err != nil {
		log.Fatalf("listen: %v", err)
	}
	defer cleanupEndpoint()

	log.Printf("anchor-lockd listening on %s", *endpoint)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	g, ctx := errgroup.WithContext(ctx)
	mgr := NewLockManager()
	if *statePath != "" {
		mgr.SetPersistPath(*statePath)
	}

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
