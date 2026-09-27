// The plugin is plain Node TypeScript, so it needs none of the React Native
// test machinery — just ts-jest.
module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  testMatch: ['<rootDir>/plugin/src/__tests__/**/*.test.ts'],
}
