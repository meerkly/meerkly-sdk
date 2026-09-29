import { requireOptionalNativeModule } from 'expo-modules-core'

import type { ClientState, MeerklyConfig } from './Meerkly.types'

/**
 * The native module, or null on platforms that do not have one.
 *
 * `requireOptionalNativeModule` rather than `requireNativeModule` on purpose:
 * this package is Android-only, and a cross-platform app must still be able to
 * import it on iOS and web without the bundle throwing at import time. The
 * public API turns that null into a clear error at the point of use instead.
 */
export type MeerklyNativeModule = {
  start(config: MeerklyConfig): Promise<void>
  stop(): Promise<void>
  getState(): ClientState
  isConnected(): boolean
  getClientKey(): string | null
  setNetwork(network: string | null): void
}

export default requireOptionalNativeModule<MeerklyNativeModule>('Meerkly')
