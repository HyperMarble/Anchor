// Unix socket accept loop — spawns one goroutine per connection.
package main

import (
	"context"
	"net"
)

func RunServer(ctx context.Context, ln net.Listener, mgr *LockManager) error {
	go func() {
		<-ctx.Done()
		ln.Close()
	}()
	for {
		conn, err := ln.Accept()
		if err != nil {
			// ln.Close() from ctx cancellation triggers this — not an error
			select {
			case <-ctx.Done():
				return nil
			default:
				return err
			}
		}
		go handleConn(conn, mgr)
	}
}
