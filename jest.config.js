module.exports = {
  testMatch: [
    '<rootDir>/clients/tests/**/*.test.{ts,tsx,js,jsx}',
    '<rootDir>/clients/deploys/*.test.{ts,tsx,js,jsx}',
    '<rootDir>/clients/scripts/*.test.{ts,tsx,js,jsx}',
    '<rootDir>/clients/releases/*.test.{ts,tsx,js,jsx}',
    '<rootDir>/clients/migrations/*.test.{ts,tsx,js,jsx}',
  ],
  collectCoverage: false,
  moduleFileExtensions: ['ts', 'tsx', 'js', 'jsx', 'json', 'node'],
  transform: {
    '^.+\\.(t|j)sx?$': [
      '@swc/jest',
      {
        sourceMaps: true,
        jsc: {
          parser: {
            syntax: 'typescript',
            tsx: false,
          },
        },
      },
    ],
  },
  extensionsToTreatAsEsm: ['.ts', '.tsx', '.wasm'],
  testEnvironment: 'node',
  testTimeout: 600000,
};
