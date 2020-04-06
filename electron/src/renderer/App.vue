<template>
  <div id="app">
    <router-view></router-view>
  </div>
</template>

<script>
  export default {
    name: 'electron'
  }
  window.addEventListener('keyup', (e) => {
    if (e.keyCode === 123) {
      this.$electron.ipcRenderer.send('f12', '')
    }
  }, false)
</script>

<style>
  /* CSS */
</style>
