// Minimal: start sharing bandwidth in the background (Kotlin / Android).
//
// The generated binding lives in package `uniffi.meerkly`; bundle it
// in your AAR (per-ABI .so + JNA). uniffi's async methods are `suspend` funs.
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.runBlocking
import uniffi.meerkly.ProxyClient
import uniffi.meerkly.ProxyConfig

fun main() = runBlocking {
    val client = ProxyClient(
        ProxyConfig(
            publisherId = System.getenv("MEERKLY_PUBLISHER_ID") ?: "your-publisher-id",
            gatewayAddresses = listOf("127.0.0.1:4443"), // dev override (prod bakes this in)
            caCertPath = "certs/dev/ca.crt",              // dev override
        )
    )

    client.start() // suspend: connects + registers; the exit node runs in the background
    println("sharing bandwidth as ${client.clientKey()}")

    // In an Android app, launch this from a foreground-service coroutine scope
    // and keep it alive with the app.
    awaitCancellation()
}
