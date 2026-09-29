/** What the exit-node client is currently doing. */
export type ClientState = 'idle' | 'connecting' | 'connected' | 'stopped'

/**
 * Opting in to running while the app is backgrounded.
 *
 * Supplying this moves the client into an Android foreground service, which
 * needs the service declared in the merged manifest — see the `backgroundMode`
 * option of the config plugin. Omit it entirely and the client runs in the app
 * process and stops when the app does, which needs no extra permissions and no
 * Play Store declaration.
 */
export type ForegroundServiceOptions = {
  /** Notification title. Shown to the user for as long as the node is running. */
  title: string
  /** Notification body. */
  text: string
  /** Notification channel name, defaulting to the title. */
  channelName?: string
}

/** The transport a device reports; free-form strings are accepted too. */
export type NetworkType = 'cellular' | 'wifi' | 'ethernet' | 'other' | (string & {})

export type MeerklyConfig = {
  /** Your publisher id from the Meerkly dashboard. Public, not a secret. */
  publisherId: string
  /** The host application, e.g. "my-game/1.4.0". Optional but useful. */
  app?: string
  /**
   * A stable id for this device. Omit it and the module generates one and
   * persists it, so the device keeps its identity across restarts.
   */
  deviceId?: string
  /** A human name for the device, defaulting to the Android model. */
  deviceName?: string
  /**
   * The transport the device is on at start: 'cellular', 'wifi', 'ethernet'
   * or 'other'. Report changes with `setNetwork`. Meerkly uses it, with the
   * network it measures, to classify the exit as mobile, residential or
   * datacenter.
   */
  network?: NetworkType
  /** Development override. Empty or omitted means the production gateway. */
  gatewayAddresses?: string[]
  startTimeoutMs?: number
  connectTimeoutMs?: number
  /** Omit to run in-process; supply to run in a foreground service. */
  foregroundService?: ForegroundServiceOptions
}

export type StateChangeEvent = {
  state: ClientState
  connected: boolean
  /** The gateway-assigned key, null until connected. */
  clientKey: string | null
}

export type MeerklyModuleEvents = {
  onStateChange: (event: StateChangeEvent) => void
}

export type Subscription = { remove(): void }
