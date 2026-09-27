package com.meerkly.sdk

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

/**
 * What this proves, and why it is worth an emulator in CI.
 *
 * The AAR content check already tells us a `.so` exists for every ABI and that
 * the Kotlin compiled. It cannot tell us the two things that actually break in
 * the field: that JNA finds and loads `libmeerkly.so` out of the packaged AAR on
 * a real Android runtime, and that the uniffi contract checksums baked into the
 * generated Kotlin match the ones compiled into that particular `.so`. A
 * mismatch there — bindings regenerated against a stale cdylib, say — aborts on
 * the very first FFI call with a checksum panic, and it would reach consumers,
 * because Maven Central releases cannot be withdrawn.
 *
 * Both are settled the moment `ProxyClient` is constructed: that call crosses
 * the FFI boundary, so it can only return if the library loaded and the
 * contract matched.
 *
 * No gateway is involved. 127.0.0.1:1 is chosen to be unroutable, so `start()`
 * exercises the async bridge and the error path without the test depending on
 * a network or a running gateway.
 */
@RunWith(AndroidJUnit4::class)
class LoadTest {

    private fun testConfig() = ProxyConfig(
        publisherId = "pub_androidtest",
        gatewayAddresses = listOf("127.0.0.1:1"),
        startTimeoutMs = 3_000u,
        connectTimeoutMs = 1_000u,
        sdk = "kotlin",
        app = "meerkly-android-sdk-test/0",
    )

    /** Loading the native library and crossing the FFI boundary once. */
    @Test
    fun clientConstructsAndReportsIdle() {
        val client = ProxyClient(testConfig())
        try {
            assertEquals(ClientState.IDLE, client.state())
            assertFalse(client.connected())
        } finally {
            client.destroy()
        }
    }

    /** The async bridge round-trips a Rust error back into a Kotlin exception. */
    @Test
    fun startAgainstUnreachableGatewayFails() {
        val client = ProxyClient(testConfig())
        try {
            try {
                runBlocking { client.start() }
                fail("start() against an unroutable address should not succeed")
            } catch (expected: ProxyException) {
                // The point is that it arrives as ProxyException rather than a
                // crash in the FFI layer.
            }
            assertFalse(client.connected())
        } finally {
            client.destroy()
        }
    }
}
