import { defineConfig } from 'wxt';
import { AI_MATCH_PATTERNS } from './src/core/origins';

const EXT_PUBLIC_KEY =
  'MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAornU7posS3zyYrbNsD+7lH70iQLorIgaBFRUpY96vGJx4VBKwuTYkblHfUM/QS2G6BjiSO+v0rehh6bm4z/fh9j+MleBR2NNZlbcviR0VHTaKkMAWHkI0fh5HaUtyseTz1C92tovOs45eIjyzYB3DFTDv3ina0Rt4z5adysQ+uhdrDYWsqId6g2qzcUIgJz9BphyaGglx4vBy+Jn9mTT0hA5Y7v6Zwq/v3Rusb08rCyEEue9iiYvo39KG/DJKaxjuFLp8abUxwImPocT8T64ezHV5vVQxLxYebsid/3gVmRG3fiEq6imoQ+Ux4tk7H934ElabPhqyOxyNoUWSE1DrQIDAQAB';

export default defineConfig({
  manifest: ({ browser }) => ({
    name: '__MSG_extName__',
    description: '__MSG_extDesc__',
    default_locale: 'en',
    version: '0.1.0',
    icons: {
      16: 'icon/16.png',
      32: 'icon/32.png',
      48: 'icon/48.png',
      128: 'icon/128.png',
    },
    ...(browser === 'firefox' ? {} : { key: EXT_PUBLIC_KEY }),
    permissions: ['tabs', 'nativeMessaging', 'scripting', 'alarms', 'webRequest', 'downloads', 'storage'],
    host_permissions: AI_MATCH_PATTERNS,
    browser_specific_settings: {
      gecko: {
        id: 'contextbraid@extensions.local',
        strict_min_version: '140.0',
        data_collection_permissions: { required: ['none'] },
      },
    },
  }),
});
