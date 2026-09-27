// One config, two builds. `expo prebuild` cannot be pointed at a different
// config file, so the mode is an environment variable — which is also how CI
// builds the app both ways from one checkout.
const backgroundMode = process.env.MEERKLY_BACKGROUND === '1'

module.exports = {
  expo: {
    name: 'meerkly-expo-example',
    slug: 'meerkly-expo-example',
    version: '1.0.0',
    platforms: ['android'],
    android: { package: 'com.meerkly.expo.example' },
    plugins: backgroundMode ? [['@meerkly/expo', { backgroundMode: true }]] : [],
  },
}
