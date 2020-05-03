<template>
  <div id="wrapper">
    <div class="center">
      <div class="battery-info">
        <div :class="{'show': batteryCharging}" class="charge-tag">
          <img class="flash" src="~@/assets/flash.svg" alt="">
          <p>Charging</p>
        </div>
        <div class="battery-shape">
          <div class="battery-content" :class="batteryColor" :style="'width:'+batteryPercent+'%'"></div>
        </div>
        <div class="battery-level">{{batteryPercent}}%</div>
        <div class="battery-model">{{model}}</div>
        <img class="logo" src="~@/assets/logo.svg" alt="">
      </div>
      <div class="setting-panel">
        <div class="title">Schedule Wake Up</div>
        <el-row>
          <el-select v-model="alarmOptionValue" placeholder="Select" :disabled="!socketConnect" @change="alarmOptionValueChange">
            <el-option
                    v-for="item in alarmOption"
                    :key="item.value"
                    :label="item.label"
                    :value="item.value">
            </el-option>
          </el-select>
          <el-time-picker
                  class="time-picker"
                  v-model="timeEditValue"
                  :disabled="alarmOptionValue === 0 || !socketConnect"
                  :picker-options="{
                    selectableRange: '00:00:00 - 23:59:59'
                  }"
                  @change="timeEditChange"
                  placeholder="select anytime">
          </el-time-picker>
          <el-button v-if="alarmOptionValue === 1" :disabled="!socketConnect" @click="repeatDialog = true">Repeat</el-button>
        </el-row>
        <el-row>
          <p class="desc">{{alarmMessage}}</p>
        </el-row>
        <div class="title">Custom Button Function</div>
        <el-row>
          <el-form ref="buttonFuncFormSingle" :model="buttonFuncForm.single" label-width="90px">
            <el-form-item label="Single Tap">
              <el-select v-model="buttonFuncForm.single.func" placeholder="Select" :disabled="!socketConnect" @change="buttonFuncChange('single')">
                <el-option
                        v-for="item in buttonFuncForm.single.options"
                        :key="item.value"
                        :label="item.label"
                        :value="item.value">
                </el-option>
              </el-select>
              <el-button v-if="buttonFuncForm.single.func === 1" @click="openShellEdit('single')" :disabled="!socketConnect">Edit</el-button>
              <span class="tag-span"><el-tag :type="singleTrigger?'success':''">Triggered</el-tag></span>
            </el-form-item>
          </el-form>
        </el-row>
        <el-row>
          <el-form ref="buttonFuncFormDouble" :model="buttonFuncForm.double" label-width="90px">
            <el-form-item label="Double Tap">
              <el-select v-model="buttonFuncForm.double.func" placeholder="Select" :disabled="!socketConnect" @change="buttonFuncChange('double')">
                <el-option
                        v-for="item in buttonFuncForm.double.options"
                        :key="item.value"
                        :label="item.label"
                        :value="item.value">
                </el-option>
              </el-select>
              <el-button v-if="buttonFuncForm.double.func === 1" @click="openShellEdit('double')" :disabled="!socketConnect">Edit</el-button>
              <span class="tag-span"><el-tag :type="doubleTrigger?'success':''">Triggered</el-tag></span>
            </el-form-item>
          </el-form>
        </el-row>
        <el-row>
          <el-form ref="buttonFuncFormLong" :model="buttonFuncForm.long" label-width="90px">
            <el-form-item label="Long Tap">
              <el-select v-model="buttonFuncForm.long.func" placeholder="Select" :disabled="!socketConnect" @change="buttonFuncChange('long')">
                <el-option
                        v-for="item in buttonFuncForm.long.options"
                        :key="item.value"
                        :label="item.label"
                        :value="item.value">
                </el-option>
              </el-select>
              <el-button v-if="buttonFuncForm.long.func === 1" @click="openShellEdit('long')" :disabled="!socketConnect">Edit</el-button>
              <span class="tag-span"><el-tag :type="longTrigger?'success':''">Triggered</el-tag></span>
            </el-form-item>
          </el-form>
        </el-row>
        <div class="title">Safe Shutdown</div>
        <el-row>
          <el-select v-model="safeShutdown" placeholder="Please Select" :disabled="!socketConnect" @change="safeShutdownChange">
            <el-option
                    v-for="item in safeShutdownOpts"
                    :key="item.value"
                    :label="item.label"
                    :value="item.value">
            </el-option>
          </el-select>
          <el-select v-if="safeShutdown" v-model="safeShutdownDelay" placeholder="Please Select" :disabled="!socketConnect" @change="safeShutdownDelayChange">
            <el-option
                    v-for="item in safeShutdownDelayOpts"
                    :key="item.value"
                    :label="item.label"
                    :value="item.value">
            </el-option>
          </el-select>
        </el-row>
      </div>

      <div class="rtc-panel">
        <div class="sys-info"><el-button icon="el-icon-refresh" circle @click="timeDialog = true"></el-button>  <span class="text">RTC Time : {{ rtcTimeDisplayString }}</span></div>
      </div>

      <el-dialog title="Repeat" :visible.sync="repeatDialog">
        <el-checkbox-group v-model="checkRepeat" @change="checkRepeatChange">
          <el-row>
            <el-checkbox label="Monday"></el-checkbox>
            <el-checkbox label="Tuesday"></el-checkbox>
            <el-checkbox label="Wednesday"></el-checkbox>
            <el-checkbox label="Thursday"></el-checkbox>
            <el-checkbox label="Friday"></el-checkbox> 
            <el-checkbox label="Saturday"></el-checkbox>
            <el-checkbox label="Sunday"></el-checkbox>
          </el-row>
          <el-row class="mt20">
            <el-button size="mini" @click="checkRepeatAll">Check All</el-button>
            <el-button size="mini" @click="uncheckRepeatAll">Clear All</el-button>
          </el-row>
        </el-checkbox-group>
        <br>
      </el-dialog>

      <el-dialog title="Sync Time" :visible.sync="timeDialog">
        <el-row>
          RTC Time : {{rtcTimeDisplayString}}
        </el-row>
        <br>
        <el-row>
          <el-button @click="syncPi2RTC">Pi > RTC</el-button>
          <el-button @click="syncRTC2Pi">RTC > Pi</el-button>
          <el-button @click="syncWebTime">Web > Pi & RTC</el-button>
        </el-row>
      </el-dialog>

      <el-dialog :title="editShellDialogTitle" :visible.sync="editShellDialog">
        <el-row>
          <el-form :model="editShellDialogForm">
            <el-form-item label="Shell" label-width="50px">
              <el-input v-model="editShellDialogCache" autocomplete="off" placeholder="Input shell script here..."></el-input>
            </el-form-item>
          </el-form>
        </el-row>
        <br>
        <div slot="footer" class="dialog-footer">
          <el-button @click="closeShellEdit">Cancel</el-button>
          <el-button type="primary" @click="buttonFuncChange(editShellDialogForm.type)">Confirm</el-button>
        </div>
      </el-dialog>
    </div>
  </div>
