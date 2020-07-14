<template>
  <div id="wrapper">
    <div class="center">
      <div class="battery-info">
        <div :class="{'show': isNewVersion ? (batteryPlugged && batteryAllowCharging) : batteryCharging}" class="charge-tag flash-tag">
          <img class="flash" src="~@/assets/flash.svg" alt="">
          <p>{{$t("charging")}}</p>
        </div>
        <div :class="{'show': isNewVersion ? (batteryPlugged && !batteryAllowCharging) : false}" class="charge-tag plug-tag">
          <img class="plug" src="~@/assets/plug.svg" alt="">
          <p>{{$t("notCharging")}}</p>
        </div>
        <div class="battery-shape" @click="handleBatteryClick">
          <div class="battery-content" :class="batteryColor" :style="'width:'+batteryPercent+'%'"></div>
          <div class="charging-layer" v-show="isNewVersion && batteryPlugged && !batteryAllowCharging">
            <div class="line restart-line" :style="`left: ${chargingRestartPoint}%`"></div>
          </div>
        </div>
        <div class="battery-level">{{batteryPercent}}%</div>
        <div class="battery-model">{{model}}</div>
        <img class="logo" src="~@/assets/logo.svg" alt="">
        <div class="website"><a href="http://www.pisugar.com" target="_blank">www.pisugar.com</a></div>
      </div>
      <div class="setting-panel">
        <div class="language">
          <el-link type="info" @click="languageDialog = true">Language: {{locale}}</el-link>
        </div>
        <div class="title">{{$t('wakeUpFeature')}}</div>
        <el-row>
          <el-select v-model="alarmOptionValue"
            placeholder="Select"
            :disabled="!socketConnect"
            @change="alarmOptionValueChange">
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
          <el-button v-if="alarmOptionValue === 1" :disabled="!socketConnect" @click="repeatDialog = true">{{$t('repeat')}}</el-button>
        </el-row>
        <el-row>
          <p class="desc">{{alarmMessage}}</p>
        </el-row>
        <div class="title">{{$t('buttonFunction')}}</div>
        <el-row>
          <el-form ref="buttonFuncFormSingle" :model="buttonFuncForm.single" label-width="90px">
            <el-form-item :label="$t('singleTapLabel')">
              <el-select v-model="buttonFuncForm.single.func" placeholder="Select" :disabled="!socketConnect" @change="buttonFuncChange('single')">
                <el-option
                        v-for="item in buttonFuncForm.single.options"
                        :key="item.value"
                        :label="item.label"
                        :value="item.value">
                </el-option>
              </el-select>
              <el-button v-if="buttonFuncForm.single.func === 1" @click="openShellEdit('single')" :disabled="!socketConnect">{{$t('edit')}}</el-button>
              <span class="tag-span"><el-tag :type="singleTrigger?'success':''">{{$t('triggered')}}</el-tag></span>
            </el-form-item>
          </el-form>
        </el-row>
        <el-row>
          <el-form ref="buttonFuncFormDouble" :model="buttonFuncForm.double" label-width="90px">
            <el-form-item :label="$t('doubleTapLabel')">
              <el-select v-model="buttonFuncForm.double.func" placeholder="Select" :disabled="!socketConnect" @change="buttonFuncChange('double')">
                <el-option
                        v-for="item in buttonFuncForm.double.options"
                        :key="item.value"
                        :label="item.label"
                        :value="item.value">
                </el-option>
              </el-select>
              <el-button v-if="buttonFuncForm.double.func === 1" @click="openShellEdit('double')" :disabled="!socketConnect">{{$t('edit')}}</el-button>
              <span class="tag-span"><el-tag :type="doubleTrigger?'success':''">{{$t('triggered')}}</el-tag></span>
            </el-form-item>
          </el-form>
        </el-row>
        <el-row>
          <el-form ref="buttonFuncFormLong" :model="buttonFuncForm.long" label-width="90px">
            <el-form-item :label="$t('longTapLabel')">
              <el-select v-model="buttonFuncForm.long.func" placeholder="Select" :disabled="!socketConnect" @change="buttonFuncChange('long')">
                <el-option
                        v-for="item in buttonFuncForm.long.options"
                        :key="item.value"
                        :label="item.label"
                        :value="item.value">
                </el-option>
              </el-select>
              <el-button v-if="buttonFuncForm.long.func === 1" @click="openShellEdit('long')" :disabled="!socketConnect">{{$t('edit')}}</el-button>
              <span class="tag-span"><el-tag :type="longTrigger?'success':''">{{$t('triggered')}}</el-tag></span>
            </el-form-item>
          </el-form>
        </el-row>
        <div class="title">{{$t('safeShutdown')}}</div>
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
        <div class="sys-info">
          <el-button icon="el-icon-refresh" circle @click="timeDialog = true"></el-button> 
          <div class="time-text">
            <div class="text rtc"><span class="label">{{$t('rtcTime')}}</span> : {{ rtcTimeString }}</div>
            <div class="text sys"><span class="label">{{$t('sysTime')}}</span> : {{ sysTimeString }}</div>
          </div>
        </div>
      </div>

      <el-dialog :title="$t('repeat')" :visible.sync="repeatDialog">
        <el-row>
          <el-checkbox v-model="alarmRepeatSun">{{$t('weekDay.Sunday')}}</el-checkbox>
          <el-checkbox v-model="alarmRepeatMon">{{$t('weekDay.Monday')}}</el-checkbox>
          <el-checkbox v-model="alarmRepeatTue">{{$t('weekDay.Tuesday')}}</el-checkbox>
          <el-checkbox v-model="alarmRepeatWed">{{$t('weekDay.Wednesday')}}</el-checkbox>
          <el-checkbox v-model="alarmRepeatThu">{{$t('weekDay.Thursday')}}</el-checkbox>
          <el-checkbox v-model="alarmRepeatFri">{{$t('weekDay.Friday')}}</el-checkbox>
          <el-checkbox v-model="alarmRepeatSat">{{$t('weekDay.Saturday')}}</el-checkbox>
        </el-row>
        <el-row class="mt20">
          <el-button size="mini" @click="checkRepeatAll">{{$t('checkAll')}}</el-button>
          <el-button size="mini" @click="uncheckRepeatAll">{{$t('clearAll')}}</el-button>
        </el-row>
        <br>
      </el-dialog>

      <el-dialog :title="$t('syncTime')" :visible.sync="timeDialog">
        <el-row>
          <div class="time-text">
            <div class="text rtc"><span class="label">{{$t('rtcTime')}}</span> : {{ rtcTimeString }}</div>
            <div class="text sys"><span class="label">{{$t('sysTime')}}</span> : {{ sysTimeString }}</div>
          </div>
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
              <el-input v-model="editShellDialogCache" autocomplete="off" :placeholder="$t('shellPlaceholder')"></el-input>
            </el-form-item>
          </el-form>
        </el-row>
        <br>
        <div slot="footer" class="dialog-footer">
          <el-button @click="closeShellEdit">{{$t('cancel')}}</el-button>
          <el-button type="primary" @click="buttonFuncChange(editShellDialogForm.type)">{{$t('confirm')}}</el-button>
        </div>
      </el-dialog>

      <el-dialog :title="$t('selectLanguage')" :visible.sync="languageDialog">
        <el-row>
          <el-form>
            <el-form-item :label="$t('language')" label-width="100px">
              <el-select v-model="locale" placeholder="Select" @change="languageChange">
                <el-option
                        v-for="item in languageOptions"
                        :key="item.value"
                        :label="item.label"
                        :value="item.value">
                </el-option>
              </el-select>
            </el-form-item>
          </el-form>
        </el-row>
        <div slot="footer" class="dialog-footer">
          <el-button @click="languageDialog = false">{{$t('cancel')}}</el-button>
          <el-button type="primary" @click="languageConfirm">{{$t('confirm')}}</el-button>
        </div>
      </el-dialog>

      <el-dialog :title="$t('chargeSetting')" :visible.sync="chargeDialog">
        <el-row>
          <el-slider
            v-model="chargingRestartPoint"
            :min="50"
            :max="100"
            :format-tooltip="formatTooltip"
            :marks="chargingMarks"
            show-input>
          </el-slider>
        </el-row>
        <el-row>
          <span class="charging-desc">{{chargingDesc}}</span>
        </el-row>
        <div slot="footer" class="dialog-footer">
          <el-button @click="chargeDialog = false">{{$t('cancel')}}</el-button>
          <el-button type="primary" @click="chargeConfirm">{{$t('confirm')}}</el-button>
        </div>
      </el-dialog>
    </div>
  </div>
