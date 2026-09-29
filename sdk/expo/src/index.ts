import MeerklyModule from './MeerklyModule'
import type {
  ClientState,
  MeerklyConfig,
  NetworkType,
  StateChangeEvent,
  Subscription,
} from './Meerkly.types'

export * from './Meerkly.types'

/** Every failure this module raises, so callers can catch one type. */
export class MeerklyError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'MeerklyError'
  }
}

/**
 * Whether this platform can run an exit node. False on iOS and web.
 *
 * Branch on this rather than on Platform.OS, so the check keeps working if iOS
 * support lands later.
 */
export const isSupported: boolean = MeerklyModule != null

function native() {
  if (MeerklyModule == null) {
    throw new MeerklyError(
      'Meerkly runs on Android only. Guard your calls with `Meerkly.isSupported`.',
    )
  }
  return MeerklyModule
}

/**
 * Connect and register as an exit node. Resolves once online.
 *
 * Rejects with a MeerklyError if the connection fails or times out, and on
 * platforms with no native module.
 */
export async function start(config: MeerklyConfig): Promise<void> {
  if (!config || typeof config.publisherId !== 'string' || config.publisherId === '') {
    throw new MeerklyError('start() needs a publisherId — get one from the Meerkly dashboard')
  }
  try {
    await native().start(config)
  } catch (e) {
    throw new MeerklyError(e instanceof Error ? e.message : String(e))
  }
}

/** Disconnect and stop reconnecting. Safe to call when already stopped. */
export async function stop(): Promise<void> {
  try {
    await native().stop()
  } catch (e) {
    throw new MeerklyError(e instanceof Error ? e.message : String(e))
  }
}

// The read-only getters below deliberately do NOT throw on an unsupported
// platform. A component that renders connection status should not have to
// branch before it can paint; a cross-platform screen reads "idle, not
// connected" on iOS and renders fine. Only the actions above refuse.

export function getState(): ClientState {
  return MeerklyModule?.getState() ?? 'idle'
}

export function isConnected(): boolean {
  return MeerklyModule?.isConnected() ?? false
}

export function getClientKey(): string | null {
  return MeerklyModule?.getClientKey() ?? null
}

/**
 * Tell Meerkly which transport the device is on now ('cellular', 'wifi',
 * 'ethernet', 'other'), or null when unknown. Call it from your network-change
 * listener; it is remembered across `start()` calls and reaches a running
 * client at once. A no-op on platforms with no native module.
 */
export function setNetwork(network: NetworkType | null): void {
  MeerklyModule?.setNetwork(network ?? null)
}

/**
 * Listen for connection state changes.
 *
 * The native side polls the client while it is running and emits only when
 * something actually changed, so there is no JavaScript timer and an idle app
 * does no work. On an unsupported platform this returns a subscription that
 * never fires, so callers need no branch.
 */
export function addStateListener(
  listener: (event: StateChangeEvent) => void,
): Subscription {
  if (MeerklyModule == null) return { remove() {} }
  return (MeerklyModule as any).addListener('onStateChange', listener)
}
