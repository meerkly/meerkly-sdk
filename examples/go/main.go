// Minimal: start sharing bandwidth in the background (Go).
//
//	./scripts/build-ffi-bindings.sh
//	cd examples/go
//	CGO_CFLAGS="-I$(git rev-parse --show-toplevel)/sdk/go/meerkly" \
//	CGO_LDFLAGS="-L$(git rev-parse --show-toplevel)/target/release -lmeerkly \
//	  -Wl,-rpath,$(git rev-parse --show-toplevel)/target/release" \
//	  go run .
package main

import (
	"fmt"
	"os"
	"os/signal"

	"github.com/meerkly/meerkly-sdk/sdk/go/meerkly"
)

func main() {
	ca := envOr("MEERKLY_CA_CERT_PATH", "certs/dev/ca.crt") // dev override (prod bakes this in)
	client, err := meerkly.NewProxyClient(meerkly.ProxyConfig{
		PublisherId:      envOr("MEERKLY_PUBLISHER_ID", "your-publisher-id"),
		GatewayAddresses: []string{"127.0.0.1:4443"}, // dev override
		CaCertPath:       &ca,
	})
	if err != nil {
		panic(err)
	}

	if err := client.Start(); err != nil { // connects + registers; runs in the background
		panic(err)
	}
	fmt.Println("sharing bandwidth as", *client.ClientKey())

	// Keep running; the proxy works in the background until interrupted.
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt)
	<-sig
	_ = client.Stop()
}

func envOr(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}
