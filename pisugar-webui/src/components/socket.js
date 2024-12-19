import Moment from 'moment'
import { ppm2ms } from '../utils'

export const initCommands = [
  'get version',
  'get model',
  'get rtc_time',
  'get system_time',
  'get rtc_alarm_enabled',
  'get rtc_alarm_time',
  'get alarm_repeat',
  'get button_enable single',
  'get button_enable double',
  'get button_enable long',
  'get button_shell single',
  'get button_shell double',
  'get button_shell long',
  'get safe_shutdown_level',
  'get safe_shutdown_delay',
  'get battery_charging_range',
  'get battery_led_amount',
  'get auto_power_on',
  'get battery_input_protect_enabled',
  'get auth_username',
  'get soft_poweroff',
  'get soft_poweroff_shell',
  'get anti_mistouch'
]

export const cycleCommands = [
  'get battery',
  'get battery_i',
  'get battery_v',
  'get battery_charging',
  'get battery_power_plugged',
  'get battery_allow_charging'
]

export async function onSocketMessage (e) {
  let msg = e.data
  if (msg.indexOf('battery') < 0) console.log(msg)
  if (msg.startsWith('model:')) {
    this.model = msg.replace('model: ', '')
    return
  }
  if (msg.startsWith('version:')) {
    this.version = msg.replace('version: ', '')
    return
  }
  if (msg.startsWith('battery:')) {
    this.batteryPercent = parseInt(msg.replace('battery: ', ''))
    return
  }
  if (msg.startsWith('battery_charging:')) {
    this.batteryCharging = msg.indexOf('true') > 0
    return
  }
  if (msg.startsWith('battery_charging_range:')) {
    const res = msg.replace('battery_charging_range: ', '')
    if (res.trim()) {
      this.chargingRange = res.split(',').map(i => parseInt(i))
    } else {
      this.chargingRange = [100, 100]
    }
    this.chargingRestartPoint = this.chargingRange[0]
    return
  }
  if (msg.startsWith('battery_led_amount:')) {
    // versions with advance hardware features
    this.isModel2Adv = msg.indexOf('2') > 0
    return
  }
  if (msg.startsWith('battery_power_plugged:')) {
    this.batteryPlugged = msg.indexOf('true') > 0
    return
  }
  if (msg.startsWith('battery_allow_charging:')) {
    this.batteryAllowCharging = msg.indexOf('true') > 0
    return
  }
  if (msg.startsWith('rtc_time:')) {
    msg = msg.replace('rtc_time: ', '').trim()
    this.rtcTime = new Moment(msg).parseZone()
    this.rtcUpdateTime = new Date().getTime()
    return
  }
  if (msg.startsWith('system_time:')) {
    msg = msg.replace('system_time: ', '').trim()
    this.sysTime = new Moment(msg).parseZone()
    this.sysUpdateTime = new Date().getTime()
    return
  }
  if (msg.startsWith('rtc_alarm_enabled:')) {
    this.alarmOptionValue = (msg.replace('rtc_alarm_enabled: ', '').trim() === 'true') ? 1 : this.alarmOptionValue
    return
  }
  if (msg.startsWith('auto_power_on:')) {
    this.alarmOptionValue = (msg.replace('auto_power_on: ', '').trim() === 'true') ? 2 : this.alarmOptionValue
    return
  }
  if (msg.startsWith('rtc_alarm_time:')) {
    // dont update timeEditValue when editing
    if (this.isTimeEditFocused) return
    msg = msg.replace('rtc_alarm_time: ', '').trim()
    const alarmTime = new Moment(msg).parseZone()
    const tempTime = new Date()
    tempTime.setSeconds(alarmTime.second())
    tempTime.setMinutes(alarmTime.minute())
    tempTime.setHours(alarmTime.hour())
    this.timeEditValue = tempTime
    console.log('update alarm time', tempTime)
    return
  }
  if (msg.startsWith('alarm_repeat:')) {
    const alarmRepeat = parseInt(msg.replace('alarm_repeat: ', ''))
    this.timeRepeat = alarmRepeat
    if (!this.timeRepeat) this.alarmOptionValue = 0
    return
  }
  if (msg.startsWith('safe_shutdown_level:')) {
    this.safeShutdown = parseInt(msg.replace('safe_shutdown_level: ', ''))
    return
  }
  if (msg.startsWith('safe_shutdown_delay:')) {
    this.safeShutdownDelay = parseInt(msg.replace('safe_shutdown_delay: ', ''))
    return
  }
  if (msg.startsWith('button_enable:')) {
    let msgArr = msg.split(' ')
    this.buttonFuncForm[msgArr[1]].enable = (msgArr[2].trim() === 'true')
    return
  }
  if (msg.startsWith('button_shell:')) {
    let msgArr = msg.split(' ')
    let shell = msg.replace(msgArr[0] + ' ' + msgArr[1] + ' ', '').replace('\n', '')
    let button = this.buttonFuncForm[msgArr[1]]
    button.shell = shell
    if (button.enable) {
      button.func = shell === 'sudo shutdown now' ? 2 : 1
    }
    return
  }
  if (msg.startsWith('battery_input_protect_enabled:')) {
    this.inputProtectEnabled = (msg.replace('battery_input_protect_enabled: ', '').trim() === 'true')
    return
  }
  if (msg.startsWith('rtc_adjust_ppm:')) {
    const ppmValue = parseInt(msg.replace('rtc_adjust_ppm: ', ''))
    if (isNaN(ppmValue)) return
    this.adjustPPM = ppmValue
    this.adjustMsPerHour = ppm2ms(this.adjustPPM)
    return
  }
  if (msg.startsWith('auth_username:')) {
    this.passwordForm.username = msg.replace('auth_username: ', '').trim()
    return
  }
  if (msg.startsWith('soft_poweroff_shell:')) {
    this.softPoweroffShell = msg.replace('soft_poweroff_shell: ', '').trim()
    this.softPoweroffShellOption = this.softPoweroffShell === 'sudo shutdown now' ? 0 : 1
    return
  }
  if (msg.startsWith('soft_poweroff:')) {
    this.softPoweroffEnabled = (msg.replace('soft_poweroff: ', '').trim() === 'true')
    return
  }
  if (msg.startsWith('anti_mistouch:')) {
    this.antiMistouchEnabled = (msg.replace('anti_mistouch: ', '').trim() === 'true')
    return
  }

  // TODO
  // show realtime button event
  if (['single', 'double', 'long'].indexOf(msg) >= 0) {
    if (msg === 'single') {
      this.singleTrigger = false
    }
    if (msg === 'double') {
      this.doubleTrigger = false
    }
    if (msg === 'long') {
      this.longTrigger = false
    }
    setTimeout(() => {
      this.singleTrigger = true
      this.doubleTrigger = true
      this.longTrigger = true
    }, 100)
  }
}
