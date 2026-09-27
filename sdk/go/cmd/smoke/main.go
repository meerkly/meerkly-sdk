// Go SDK smoke: run the uniffi Go binding as an exit node and prove a request is
// proxied through it. Driven by scripts/go-sdk-smoke.sh.
package main

import (
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"sort"
	"strings"

	"github.com/meerkly/meerkly-sdk/sdk/go/meerkly"
)

func env(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}

func fail(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "FAIL: "+format+"\n", args...)
	os.Exit(1)
}

// Absolute-form GET through the gateway proxy; routes out via our exit node.
//
// Reports the status alongside the body. The status is the point: every way the
// gateway refuses a request answers with an empty body ("Content-Length: 0") and
// distinguishes itself only by code — 407 no credentials, 400 unparseable ones,
// 403 destination policy, 429 over the concurrency cap, 503 nothing selectable,
// 502 selected but unreachable. A caller that reads only the body sees the same
// empty string for all of them.
func proxyGet(proxyAddr, target string) (*http.Response, string, error) {
	proxyURL, err := url.Parse("http://pub_dev000000000000000000000:pw@" + proxyAddr)
	if err != nil {
		return nil, "", err
	}
	client := &http.Client{Transport: &http.Transport{Proxy: http.ProxyURL(proxyURL)}}
	resp, err := client.Get("http://" + target + "/")
	if err != nil {
		return nil, "", err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	return resp, string(body), err
}

func describe(resp *http.Response, body string) string {
	headers := make([]string, 0, len(resp.Header))
	for k, v := range resp.Header {
		headers = append(headers, fmt.Sprintf("%s: %s", k, strings.Join(v, "; ")))
	}
	sort.Strings(headers)
	return fmt.Sprintf("HTTP %s [%s] body=%q", resp.Status, strings.Join(headers, ", "), body)
}

func main() {
	quic := env("MEERKLY_GATEWAY_ADDRESSES", "127.0.0.1:4443")
	ca := env("MEERKLY_CA_CERT_PATH", "certs/dev/ca.crt")
	proxyAddr := env("GATEWAY_PROXY", "127.0.0.1:8888")
	target := env("TARGET", "127.0.0.1:18080")

	client, err := meerkly.NewProxyClient(meerkly.ProxyConfig{
		PublisherId:      "pub_dev000000000000000000000",
		GatewayAddresses: strings.Split(quic, ","),
		CaCertPath:       &ca,
	})
	if err != nil {
		fail("NewProxyClient: %v", err)
	}

	if err := client.Start(); err != nil {
		fail("Start: %v", err)
	}
	if !client.Connected() {
		fail("client not connected after Start()")
	}
	ck := client.ClientKey()
	if ck == nil || *ck == "" {
		fail("no client key was assigned")
	}
	fmt.Printf("PASS: Go exit node online (clientKey=%s)\n", *ck)

	resp, body, err := proxyGet(proxyAddr, target)
	if err != nil {
		fail("proxy request: %v", err)
	}
	if resp.StatusCode != http.StatusOK || body != "OK" {
		fail("proxied request did not return 200 \"OK\": %s", describe(resp, body))
	}
	fmt.Println("PASS: request proxied through the Go exit node")

	if err := client.Stop(); err != nil {
		fail("Stop: %v", err)
	}
	fmt.Println("PASS: client stopped cleanly")
	fmt.Println("GO SDK SMOKE PASSED")
}
