# @meerkly/expo

Turn an Expo or React Native app into a Meerkly exit node. Wraps the
[`com.meerkly:sdk`](../android) Android library.

**Android only.** The Node SDK cannot serve this case: React Native runs Hermes
or JavaScriptCore rather than Node, so a native Node addon cannot load in it at
all. On iOS and web this package imports cleanly and reports `isSupported` as
false, so a cross-platform app still builds and runs.

## Install

```bash
npx expo install @meerkly/expo
```

Requires a development build. Expo Go cannot load custom native code.

## Use

```ts
import * as Meerkly from '@meerkly/expo'

if (Meerkly.isSupported) {
  await Meerkly.start({
    publisherId: 'pub_…',      // from the Meerkly dashboard; public, not a secret
    app: 'my-game/1.4.0',
  })
}

const sub = Meerkly.addStateListener(({ state, connected }) => {
  console.log(state, connected)
})

await Meerkly.stop()
sub.remove()
```

`start` resolves once the node is online and rejects with a `MeerklyError`
otherwise. The read-only getters (`getState`, `isConnected`, `getClientKey`)
never throw, so a status component can render on any platform without branching.

## Two modes

**In-process, the default.** The node runs as long as your app process does. No
extra permissions, no service, nothing to declare to the Play Store. This is the
right choice for an app that earns while the user has it open — a game, say.

**Foreground service, opt in.** To keep earning while the app is backgrounded,
enable it at build time:

```json
{
  "expo": {
    "plugins": [["@meerkly/expo", { "backgroundMode": true }]]
  }
}
```

then pass notification copy at runtime:

```ts
await Meerkly.start({
  publisherId: 'pub_…',
  foregroundService: { title: 'Earning', text: 'Sharing bandwidth' },
})
```

Calling that without the plugin flag rejects with an error naming the option to
set, rather than failing obscurely at the Android layer.

The flag is off by default on purpose. An Android library's manifest merges into
every app that installs it, so declaring the service unconditionally would give
every consumer a `specialUse` foreground service to justify at Play Store
review — including apps that never run in the background.

### What background mode obliges you to do

- The plugin adds `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE` and
  `POST_NOTIFICATIONS`, plus the service declaration with its Android 14
  subtype property.
- Expect Play Store review of the `specialUse` declaration. Google asks what the
  service does and why no standard type fits.
- Request `POST_NOTIFICATIONS` at runtime on Android 13+. Without it the ongoing
  notification is suppressed. The service still runs, but the user cannot see
  that it is running, which is worth avoiding on its own terms.

## Device identity

The module generates a device id on first run and keeps it in SharedPreferences,
so a device keeps its identity and its earnings history across restarts. Pass
`deviceId` to use your own instead. The SDK core itself still stores nothing.

## API

| | |
|---|---|
| `start(config)` | connect and register; resolves when online |
| `stop()` | disconnect and stop reconnecting |
| `getState()` | `'idle' \| 'connecting' \| 'connected' \| 'stopped'` |
| `isConnected()` | boolean |
| `getClientKey()` | gateway-assigned key, null until connected |
| `setNetwork(network)` | report the transport (`'cellular'`, `'wifi'`, `'ethernet'`, `'other'`, or null) from your network-change listener; also accepted as `network` in `start(config)` |
| `addStateListener(fn)` | state changes; returns a subscription |
| `isSupported` | false on iOS and web |

State arrives as events rather than polling: the native side watches the client
and emits only on change, so a connected app sends nothing across the bridge.

## License

MIT.
