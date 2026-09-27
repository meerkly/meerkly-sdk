package com.meerkly.expo

import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import com.meerkly.sdk.ClientState
import com.meerkly.sdk.ProxyClient
import com.meerkly.sdk.ProxyConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.UUID

/**
 * The single exit-node client for the process, plus the two things the SDK
 * deliberately leaves to its host: where a device id is kept, and how state
 * changes reach the app.
 *
 * A singleton because a process should be exactly one exit node. Two clients
 * would register twice under one publisher id and compete for the same
 * bandwidth.
 */
object MeerklyClient {

    /** What a state change looks like when it reaches JavaScript. */
    data class Snapshot(val state: String, val connected: Boolean, val clientKey: String?)

    private const val PREFS = "com.meerkly.expo"
    private const val KEY_DEVICE_ID = "device_id"

    private var client: ProxyClient? = null
    private var pollJob: Job? = null
    private var listener: ((Snapshot) -> Unit)? = null
    private var lastSnapshot: Snapshot? = null

    private val scope = CoroutineScope(Dispatchers.Default)

    // connect() and disconnect() are suspend, so they cannot use
    // @Synchronized: a monitor held across a suspension point blocks the
    // thread the coroutine resumes on. A Mutex is the coroutine equivalent
    // and keeps a start racing a stop from ending with two clients.
    private val lifecycle = Mutex()

    @Synchronized
    fun setListener(l: ((Snapshot) -> Unit)?) {
        listener = l
    }

    fun snapshot(): Snapshot {
        val c = client ?: return Snapshot("idle", false, null)
        return Snapshot(stateName(c.state()), c.connected(), c.clientKey())
    }

    private fun stateName(state: ClientState) = when (state) {
        ClientState.IDLE -> "idle"
        ClientState.CONNECTING -> "connecting"
        ClientState.CONNECTED -> "connected"
        ClientState.STOPPED -> "stopped"
    }

    /**
     * A device id that survives restarts.
     *
     * The SDK core stores nothing on purpose — only the host knows where an id
     * belongs on its platform. For an Expo app this module *is* that host, so
     * it keeps one here. An app that has its own identity passes it instead.
     */
    private fun deviceId(context: Context, supplied: String?): String {
        if (!supplied.isNullOrBlank()) return supplied
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        prefs.getString(KEY_DEVICE_ID, null)?.let { return it }
        val fresh = UUID.randomUUID().toString()
        prefs.edit().putString(KEY_DEVICE_ID, fresh).apply()
        return fresh
    }

    /** Whether the app's merged manifest actually declares the service. */
    fun serviceDeclared(context: Context): Boolean = try {
        context.packageManager.getServiceInfo(
            ComponentName(context, MeerklyService::class.java), 0,
        )
        true
    } catch (_: PackageManager.NameNotFoundException) {
        false
    }

    @Synchronized
    private fun build(context: Context, options: MeerklyConfig): ProxyClient {
        client?.let { return it }
        val created = ProxyClient(
            ProxyConfig(
                publisherId = options.publisherId,
                gatewayAddresses = options.gatewayAddresses ?: emptyList(),
                startTimeoutMs = options.startTimeoutMs?.toUInt(),
                connectTimeoutMs = options.connectTimeoutMs?.toUInt(),
                deviceId = deviceId(context, options.deviceId),
                deviceName = options.deviceName ?: Build.MODEL,
                sdk = "expo",
                app = options.app,
            ),
        )
        client = created
        return created
    }

    /** Connect and register. Suspends until online, or throws. */
    suspend fun connect(context: Context, options: MeerklyConfig) = lifecycle.withLock {
        val c = build(context.applicationContext, options)
        startPolling()
        c.start()
        emitIfChanged()
    }

    suspend fun disconnect() = lifecycle.withLock {
        val c = client
        stopPolling()
        if (c != null) {
            try {
                c.stop()
            } finally {
                c.destroy()
                client = null
            }
        }
        emitIfChanged()
    }

    /**
     * Poll the client and emit only when something changed.
     *
     * The SDK exposes state as getters, not a callback, so something has to
     * poll. Doing it here rather than in JavaScript keeps the bridge quiet: an
     * app that is simply connected sends no events at all, instead of a
     * timer's worth of identical ones.
     */
    @Synchronized
    private fun startPolling() {
        if (pollJob?.isActive == true) return
        pollJob = scope.launch {
            while (isActive) {
                emitIfChanged()
                delay(1_000)
            }
        }
    }

    @Synchronized
    private fun stopPolling() {
        pollJob?.cancel()
        pollJob = null
    }

    private fun emitIfChanged() {
        val now = snapshot()
        if (now != lastSnapshot) {
            lastSnapshot = now
            listener?.invoke(now)
        }
    }
}
