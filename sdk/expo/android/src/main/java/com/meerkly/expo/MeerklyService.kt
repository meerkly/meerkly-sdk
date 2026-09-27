package com.meerkly.expo

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

/**
 * Keeps the process alive while the exit node runs, and shows the user that it
 * is running.
 *
 * The client itself lives in [MeerklyClient], not here. This service exists for
 * the two things only a service can do: raise the process out of the cached
 * state Android would otherwise kill it in, and post the ongoing notification
 * that makes continuous background work visible to the person whose bandwidth
 * it is. Keeping the client outside means starting and stopping the service
 * cannot lose a connection, and the in-process mode shares exactly the same
 * client code.
 *
 * It is only ever reachable when the app opted in through the config plugin,
 * which is what adds the declaration to the merged manifest.
 */
class MeerklyService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val title = intent?.getStringExtra(EXTRA_TITLE) ?: "Meerkly"
        val text = intent?.getStringExtra(EXTRA_TEXT) ?: "Sharing bandwidth"
        val channelName = intent?.getStringExtra(EXTRA_CHANNEL_NAME) ?: title

        startForegroundCompat(buildNotification(title, text, channelName))

        // START_STICKY would have Android restart this service after the process
        // dies, but without the config the client needs to reconnect, leaving a
        // notification attached to nothing running. Restarting is the app's
        // call to make, through start().
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        stopForegroundCompat()
        super.onDestroy()
    }

    private fun buildNotification(title: String, text: String, channelName: String): Notification {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            if (manager.getNotificationChannel(CHANNEL_ID) == null) {
                manager.createNotificationChannel(
                    NotificationChannel(
                        CHANNEL_ID,
                        channelName,
                        // Low: this is a persistent status, not an alert. It has
                        // to be visible, it must not make a sound every launch.
                        NotificationManager.IMPORTANCE_LOW,
                    ),
                )
            }
        }

        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }

        return builder
            .setContentTitle(title)
            .setContentText(text)
            // The host app's own icon: a library cannot ship an icon that suits
            // every app, and this one is guaranteed to exist.
            .setSmallIcon(applicationInfo.icon)
            .setOngoing(true)
            .build()
    }

    private fun startForegroundCompat(notification: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            // Android 14+ requires the type at the call site as well as in the
            // manifest, and the two have to agree.
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    @Suppress("DEPRECATION")
    private fun stopForegroundCompat() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            stopForeground(STOP_FOREGROUND_REMOVE)
        } else {
            stopForeground(true)
        }
    }

    companion object {
        private const val CHANNEL_ID = "com.meerkly.expo.node"
        private const val NOTIFICATION_ID = 8_651
        private const val EXTRA_TITLE = "title"
        private const val EXTRA_TEXT = "text"
        private const val EXTRA_CHANNEL_NAME = "channelName"

        fun intent(context: Context, options: ForegroundServiceOptions): Intent =
            Intent(context, MeerklyService::class.java)
                .putExtra(EXTRA_TITLE, options.title)
                .putExtra(EXTRA_TEXT, options.text)
                .putExtra(EXTRA_CHANNEL_NAME, options.channelName ?: options.title)
    }
}
