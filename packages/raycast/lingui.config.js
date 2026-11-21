/** @type {import('@lingui/conf').LinguiConfig} */
export default {
  locales: ['en', 'fr', 'zh-CN', 'ja', 'de', 'es', 'ko'],
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
