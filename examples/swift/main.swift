// Minimal: start sharing bandwidth in the background (Swift).
//
// Wrap sdk/swift in a SwiftPM target and import it here:
//   import MeerklyNetwork
import Foundation

@main
struct Example {
    static func main() async throws {
        let client = try ProxyClient(config: ProxyConfig(
            publisherId: ProcessInfo.processInfo.environment["MEERKLY_PUBLISHER_ID"] ?? "your-publisher-id",
            gatewayAddresses: ["127.0.0.1:4443"], // dev override (prod bakes this in)
            caCertPath: "certs/dev/ca.crt"         // dev override
        ))

        try await client.start() // connects + registers; the exit node runs in the background
        print("sharing bandwidth as", client.clientKey() ?? "?")

        // Keep alive; the proxy works in the background.
        try await Task.sleep(nanoseconds: .max)
    }
}
