import { useEffect, useState } from 'react'
import { Button, StyleSheet, Text, View } from 'react-native'
import * as Meerkly from '@meerkly/expo'

// Exercises the whole public surface, so that a change which breaks the API
// breaks this app's build rather than a consumer's.
export default function App() {
  const [state, setState] = useState(Meerkly.getState())
  const [error, setError] = useState(null)

  useEffect(() => {
    const sub = Meerkly.addStateListener(({ state }) => setState(state))
    return () => sub.remove()
  }, [])

  const start = async () => {
    setError(null)
    try {
      await Meerkly.start({
        publisherId: process.env.EXPO_PUBLIC_MEERKLY_PUBLISHER_ID ?? 'pub_example',
        app: 'meerkly-expo-example/0.0.1',
        // Only meaningful when the app was built with the plugin's
        // backgroundMode enabled; without it this rejects with a clear error,
        // which is itself worth seeing in the example.
        ...(process.env.EXPO_PUBLIC_MEERKLY_BACKGROUND === '1'
          ? { foregroundService: { title: 'Earning', text: 'Sharing bandwidth' } }
          : {}),
      })
    } catch (e) {
      setError(String(e))
    }
  }

  return (
    <View style={styles.container}>
      <Text style={styles.state}>{state}</Text>
      <Text>{Meerkly.isSupported ? 'supported' : 'not supported on this platform'}</Text>
      <Text>{Meerkly.isConnected() ? Meerkly.getClientKey() : 'not connected'}</Text>
      <Button title="Start" onPress={start} />
      <Button title="Stop" onPress={() => Meerkly.stop().catch((e) => setError(String(e)))} />
      {error ? <Text style={styles.error}>{error}</Text> : null}
    </View>
  )
}

const styles = StyleSheet.create({
  container: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 12 },
  state: { fontSize: 24, fontWeight: '600' },
  error: { color: 'crimson', paddingHorizontal: 24, textAlign: 'center' },
})