</template>

<script>
  import Moment from 'moment'
  import { localeOptions } from '../locale'
  export default {
    name: 'index-page',
    debug: true,
    components: { },
    data () {
      return {
        rtcTime: null,
        rtcUpdateTime: new Date().getTime(),
        rtcTimeString: '',
        sysTime: null,
        sysUpdateTime: new Date().getTime(),
        sysTimeString: '',
        batteryPercent: '...',
        batteryCharging: false,
        batteryPlugged: false,
        batteryAllowCharging: true,
        socketConnect: false,
        model: '...',
        alarmOption: [
          { label: this.$t('disabled'), value: 0 },
          { label: this.$t('enabled'), value: 1 }
          // { label: 'CircleSet', value: 2 }
        ],
        alarmOptionValue: 0,
        timeEditValue: new Date(2019, 8, 1, 18, 40, 30),
        timeRepeat: 0,
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
              { label: this.$t('none'), value: 0 },
              { label: this.$t('customShell'), value: 1 }
            ]
          },
          double: {
            type: 'double',
            enable: false,
            func: 0,
            shell: '',
            options: [
              { label: this.$t('none'), value: 0 },
              { label: this.$t('shutdown'), value: 2 },
              { label: this.$t('customShell'), value: 1 }
            ]
          },
          long: {
            type: 'long',
            enable: false,
            func: 0,
            shell: '',
            options: [
              { label: this.$t('none'), value: 0 },
              { label: this.$t('shutdown'), value: 2 },
              { label: this.$t('customShell'), value: 1 }
            ]
          }
        },
        editShellDialogTitle: '',
        editShellDialog: false,
        editShellDialogCache: '',
        editShellDialogForm: {},
        safeShutdown: 0,
        safeShutdownOpts: [
          { label: this.$t('disabled'), value: 0 },
          { label: `${this.$t('batteryLevel')} <= 1%`, value: 1 },
          { label: `${this.$t('batteryLevel')} <= 3%`, value: 3 },
          { label: `${this.$t('batteryLevel')} <= 5%`, value: 5 }
        ],
        safeShutdownDelay: 0,
        safeShutdownDelayOpts: Array(121).fill(0).map((i, k) => {
          return {
            label: k ? `${k} ${this.$t('secondsDelay')}` : `${this.$t('immediately')}`,
            value: k
          }
        }),
        chargingRange: [-1, -1],
        chargingRestartPoint: 50,
        isNewVersion: false,
        timeDialog: false,
        locale: 'en',
        languageDialog: false,
        languageOptions: localeOptions,
        chargeDialog: false,
        chargingMarks: {
          80: '80%'
        },
        timeUpdaterCount: 0
      }
    },

    mounted () {
      const that = this
      this.createWebSocketClient()
      setTimeout(() => {
        that.timeUpdater()
      }, 1000)
      this.locale = this.$i18n.locale
      console.log(this.$i18n.locale)
    },

    computed: {
      batteryColor () {
        if (this.batteryPercent < 10) return 'red'
        if (this.batteryPercent < 30) return 'yellow'
        return 'green'
      },
      alarmRepeatSun: {
        get () {
          return this.getBit(this.timeRepeat, 0)
        },
        set (v) {
          this.timeRepeat = this.setBit(this.timeRepeat, 0, v)
        }
      },
      alarmRepeatMon: {
        get () {
          return this.getBit(this.timeRepeat, 1)
        },
        set (v) {
          this.timeRepeat = this.setBit(this.timeRepeat, 1, v)
        }
      },
      alarmRepeatTue: {
        get () {
          return this.getBit(this.timeRepeat, 2)
        },
        set (v) {
          this.timeRepeat = this.setBit(this.timeRepeat, 2, v)
        }
      },
      alarmRepeatWed: {
        get () {
          return this.getBit(this.timeRepeat, 3)
        },
        set (v) {
          this.timeRepeat = this.setBit(this.timeRepeat, 3, v)
        }
      },
      alarmRepeatThu: {
        get () {
          return this.getBit(this.timeRepeat, 4)
        },
        set (v) {
          this.timeRepeat = this.setBit(this.timeRepeat, 4, v)
        }
      },
      alarmRepeatFri: {
        get () {
          return this.getBit(this.timeRepeat, 5)
        },
        set (v) {
          this.timeRepeat = this.setBit(this.timeRepeat, 5, v)
        }
      },
      alarmRepeatSat: {
        get () {
          return this.getBit(this.timeRepeat, 6)
        },
        set (v) {
          this.timeRepeat = this.setBit(this.timeRepeat, 6, v)
        }
      },
      alarmMessage () {
        if (this.alarmOptionValue === 1) {
          let repeatMessage = ''
          if (this.timeRepeat === 127) {
            repeatMessage = this.$t('repeatEveryday')
          } else {
            let repeatArray = []
            let days = ['Sat', 'Fri', 'Thu', 'Wed', 'Tue', 'Mon', 'Sun'].reverse()
            for (let i = 0; i < 7; i++) {
              if (this.getBit(this.timeRepeat, i)) {
                repeatArray.push(this.$t(`weekDayShort.${days[i]}`))
              }
            }
            repeatMessage = `${this.$t('repeatOn')} ${repeatArray.join(', ')}`
          }
          return `${this.$t('wakeUpDesc')} ${this.timeEditValue.toTimeString().split(' ')[0]}, ${repeatMessage}`
        } else {
          return `${this.$t('wakeUpOffDesc')}`
        }
      },
      chargingDesc () {
        switch (this.$i18n.locale) {
          case 'zh-CN':
            return this.chargingRestartPoint !== 100
              ? `电池电量低于${this.chargingRestartPoint}%时重启充电。`
              : `连接电源时一直保持充电状态。`
          default:
            return this.chargingRestartPoint !== 100
              ? `Start charging when the battery level is lower than ${this.chargingRestartPoint}%.`
              : `Always keep charging when USB is connected.`
        }
      }
    },

    watch: {
      alarmOptionValue: function (val) {
        if (val) {
          if (this.timeRepeat === 0) {
            this.timeRepeat = 127
          }
        } else {
          this.timeRepeat = 0
          this.$socket.send('rtc_alarm_disable')
          this.$socket.send('get rtc_alarm_enabled')
          this.$socket.send('get rtc_alarm_time')
        }
      },
      timeRepeat: function (val) {
        if (!this.repeatDialog) return
        this.alarmOptionValue = val === 0 ? 0 : 1
        this.setRtcAlarm()
      }
    },

    methods: {
      getBit (n, pos) {
        return (n & (1 << pos)) > 0
      },
      setBit (n, pos, v) {
        if (v) {
          return n | (1 << pos)
        } else {
          return n & (~(1 << pos))
        }
      },
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
          if (!msg.indexOf('battery_charging_range: ')) {
            const res = msg.replace('battery_charging_range: ', '')
            if (res.trim()) {
              that.chargingRange = res.split(',').map(i => parseInt(i))
            } else {
              that.chargingRange = [100, 100]
            }
            that.chargingRestartPoint = that.chargingRange[0]
          }
          if (!msg.indexOf('battery_led_amount: ')) {
            that.isNewVersion = msg.indexOf('2') > 0
          }
          if (!msg.indexOf('battery_power_plugged: ')) {
            that.batteryPlugged = msg.indexOf('true') > 0
          }
          if (!msg.indexOf('battery_allow_charging: ')) {
            that.batteryAllowCharging = msg.indexOf('true') > 0
          }
          if (!msg.indexOf('rtc_time: ')) {
            msg = msg.replace('rtc_time: ', '').trim()
            that.rtcTime = new Moment(msg).parseZone()
            that.rtcUpdateTime = new Date().getTime()
          }
          if (!msg.indexOf('system_time: ')) {
            msg = msg.replace('system_time: ', '').trim()
            that.sysTime = new Moment(msg).parseZone()
            that.sysUpdateTime = new Date().getTime()
          }
          if (!msg.indexOf('rtc_alarm_enabled: ')) {
            that.alarmOptionValue = (msg.replace('rtc_alarm_enabled: ', '').trim() === 'true') ? 1 : 0
            console.log(msg, that.alarmOptionValue)
          }
          if (!msg.indexOf('rtc_alarm_time: ')) {
            msg = msg.replace('rtc_alarm_time: ', '').trim()
            const alarmTime = new Moment(msg).parseZone()
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
            // that.timeRepeat2checkbox()
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
            this.$socket.send('get system_time')
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
            this.$socket.send('get battery_charging_range')
            this.$socket.send('get battery_led_amount')
          }
          this.socketConnect = true
          this.$socket.send('get battery')
          this.$socket.send('get battery_i')
          this.$socket.send('get battery_v')
          this.$socket.send('get battery_charging')
          this.$socket.send('get battery_power_plugged')
          this.$socket.send('get battery_allow_charging')
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
          this.getDeviceTime()
        }, 1000)
        this.timeDialog = false
      },
      syncRTC2Pi () {
        this.$socket.send('rtc_rtc2pi')
        setTimeout(() => {
          this.getDeviceTime()
        }, 1000)
        this.timeDialog = false
      },
      syncWebTime () {
        this.$socket.send('rtc_web')
        setTimeout(() => {
          this.getDeviceTime()
        }, 1000)
        this.timeDialog = false
      },
      timeUpdater () {
        const that = this
        const current = new Date().getTime()
        this.timeUpdaterCount++
        // align time if diff < 2000
        if (this.rtcTime && this.sysTime) {
          const diff = this.sysTime.diff(this.rtcTime)
          if (Math.abs(diff) < 2000) {
            this.rtcTime = this.rtcTime.add({ milliseconds: diff })
          }
        }
        if (this.rtcTime) {
          const rtcOffset = current - this.rtcUpdateTime
          this.rtcUpdateTime = current
          this.rtcTime = this.rtcTime.add({ milliseconds: rtcOffset })
          this.rtcTimeString = this.rtcTime.toString(true)
        }
        if (this.sysTime) {
          const sysOffset = current - this.sysUpdateTime
          this.sysUpdateTime = current
          this.sysTime = this.sysTime.add({ milliseconds: sysOffset })
          this.sysTimeString = this.sysTime.toString(true)
        }
        setTimeout(() => {
          that.timeUpdater()
        }, 1000)
        if (this.timeUpdaterCount % 5 === 0) {
          this.getDeviceTime()
        }
      },
      timeEditChange () {
        this.setRtcAlarm()
      },
      checkRepeatAll () {
        this.timeRepeat = 127
      },
      uncheckRepeatAll () {
        this.timeRepeat = 0
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
        this.editShellDialogTitle = this.$t(`shellModalTitle.${type}`)
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
          if (this.timeRepeat === 0) {
            this.timeRepeat = 127
          }
          this.setRtcAlarm()
        } else {
          this.$socket.send('rtc_alarm_disable')
          this.$socket.send('get rtc_alarm_enabled')
          this.$socket.send('get rtc_alarm_time')
        }
      },
      languageChange (value) {
        this.locale = value
      },
      languageConfirm () {
        // this.$i18n.locale = this.locale
        if (this.$i18n.locale === this.locale) {
          this.languageDialog = false
          return
        }
        try {
          localStorage.setItem('locale', this.locale)
        } catch (e) {
          console.warn(e)
        }
        window.location.reload()
      },
      chargeConfirm () {
        this.chargeDialog = false
        if (this.chargingRestartPoint === 100) {
          this.$socket.send(`set_battery_charging_range`)
        } else {
          this.$socket.send(`set_battery_charging_range ${this.chargingRestartPoint},100`)
        }
      },
      getDeviceTime () {
        if (!this.socketConnect) return
        this.$socket.send('get rtc_time')
        this.$socket.send('get system_time')
      },
      formatTooltip (val) {
        return `${val}%`
      },
      setRtcAlarm () {
        const sec = this.timeEditValue.getSeconds()
        const min = this.timeEditValue.getMinutes()
        const hour = this.timeEditValue.getHours()
        const setTime = new Moment().second(sec).minute(min).hour(hour).parseZone()
        this.$socket.send(`rtc_alarm_set ${setTime.toISOString()} ${this.timeRepeat}`)
      },
      handleBatteryClick () {
        if (this.isNewVersion) {
          this.chargeDialog = true
        }
      }
    }
  }