</template>

<script>
  import Moment from 'moment'
  export default {
    name: 'index-page',
    debug: true,
    components: { },
    data () {
      return {
        rtcTime: null,
        rtcTimeDisplayString: '',
        rtcUpdateTime: new Date().getTime(),
        batteryPercent: '...',
        batteryCharging: false,
        socketConnect: false,
        model: '...',
        alarmOption: [
          { label: 'Disabled', value: 0 },
          { label: 'Enabled', value: 1 }
          // { label: 'CircleSet', value: 2 }
        ],
        alarmOptionValue: 0,
        timeEditValue: new Date(2019, 8, 1, 18, 40, 30),
        timeRepeat: parseInt(0, 2),
        checkRepeat: [],
        repeatDialog: false,
        singleTrigger: true,
        doubleTrigger: true,
        longTrigger: true,
        buttonFuncForm: {
          single: {
            type: 'single',
            enable: false,
            func: 0,
            shell: '',
            options: [
              { label: 'None', value: 0 },
              { label: 'Custom Shell', value: 1 }
            ]
          },
          double: {
            type: 'double',
            enable: false,
            func: 0,
            shell: '',
            options: [
              { label: 'None', value: 0 },
              { label: 'Shutdown', value: 2 },
              { label: 'Custom Shell', value: 1 }
            ]
          },
          long: {
            type: 'long',
            enable: false,
            func: 0,
            shell: '',
            options: [
              { label: 'None', value: 0 },
              { label: 'Shutdown', value: 2 },
              { label: 'Custom Shell', value: 1 }
            ]
          }
        },
        editShellDialogTitle: '',
        editShellDialog: false,
        editShellDialogCache: '',
        editShellDialogForm: {},
        safeShutdown: 0,
        safeShutdownOpts: [
          { label: 'Disabled', value: 0 },
          { label: 'Battery <= 1%', value: 1 },
          { label: 'Battery <= 3%', value: 3 },
          { label: 'Battery <= 5%', value: 5 }
        ],
        safeShutdownDelay: 0,
        safeShutdownDelayOpts: Array(121).fill(0).map((i, k) => {
          return {
            label: k ? `${k} seconds delay` : `immediately`,
            value: k
          }
        }),
        timeDialog: false
      }
    },
    mounted () {
      const that = this
      this.createWebSocketClient()
      setTimeout(() => {
        that.timeUpdater()
      }, 1000)
    },
    computed: {
      batteryColor () {
        if (this.batteryPercent < 10) return 'red'
        if (this.batteryPercent < 30) return 'yellow'
        return 'green'
      },
      alarmMessage () {
        if (this.alarmOptionValue === 1) {
          let repeatString = this.timeRepeat.toString(2)
          repeatString = '0000000'.substring(0, 7 - repeatString.length) + repeatString
          let repeatMessage = ''
          if (repeatString === '1111111') {
            repeatMessage = 'repeat everyday.'
          } else {
            let repeatArray = []
            repeatString.split('').map((item, index) => {
              item = parseInt(item)
              let days = ['Sun', 'Sat', 'Fri', 'Thu', 'Wed', 'Tue', 'Mon']
              if (item) {
                repeatArray.push(days[index])
              }
            })
            repeatMessage = `repeat on ${repeatArray.join(', ')}.`
          }
          return `Schedule wake up at ${this.timeEditValue.toTimeString().split(' ')[0]}, ${repeatMessage}`
        } else {
          return 'Schedule wake up off.'
        }
      }
    },
    methods: {
      createWebSocketClient () {
        const that = this
        this.$socket.onopen = function () {
          console.log(`[Websocket CLIENT] open()`)
          that.getBatteryInfo(true)
        }
      },
      bindSocket () {
        const that = this
        this.$socket.onmessage = async function (e) {
          let msg = e.data
          if (msg.indexOf('battery') < 0) console.log(msg)
          if (msg.indexOf('model:') > -1) {
            that.model = msg.replace('model: ', '')
          }
          if (!msg.indexOf('battery:')) {
            that.batteryPercent = parseInt(msg.replace('battery: ', ''))
          }
          if (!msg.indexOf('battery_charging: ')) {
            that.batteryCharging = msg.indexOf('true') > 0
          }
          if (!msg.indexOf('rtc_time: ')) {
            msg = msg.replace('rtc_time: ', '').trim()
            that.rtcTime = new Moment(msg)
            that.rtcUpdateTime = new Date().getTime()
          }
          if (!msg.indexOf('rtc_alarm_enabled: ')) {
            that.alarmOptionValue = (msg.replace('rtc_alarm_enabled: ', '').trim() === 'true') ? 1 : 0
          }
          if (!msg.indexOf('rtc_alarm_time: ')) {
            msg = msg.replace('rtc_alarm_time: ', '').trim()
            const alarmTime = new Moment(msg)
            const tempTime = new Date()
            tempTime.setSeconds(alarmTime.second())
            tempTime.setMinutes(alarmTime.minute())
            tempTime.setHours(alarmTime.hour())
            that.timeEditValue = tempTime
          }
          if (!msg.indexOf('alarm_repeat: ')) {
            const alarmRepeat = parseInt(msg.replace('alarm_repeat: ', ''))
            that.timeRepeat = alarmRepeat
            if (!that.timeRepeat) that.alarmOptionValue = 0 
            that.timeRepeat2checkbox()
          }
          if (!msg.indexOf('safe_shutdown_level: ')) {
            that.safeShutdown = parseInt(msg.replace('safe_shutdown_level: ', ''))
          }
          if (!msg.indexOf('safe_shutdown_delay: ')) {
            that.safeShutdownDelay = parseInt(msg.replace('safe_shutdown_delay: ', ''))
          }
          if (!msg.indexOf('button_enable')) {
            let msgArr = msg.split(' ')
            that.buttonFuncForm[msgArr[1]].enable = (msgArr[2].trim() === 'true')
          }
          if (!msg.indexOf('button_shell')) {
            let msgArr = msg.split(' ')
            let shell = msg.replace(msgArr[0] + ' ' + msgArr[1] + ' ', '').replace('\n', '')
            let button = that.buttonFuncForm[msgArr[1]]
            button.shell = shell
            if (button.enable) {
              button.func = shell === 'sudo shutdown now' ? 2 : 1
            }
          }
          if (['single', 'double', 'long'].indexOf(msg) >= 0) {
            if (msg === 'single') {
              that.singleTrigger = false
            }
            if (msg === 'double') {
              that.doubleTrigger = false
            }
            if (msg === 'long') {
              that.longTrigger = false
            }
            setTimeout(() => {
              that.singleTrigger = true
              that.doubleTrigger = true
              that.longTrigger = true
            }, 100)
          }
        }
      },
      getBatteryInfo (loop) {
        const that = this
        if (this.$socket.readyState === 1) {
          if (!this.socketConnect) {
            this.bindSocket()
            this.$socket.send('get model')
            this.$socket.send('get rtc_time')
            this.$socket.send('get rtc_alarm_enabled')
            this.$socket.send('get rtc_alarm_time')
            this.$socket.send('get alarm_repeat')
            this.$socket.send('get button_enable single')
            this.$socket.send('get button_enable double')
            this.$socket.send('get button_enable long')
            this.$socket.send('get button_shell single')
            this.$socket.send('get button_shell double')
            this.$socket.send('get button_shell long')
            this.$socket.send('get safe_shutdown_level')
            this.$socket.send('get safe_shutdown_delay')
          }
          this.socketConnect = true
          this.$socket.send('get battery')
          this.$socket.send('get battery_i')
          this.$socket.send('get battery_v')
          this.$socket.send('get battery_charging')
        } else {
          this.socketConnect = false
          this.batteryPercent = 0
          this.batteryCharging = false
          this.model = 'Not Available'
        }
        if (loop) {
          setTimeout(() => {
            that.getBatteryInfo(true)
          }, 1000)
        }
      },
      syncPi2RTC () {
        this.$socket.send('rtc_pi2rtc')
        setTimeout(() => {
          this.$socket.send('get rtc_time')
        }, 1000)
        this.timeDialog = false
      },
      syncRTC2Pi () {
        this.$socket.send('rtc_rtc2pi')
        setTimeout(() => {
          this.$socket.send('get rtc_time')
        }, 1000)
        this.timeDialog = false
      },
      syncWebTime () {
        this.$socket.send('rtc_web')
        setTimeout(() => {
          this.$socket.send('get rtc_time')
        }, 1000)
        this.timeDialog = false
      },
      timeUpdater () {
        const that = this
        if (this.rtcTime) {
          const current = new Date().getTime()
          const offset = current - this.rtcUpdateTime
          this.rtcUpdateTime = current
          this.rtcTime = this.rtcTime.add({ milliseconds: offset })
          this.rtcTimeDisplayString = this.rtcTime.toDate()
        }
        setTimeout(() => {
          that.timeUpdater()
        }, 1000)
      },
      timeEditChange () {
        const sec = this.timeEditValue.getSeconds()
        const min = this.timeEditValue.getMinutes()
        const hour = this.timeEditValue.getHours()
        const setTime = new Moment().second(sec).minute(min).hour(hour)
        this.$socket.send(`rtc_alarm_set ${setTime.toISOString()} ${this.timeRepeat}`)
      },
      timeRepeat2checkbox () {
        const weekdays = ['Sunday', 'Saturday', 'Friday', 'Thursday', 'Wednesday', 'Tuesday', 'Monday']
        const repeatString = this.timeRepeat.toString(2).split('')
        this.checkRepeat = repeatString.map((i, k) => (i === '1') ? weekdays[k] : null).filter(i => i !== null)
      },
      checkRepeatAll () {
        this.checkRepeat = ['Sunday', 'Saturday', 'Friday', 'Thursday', 'Wednesday', 'Tuesday', 'Monday']
        this.checkRepeatChange()
      },
      uncheckRepeatAll () {
        this.checkRepeat = []
        this.checkRepeatChange()
      },
      checkRepeatChange () {
        const weekdays = ['Sunday', 'Saturday', 'Friday', 'Thursday', 'Wednesday', 'Tuesday', 'Monday']
        const repeatArray = weekdays.map(i => this.checkRepeat.indexOf(i) >= 0 ? 1 : 0)
        this.timeRepeat = parseInt(repeatArray.join(''), 2)
        this.alarmOptionValue = this.timeRepeat ? 1 : 0
        this.timeEditChange()
      },
      buttonFuncChange (type) {
        let button = this.buttonFuncForm[type]
        button.shell = this.editShellDialogCache
        button.enable = button.func > 0
        if (button.func === 2) {
          button.shell = 'sudo shutdown now'
        }
        this.$socket.send(`set_button_enable ${type} ${button.enable ? 1 : 0}`)
        this.$socket.send(`set_button_shell ${type} ${button.shell}`)
        this.editShellDialog = false
      },
      openShellEdit (type) {
        this.editShellDialogTitle = `Shell to execute for ${type} tap`
        this.editShellDialogForm = this.buttonFuncForm[type]
        this.editShellDialog = true
        this.editShellDialogCache = this.editShellDialogForm.shell
      },
      closeShellEdit () {
        this.editShellDialog = false
      },
      safeShutdownChange () {
        this.$socket.send(`set_safe_shutdown_level ${this.safeShutdown}`)
      },
      safeShutdownDelayChange () {
        this.$socket.send(`set_safe_shutdown_delay ${this.safeShutdownDelay}`)
      },
      alarmOptionValueChange () {
        if (this.alarmOptionValue) {
          this.timeRepeat = 127
          this.timeRepeat2checkbox()
          this.checkRepeatChange()
        } else {
          this.$socket.send('rtc_alarm_disable')
          this.$socket.send('get rtc_alarm_enabled')
          this.$socket.send('get rtc_alarm_time')
        }
      }
    }
  }
