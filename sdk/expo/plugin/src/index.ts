import {
  AndroidConfig,
  ConfigPlugin,
  createRunOncePlugin,
  withAndroidManifest,
} from '@expo/config-plugins'

const pkg = require('../../package.json')

export type MeerklyPluginOptions = {
  /**
   * Run the exit node while the app is backgrounded.
   *
   * Off by default, and that default is the whole point. An Android library's
   * manifest merges into every app that installs it, so if this package
   * declared a `specialUse` foreground service outright, every consumer would
   * inherit that declaration and have to justify it at Play Store review —
   * including an app that only ever earns while the user has it open, which
   * needs no background execution at all.
   *
   * Turning this on adds the service and its permissions to your manifest, and
   * you then own the Play Store declaration that comes with it.
   */
  backgroundMode?: boolean
}

const SERVICE = 'com.meerkly.expo.MeerklyService'

// Android 14+ requires every specialUse foreground service to declare what it
// is for. The value is shown to reviewers, so it says plainly what the service
// does rather than restating the product name.
const SUBTYPE_PROPERTY = 'android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE'
const SUBTYPE_VALUE =
  'Maintains the user-initiated network connection that shares this ' +
  'device bandwidth as a proxy exit node, which must persist while the app ' +
  'is backgrounded for the user to be paid for it.'

const PERMISSIONS = [
  'android.permission.FOREGROUND_SERVICE',
  'android.permission.FOREGROUND_SERVICE_SPECIAL_USE',
  // Android 13+. Without it the ongoing notification is suppressed; the
  // service still runs, but the user cannot see that it is running.
  'android.permission.POST_NOTIFICATIONS',
]

/**
 * The manifest edit itself, separated from the Expo plumbing so it can be
 * tested directly. A prebuild is far too heavy a thing to run in unit tests,
 * and this is the part with the actual logic in it.
 */
export function addBackgroundMode(manifest: AndroidConfig.Manifest.AndroidManifest) {
  const application = AndroidConfig.Manifest.getMainApplicationOrThrow(manifest)

  manifest.manifest['uses-permission'] ??= []
  const declared = manifest.manifest['uses-permission']
  for (const name of PERMISSIONS) {
    if (!declared.some((p: any) => p.$?.['android:name'] === name)) {
      declared.push({ $: { 'android:name': name } } as any)
    }
  }

  application.service ??= []
  const already = application.service.some(
    (s: any) => s.$?.['android:name'] === SERVICE,
  )
  if (!already) {
    application.service.push({
      $: {
        'android:name': SERVICE,
        'android:exported': 'false',
        'android:foregroundServiceType': 'specialUse',
      },
      property: [
        { $: { 'android:name': SUBTYPE_PROPERTY, 'android:value': SUBTYPE_VALUE } },
      ],
    } as any)
  }

  return manifest
}

const withMeerklyBackgroundMode: ConfigPlugin = (config) =>
  withAndroidManifest(config, (cfg) => {
    addBackgroundMode(cfg.modResults)
    return cfg
  })

const withMeerkly: ConfigPlugin<MeerklyPluginOptions | void> = (config, options) => {
  // Nothing is added unless the app asks for it. An app using the default
  // in-process mode gets a manifest this plugin never touched.
  if (!options || options.backgroundMode !== true) return config
  return withMeerklyBackgroundMode(config)
}

export default createRunOncePlugin(withMeerkly, pkg.name, pkg.version)
