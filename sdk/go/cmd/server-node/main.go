// Always-on meerkly exit node for headless servers, in Go.
//
// Embeds the Go SDK (github.com/meerkly/meerkly-sdk/sdk/go) and runs it until stopped, so this
// machine's connection is shared with the network as an exit. Configured
// entirely from the environment; it holds one QUIC connection to a gateway and
// stores nothing.
//
// It links the uniffi cdylib, so build/run through scripts/go-server-node.sh
// (which builds meerkly-client-ffi and sets the CGO flags), e.g.:
//
//	MEERKLY_PUBLISHER_ID=pub_... \
//	MEERKLY_GATEWAY_ADDRESSES=gateway-1:4443,gateway-2:4443 \
//	MEERKLY_CA_CERT_PATH=/certs/ca.crt \
//	  ./scripts/go-server-node.sh
package main

import (
	"log"
	"os"
	"os/signal"
	"strings"
	"syscall"

	"github.com/meerkly/meerkly-sdk/sdk/go/meerkly"
)

func main() {
	log.SetFlags(log.LstdFlags | log.LUTC)

	config := meerkly.ProxyConfig{
		PublisherId:      requireEnv("MEERKLY_PUBLISHER_ID"),
		GatewayAddresses: splitAddrs(requireEnv("MEERKLY_GATEWAY_ADDRESSES")),
		CaCertPath:       ptr(envOr("MEERKLY_CA_CERT_PATH", "certs/dev/ca.crt")),
	}

	client, err := meerkly.NewProxyClient(config)
	if err != nil {
		log.Fatalf("could not create exit node: %v", err)
	}

	log.Printf("starting meerkly exit node (gateways=%s)", strings.Join(config.GatewayAddresses, ","))
	// Start blocks until the client is online (or the attempt fails); a background
	// supervisor then keeps the connection up until Stop.
	if err := client.Start(); err != nil {
		log.Fatalf("could not start: %v", err)
	}
	log.Printf("exit node online (gateway=%s clientKey=%s)", deref(client.GatewayId()), deref(client.ClientKey()))

	// Run until SIGINT/SIGTERM, then tear down cleanly.
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	<-sig

	log.Print("shutting down")
	if err := client.Stop(); err != nil {
		log.Fatalf("error while stopping: %v", err)
	}
	log.Print("exit node stopped")
}

func requireEnv(key string) string {
	v := strings.TrimSpace(os.Getenv(key))
	if v == "" {
		log.Fatalf("%s must be set", key)
	}
	return v
}

func envOr(key, def string) string {
	if v := strings.TrimSpace(os.Getenv(key)); v != "" {
		return v
	}
	return def
}

func splitAddrs(csv string) []string {
	var out []string
	for _, a := range strings.Split(csv, ",") {
		if a = strings.TrimSpace(a); a != "" {
			out = append(out, a)
		}
	}
	if len(out) == 0 {
		log.Fatal("MEERKLY_GATEWAY_ADDRESSES must contain at least one address")
	}
	return out
}

func ptr(s string) *string { return &s }

func deref(s *string) string {
	if s == nil {
		return "?"
	}
	return *s
}
