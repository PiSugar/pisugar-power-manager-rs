import * as enUS from './en-US'
import * as zhCN from './zh-CN'

export const localeOptions = [
  { label: 'English', value: 'en-US', messages: enUS },
  { label: '简体中文', value: 'zh-CN', messages: zhCN }
]

export const messages = localeOptions.reduce((p, k) => ({
  ...p,
  [k.value]: k.messages
}), {})
