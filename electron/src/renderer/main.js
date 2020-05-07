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

const defaultWsPort = 8422
const defaultHost = localStorage.getItem('webSocketAddress') || `ws://${window.location.hostname}:${defaultWsPort}`
const webSocketHost = process.env.NODE_ENV === 'development' ? 'ws://192.168.100.201:8422' : defaultHost

axios.get(`http://${window.location.host}/_ws.json`).then(res => {
  const { wsPort } = res.data
  if (wsPort) {
    const wsHost = `ws://${window.location.hostname}:${wsPort}`
    if (wsHost !== webSocketHost) {
      localStorage.setItem('webSocketAddress', wsHost)
      window.location.reload()
    }
  }
}).catch(e => {
  console.log(`Unable to get webscoket host, use default: ${webSocketHost}`)
})

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
