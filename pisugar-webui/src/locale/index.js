import * as enUS from './en-US'
import * as zhCN from './zh-CN'
import * as ruRU from './ru-RU'
import * as esES from './es-ES'
import * as deDE from './de-DE'
import * as frFR from './fr-FR'
import * as itIT from './it-IT'
import * as jaJP from './ja-JP'
import * as koKR from './ko-KR'
import * as nlNL from './nl-NL'
import * as zhTW from './zh-TW'

export const localeOptions = [
  { label: 'English', value: 'en-US', messages: enUS },
  { label: '简体中文', value: 'zh-CN', messages: zhCN },
  { label: 'Русский', value: 'ru-RU', messages: ruRU },
  { label: 'Español', value: 'es-ES', messages: esES },
  { label: 'Deutsch', value: 'de-DE', messages: deDE },
  { label: 'Français', value: 'fr-FR', messages: frFR },
  { label: 'Italiano', value: 'it-IT', messages: itIT },
  { label: '日本語', value: 'ja-JP', messages: jaJP },
  { label: '한국어', value: 'ko-KR', messages: koKR },
  { label: 'Nederlands', value: 'nl-NL', messages: nlNL },
  { label: '繁體中文', value: 'zh-TW', messages: zhTW }
]

export const messages = localeOptions.reduce((p, k) => ({
  ...p,
  [k.value]: k.messages
}), {})
