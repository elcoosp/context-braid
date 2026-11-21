/** @type {import('@lingui/conf').LinguiConfig} */
export default {
  locales: ['en', 'fr'],
  sourceLocale: 'en',
  compileNamespace: 'ts',
  catalogs: [
    {
      path: '<rootDir>/src/locales/{locale}/messages',
      include: ['src'],
      exclude: ['src/locales/**'],
    },
  ],
};
