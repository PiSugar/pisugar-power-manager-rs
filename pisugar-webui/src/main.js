// The Vue build version to load with the `import` command
// (runtime-only or standalone) has been set in webpack.base.conf with an alias.
import Vue from 'vue'
import App from './App'
import VueI18n from 'vue-i18n'
import ElementUI from 'element-ui'
import 'element-ui/lib/theme-chalk/index.css'
import VueNativeSock from 'vue-native-websocket'
import locale from 'element-ui/lib/locale/lang/en'
import { messages, localeOptions } from './locale'

const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
let webSocketHost = `${wsProtocol}//${window.location.hostname}:${window.location.port}/ws`
// const devWsHost = 'ws://192.168.100.112:8421/ws'
// webSocketHost = devWsHost

Vue.config.productionTip = false
Vue.use(VueI18n)
Vue.use(ElementUI, { locale })
Vue.use(VueNativeSock, webSocketHost, {
  reconnection: true,
  reconnectionDelay: 3000
})

let userLocale = navigator.language
try {
  userLocale = localStorage.getItem('locale')
  userLocale = userLocale == null ? navigator.language : userLocale
} catch (e) {
  console.warn(e)
}

const i18n = new VueI18n({
  locale: localeOptions.map(i => i.value).indexOf(userLocale) >= 0 ? userLocale : 'en-US',
  messages
})

/* eslint-disable no-new */
new Vue({
  el: '#app',
  i18n,
  components: { App },
  template: '<App/>'
})
