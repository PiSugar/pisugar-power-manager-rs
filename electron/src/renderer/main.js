import Vue from 'vue'
import axios from 'axios'

import App from './App'
import router from './router'
// import store from './store'
import VueNativeSock from 'vue-native-websocket'
import VueI18n from 'vue-i18n'
import ElementUI from 'element-ui'
import 'element-ui/lib/theme-chalk/index.css'
import locale from 'element-ui/lib/locale/lang/en'
import { messages, localeOptions } from './locale'

const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
const devWsHost = 'ws://192.168.100.118:8421/ws'
let webSocketHost = `${wsProtocol}//${window.location.hostname}:${window.location.port}/ws`
if (!process.env.IS_WEB) webSocketHost = devWsHost
if (!process.env.IS_WEB) Vue.use(require('vue-electron'))
Vue.webSocketAddress = webSocketHost
Vue.http = Vue.prototype.$http = axios
Vue.config.productionTip = false
Vue.use(ElementUI, { locale })
Vue.use(VueI18n)
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
  components: { App },
  router,
  // store,
  i18n,
  template: '<App/>'
}).$mount('#app')