</script>

<style>
  @import url('https://fonts.googleapis.com/css?family=Source+Sans+Pro');
  @keyframes show-once {
    0% {
      opacity: 0;
    }
    10% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
  * {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }
  body {
    font-family: 'Source Sans Pro', sans-serif;
    position: fixed;
    background-color: orange;
    text-align: center;
    width: 100%;
    height: 100%;
  }
  .setting-panel .el-date-editor.el-input, .el-date-editor.el-input__inner{
    width: 160px;
  }
  .el-row{
    margin-top: 6px;
  }
  .setting-panel .el-form-item__label{
    text-align: left;
  }
  .setting-panel .el-form-item{
    margin-bottom: 10px;
  }
  .tag-span .el-tag{
    display: none;
    opacity: 1;
  }
  .tag-span .el-tag.el-tag--success{
    display: inline-block;
    animation: show-once 2s ease-in-out forwards;
  }
  .el-checkbox{
    width: 100px;
  }
  .mt20{
    margin-top: 20px;
  }
</style>

<style lang="less">
  #wrapper {
    background: linear-gradient(#ffe025, orange);
    width: 100%;
    height: 100vw;
    margin: 0 auto;
    text-align: left;
  }
  .center{
    position: relative;
    width: 900px;
    margin: 0 auto;
    text-align: left;
  }
  .battery-info{
    position: absolute;
    top: 0;
    left: 0;
    width: 350px;
    height: 595px;
  }

  .charge-tag{
    position: absolute;
    left: 50%;
    top: 140px;
    width: 120px;
    height: 30px;
    margin-left: -75px;
    color: orange;
    padding: 3px 40px;
    background-color: #fff;
    border-radius: 15px;
    box-shadow: 0 0 10px 2px rgba(157, 104, 0, 0.1);
    font-weight: bold;
    opacity: 0;
    transition: all 0.5s ease-in-out;
    transform: translateY(80px);
    .flash{
      position: absolute;
      left: 20px;
      top: 6px;
      width: 12px;
    }
    &.show{
      transform: translateY(0);
      opacity: 1;
    }
  }
  .battery-shape{
    position: absolute;
    top: 200px;
    left: 80px;
    width: 160px;
    height: 80px;
    padding: 6px;
    background-color: #fff;
    border-radius: 6px;
    box-shadow: 0 0 10px 2px rgba(157, 104, 0, 0.1);
    &:before{
      display: block;
      position: absolute;
      content: " ";
      width: 30px;
      height: 30px;
      background-color: #fff;
      right: -15px;
      top: 25px;
      border-radius: 6px;
    }
    .battery-content{
      position: relative;
      width: 0%;
      height: 100%;
      border-radius: 4px;
      transition: all 1s ease-in-out;
      &.green{
        background-color: #88e61b;
      }
      &.red{
        background-color: #ff521c;
      }
      &.yellow{
        background-color: #ffd100;
      }
    }
  }
  
  .battery-level{
    position: absolute;
    top: 290px;
    left: 80px;
    width: 160px;
    height: 80px;
    text-align: center;
    color: #fff;
    font-size: 42px;
    font-weight: bold;
  }

  .battery-model{
    position: absolute;
    top: 340px;
    left: 80px;
    width: 160px;
    height: 80px;
    text-align: center;
    color: #fff;
    font-size: 16px;
  }

  .logo{
    position: absolute;
    width: 140px;
    bottom: 50px;
    left: 50%;
    margin-left: -85px;
  }

  .setting-panel{
    position: absolute;
    top: 20px;
    right: 20px;
    width: 550px;
    height: 470px;
    padding: 0 30px;
    background-color: #fff;
    border-radius: 8px;
    box-shadow: 0 0 10px 2px rgba(157, 104, 0, 0.1);
    .title{
      font-size: 18px;
      font-weight: bold;
      color: #1f3f6b;
      margin: 20px 0;
    }
    .desc{
      color: #a2a6b8;
      font-size: 12px;
    }
  }
  .rtc-panel{
    position: absolute;
    top: 500px;
    right: 20px;
    width: 550px;
    height: 60px;
    padding: 0 30px;
    background-color: #fff;
    border-radius: 8px;
    box-shadow: 0 0 10px 2px rgba(157, 104, 0, 0.1);
  }
  .sys-info{
    margin-top: 10px;
    font-size: 14px;
    color: #999;
    .text{
      margin-left: 10px;
    }
  }

</style>