</script>

<style>
  @font-face{
    font-family: 'Source Sans Pro';
    src: url('~@/assets/source_sans_pro.woff2')
  }
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
  @keyframes breath {
    0% {
      opacity: 0.6;
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
    font-family: 'Source Sans Pro', '微软雅黑', sans-serif;
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
    height: 30px;
    color: orange;
    padding-top: 3px;
    padding-left: 40px;
    background-color: #fff;
    border-radius: 15px;
    box-shadow: 0 0 10px 2px rgba(157, 104, 0, 0.1);
    font-weight: bold;
    opacity: 0;
    transition: all 0.5s ease-in-out;
    transform: translateY(80px);
    &.flash-tag{
      width: 120px;
      margin-left: -75px;
    }
    &.plug-tag{
      width: 140px;
      margin-left: -85px;
    }
    .flash{
      position: absolute;
      left: 20px;
      top: 6px;
      width: 12px;
    }
    .plug{
      position: absolute;
      left: 12px;
      top: 2px;
      width: 24px;
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
    transition: all 0.5s ease-in-out;
    cursor: pointer;
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
    .charging-layer{
      position: absolute;
      top: 6px;
      left: 6px;
      width: calc(100% - 12px);
      height: calc(100% - 12px);
      border-radius: 4px;
      overflow: hidden;
      .line{
        position: absolute;
        width: 3px;
        height: 100%;
        background: white;
        top: 0;
        left: 0;
        margin-left: -2px;
        animation: breath 0.8s ease-in-out alternate-reverse infinite;
        background: linear-gradient(white 25%, transparent 0,
         transparent 50%, white 0, 
         white 75%, transparent 0);
        background-size: 100% 30px;
        transition: all 0.5s ease-in-out;
      }
      &:hover .line{
        transform: scale(2, 1);
      }
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
    &:hover{
      transform: scale(1.03);
    }
    &:active{
      transition: all 0.1s ease-in-out;
      transform: scale(0.93);
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
    bottom: 60px;
    left: 50%;
    margin-left: -85px;
  }
  
  .website{
    position: absolute;
    width: 140px;
    bottom: 40px;
    left: 50%;
    margin-left: -85px;
    text-align: center;
    a{
      color: white;
      text-decoration: none;
      opacity: 0.8;
    }
  }

  .setting-panel{
    position: absolute;
    top: 20px;
    right: 20px;
    width: 550px;
    height: 470px;
    padding: 10px 30px 0;
    background-color: #fff;
    border-radius: 8px;
    box-shadow: 0 0 10px 2px rgba(157, 104, 0, 0.1);
    .language{
      position: absolute;
      top: 10px;
      right: 16px;
      opacity: 0.6;
    }
    .title{
      font-size: 18px;
      font-weight: bold;
      color: #1f3f6b;
      margin: 20px 0 12px;
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
    .time-text{
      position: absolute;
      top: 10px;
      left: 80px;
    }
    .text{
      margin-left: 10px;
      .label{
        display: inline-block;
        width: 60px;
      }
    }
  }
  .time-text{
    .text{
      margin-left: 15px;
      .label{
        display: inline-block;
        width: 60px;
      }
    }
  }
  .charging-desc{
    display: block;
    margin-top: 10px;
    color: #999;
  }
</style>
