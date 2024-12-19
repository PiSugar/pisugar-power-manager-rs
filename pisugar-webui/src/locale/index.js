import * as enUS from './en-US'
import * as zhCN from './zh-CN'
import * as ruRU from './ru-RU'

export const localeOptions = [
  { label: 'English', value: 'en-US', messages: enUS },
  { label: '简体中文', value: 'zh-CN', messages: zhCN },
  { label: 'Русский', value: 'ru-RU', messages: ruRU }
]

export const messages = localeOptions.reduce((p, k) => ({
  ...p,
  [k.value]: k.messages
}), {})
