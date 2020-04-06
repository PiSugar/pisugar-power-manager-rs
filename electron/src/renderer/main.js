import Vue from 'vue'
import axios from 'axios'

import App from './App'
import router from './router'
// import store from './store'
import VueNativeSock from 'vue-native-websocket'

import ElementUI from 'element-ui'
import 'element-ui/lib/theme-chalk/index.css'

const defaultWsPort = 8081
const defaultHost = localStorage.getItem('webSocketAddress') || `ws://${window.location.hostname}:${defaultWsPort}`
const webSocketHost = process.env.NODE_ENV === 'development' ? 'ws://192.168.100.201:8081' : defaultHost

axios.get(`http://${window.location.host}/_ws.json`).then(res => {
  const { wsPort } =  res.data
  if (wsPort) {
    const wsHost = `ws://${window.location.hostname}:${wsPort}`
    if (wsHost !== webSocketHost) {
      localStorage.setItem('webSocketAddress', )
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
Vue.use(ElementUI)
Vue.use(VueNativeSock, webSocketHost, {
  reconnection: true,
  reconnectionDelay: 3000
})

/* eslint-disable no-new */
new Vue({
  components: { App },
  router,
  // store,
  template: '<App/>'
}).$mount('#app')
