package com.meerkly.expo

import android.content.Context
import android.content.Intent
import android.os.Build
import expo.modules.kotlin.exception.CodedException
// The infix builder that gives an AsyncFunction a suspend body. Without it
// the block is an ordinary lambda and cannot call a suspend function.
import expo.modules.kotlin.functions.Coroutine
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.records.Field
import expo.modules.kotlin.records.Record

class ForegroundServiceOptions : Record {
    @Field val title: String = "Meerkly"
    @Field val text: String = "Sharing bandwidth"
    @Field val channelName: String? = null
}

class MeerklyConfig : Record {
    @Field val publisherId: String = ""
    @Field val app: String? = null
    @Field val deviceId: String? = null
    @Field val deviceName: String? = null
    @Field val network: String? = null
    @Field val gatewayAddresses: List<String>? = null
    @Field val startTimeoutMs: Int? = null
    @Field val connectTimeoutMs: Int? = null
    @Field val foregroundService: ForegroundServiceOptions? = null
}

/**
 * Raised when an app asks for background mode without having enabled it at
 * build time. Worth its own error because the fix is a specific line in
 * app.json that the developer will not guess from a generic failure.
 */
class BackgroundModeNotEnabled : CodedException(
    "Meerkly: foregroundService was requested but this app does not declare the " +
        "service. Add the config plugin with background mode enabled — " +
        "[\"@meerkly/expo\", { \"backgroundMode\": true }] in app.json — then " +
        "rebuild the app. Omit `foregroundService` to run in-process instead, " +
        "which needs no declaration.",
)

class MeerklyStartFailed(cause: String) : CodedException("Meerkly could not start: $cause")

class MeerklyModule : Module() {

    private val context: Context
        get() = requireNotNull(appContext.reactContext) { "no Android context" }

    override fun definition() = ModuleDefinition {
        Name("Meerkly")

        Events("onStateChange")

        AsyncFunction("start") Coroutine { config: MeerklyConfig ->
            val service = config.foregroundService

            // Checked before connecting, not after. Failing afterwards would
            // leave a connected node with no notification and no way for the
            // user to see or stop it.
            if (service != null && !MeerklyClient.serviceDeclared(context)) {
                throw BackgroundModeNotEnabled()
            }

            if (service != null) {
                startService(MeerklyService.intent(context, service))
            }

            try {
                MeerklyClient.connect(context, config)
            } catch (e: Throwable) {
                // Do not leave a notification behind for a node that never came
                // up; the user would see the app claiming to earn when it isn't.
                if (service != null) context.stopService(Intent(context, MeerklyService::class.java))
                throw MeerklyStartFailed(e.message ?: e.toString())
            }
        }

        // Called with a dot rather than as an infix operator, because infix
        // notation cannot carry a type argument — and the type argument is
        // exactly what is needed here. A parameterless lambda matches both the
        // zero- and one-parameter Coroutine builders, so naming R disambiguates.
        AsyncFunction("stop").Coroutine<Unit> {
            MeerklyClient.disconnect()
            context.stopService(Intent(context, MeerklyService::class.java))
        }

        Function("getState") { MeerklyClient.snapshot().state }
        Function("isConnected") { MeerklyClient.snapshot().connected }
        Function("getClientKey") { MeerklyClient.snapshot().clientKey }
        Function("setNetwork") { network: String? -> MeerklyClient.setNetwork(network) }

        // Poll and emit only while JavaScript is actually listening.
        OnStartObserving {
            MeerklyClient.setListener { snapshot ->
                sendEvent(
                    "onStateChange",
                    mapOf(
                        "state" to snapshot.state,
                        "connected" to snapshot.connected,
                        "clientKey" to snapshot.clientKey,
                    ),
                )
            }
        }
        OnStopObserving { MeerklyClient.setListener(null) }
    }

    private fun startService(intent: Intent) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(intent)
        } else {
            context.startService(intent)
        }
    }
}
