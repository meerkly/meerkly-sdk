import withMeerkly, { addBackgroundMode } from '../index'

/** A manifest shaped the way @expo/config-plugins parses one. */
const emptyManifest = () =>
  ({
    manifest: {
      $: { 'xmlns:android': 'http://schemas.android.com/apk/res/android' },
      application: [{ $: { 'android:name': '.MainApplication' } }],
    },
  }) as any

const serviceNames = (m: any) =>
  (m.manifest.application[0].service ?? []).map((s: any) => s.$['android:name'])

const permissionNames = (m: any) =>
  (m.manifest['uses-permission'] ?? []).map((p: any) => p.$['android:name'])

describe('background mode off, the default', () => {
  it('does not touch the config at all', () => {
    const config = { name: 'game', slug: 'game' } as any
    expect(withMeerkly(config, undefined as any)).toBe(config)
    expect(withMeerkly(config, {} as any)).toBe(config)
    expect(withMeerkly(config, { backgroundMode: false } as any)).toBe(config)
  })
})

describe('background mode on', () => {
  it('declares the service with the specialUse type', () => {
    const manifest = addBackgroundMode(emptyManifest())
    expect(serviceNames(manifest)).toContain('com.meerkly.expo.MeerklyService')

    const service = (manifest as any).manifest.application[0].service[0]
    expect(service.$['android:foregroundServiceType']).toBe('specialUse')
    expect(service.$['android:exported']).toBe('false')
  })

  it('declares the subtype property Android 14 requires', () => {
    const manifest = addBackgroundMode(emptyManifest())
    const service = (manifest as any).manifest.application[0].service[0]
    const property = service.property[0]
    expect(property.$['android:name']).toBe(
      'android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE',
    )
    expect(property.$['android:value'].length).toBeGreaterThan(0)
  })

  it('adds the permissions the service needs', () => {
    const manifest = addBackgroundMode(emptyManifest())
    expect(permissionNames(manifest)).toEqual(
      expect.arrayContaining([
        'android.permission.FOREGROUND_SERVICE',
        'android.permission.FOREGROUND_SERVICE_SPECIAL_USE',
        'android.permission.POST_NOTIFICATIONS',
      ]),
    )
  })

  // Prebuild can run mods more than once, and a manifest that accumulated a
  // duplicate <service> fails to merge.
  it('is idempotent', () => {
    const manifest = addBackgroundMode(addBackgroundMode(emptyManifest()))
    expect(serviceNames(manifest)).toHaveLength(1)
    expect(permissionNames(manifest)).toHaveLength(3)
  })
})
