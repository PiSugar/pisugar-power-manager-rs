<template>
  <div id="wrapper">
    <div class="center">
      <div class="battery-info">
        <!-- charging info -->
        <template v-if="isModel2">
          <div
            :class="{
              show: isModel2Adv
                ? batteryPlugged && batteryAllowCharging
                : batteryCharging
            }"
            class="charge-tag flash-tag"
          >
            <img class="flash" src="~@/assets/flash.svg" alt="" />
            <p>{{ $t('charging') }}</p>
          </div>
          <div
            :class="{
              show: isModel2Adv
                ? batteryPlugged && !batteryAllowCharging
                : false
            }"
            class="charge-tag plug-tag"
          >
            <img class="plug" src="~@/assets/plug.svg" alt="" />
            <p>{{ $t('notCharging') }}</p>
          </div>
        </template>
        <template v-if="isModel3">
          <div :class="{ show: batteryCharging }" class="charge-tag plug-tag">
            <img class="flash" src="~@/assets/flash.svg" alt="" />
            <p>{{ $t('charging') }}</p>
          </div>
        </template>

        <div class="battery-shape" @click="handleBatteryClick">
          <div
            class="battery-content"
            :class="batteryColor"
            :style="'width:' + batteryPercent + '%'"
          ></div>
          <div
            class="charging-layer"
            v-show="isModel2Adv && batteryPlugged && !batteryAllowCharging"
          >
            <div
              class="line restart-line"
              :style="`left: ${chargingRestartPoint}%`"
            ></div>
          </div>
        </div>
        <div class="battery-level">{{ batteryPercent }}%</div>
        <div class="battery-model">{{ model }}</div>
        <img class="logo" src="~@/assets/logo.svg" alt="" />
        <el-popover placement="top" trigger="hover">
          <div class="app" slot="reference" @click="jumpAppPath">
            <img class="app-logo" src="~@/assets/logo-img.svg" alt="" />
            <div class="app-desc">
              <div class="desc-line">Download</div>
              <div class="desc-line bold">PiSugar APP</div>
            </div>
          </div>
          <div>
            <img class="qrcode" src="~@/assets/qrcode.png" alt="" />
          </div>
        </el-popover>
        <div class="website">
          <a href="http://www.pisugar.com" target="_blank">www.pisugar.com</a>
        </div>
      </div>

      <div class="setting-panel">
        <div class="global">
          <el-link type="info" @click="languageDialog = true"
            >Language: {{ locale }}</el-link
          >
          |
          <el-link type="info" @click="passwordDialog = true">Account</el-link>
          |
          <el-link type="info" @click="settingDialog = true">Setting</el-link>
        </div>
        <div class="title">{{ $t('wakeUpFeature') }}</div>
        <el-row>
          <el-select
            v-model="alarmOptionValue"
            placeholder="Select"
            :disabled="!socketConnect"
            @change="alarmOptionValueChange"
          >
            <el-option
              v-for="item in alarmOption"
              :key="item.value"
              :label="item.label"
              :value="item.value"
            >
            </el-option>
          </el-select>
          <el-time-picker
            class="time-picker"
            v-model="timeEditValue"
            v-if="alarmOptionValue !== 2"
            :disabled="alarmOptionValue === 0 || !socketConnect"
            :picker-options="{
              selectableRange: '00:00:00 - 23:59:59'
            }"
            @change="timeEditChange"
            @focus="isTimeEditFocused = true"
            @blur="isTimeEditFocused = false"
            placeholder="select anytime"
          >
          </el-time-picker>
          <el-button
            v-if="alarmOptionValue === 1"
            :disabled="!socketConnect"
            @click="repeatDialog = true"
            >{{ $t('repeat') }}</el-button
          >
        </el-row>
        <el-row>
          <p class="desc">{{ alarmMessage }}</p>
        </el-row>
        <div class="title">{{ $t('buttonFunction') }}</div>
        <el-row>
          <el-form
            ref="buttonFuncFormSingle"
            :model="buttonFuncForm.single"
            label-width="125px"
          >
            <el-form-item :label="$t('singleTapLabel')">
              <el-select
                v-model="buttonFuncForm.single.func"
                placeholder="Select"
                :disabled="!socketConnect"
                @change="buttonFuncChange('single')"
              >
                <el-option
                  v-for="item in buttonFuncForm.single.options"
                  :key="item.value"
                  :label="item.label"
                  :value="item.value"
                >
                </el-option>
              </el-select>
              <el-button
                v-if="buttonFuncForm.single.func === 1"
                @click="openShellEdit('single')"
                :disabled="!socketConnect"
                >{{ $t('edit') }}</el-button
              >
              <span class="tag-span"
                ><el-tag :type="singleTrigger ? 'success' : ''">{{
                  $t('triggered')
                }}</el-tag></span
              >
            </el-form-item>
          </el-form>
        </el-row>
        <el-row>
          <el-form
            ref="buttonFuncFormDouble"
            :model="buttonFuncForm.double"
            label-width="125px"
          >
            <el-form-item :label="$t('doubleTapLabel')">
              <el-select
                v-model="buttonFuncForm.double.func"
                placeholder="Select"
                :disabled="!socketConnect"
                @change="buttonFuncChange('double')"
              >
                <el-option
                  v-for="item in buttonFuncForm.double.options"
                  :key="item.value"
                  :label="item.label"
                  :value="item.value"
                >
                </el-option>
              </el-select>
              <el-button
                v-if="buttonFuncForm.double.func === 1"
                @click="openShellEdit('double')"
                :disabled="!socketConnect"
                >{{ $t('edit') }}</el-button
              >
              <span class="tag-span"
                ><el-tag :type="doubleTrigger ? 'success' : ''">{{
                  $t('triggered')
                }}</el-tag></span
              >
            </el-form-item>
          </el-form>
        </el-row>
        <el-row>
          <el-form
            ref="buttonFuncFormLong"
            :model="buttonFuncForm.long"
            label-width="125px"
          >
            <el-form-item :label="$t('longTapLabel')">
              <el-select
                v-model="buttonFuncForm.long.func"
                placeholder="Select"
                :disabled="!socketConnect"
                @change="buttonFuncChange('long')"
              >
                <el-option
                  v-for="item in buttonFuncForm.long.options"
                  :key="item.value"
                  :label="item.label"
                  :value="item.value"
                >
                </el-option>
              </el-select>
              <el-button
                v-if="buttonFuncForm.long.func === 1"
                @click="openShellEdit('long')"
                :disabled="!socketConnect"
                >{{ $t('edit') }}</el-button
              >
              <span class="tag-span"
                ><el-tag :type="longTrigger ? 'success' : ''">{{
                  $t('triggered')
                }}</el-tag></span
              >
            </el-form-item>
          </el-form>
        </el-row>
        <div class="title">{{ $t('safeShutdown') }}</div>
        <el-row>
          <el-select
            v-model="safeShutdown"
            placeholder="Please Select"
            :disabled="!socketConnect"
            @change="safeShutdownChange"
          >
            <el-option
              v-for="item in safeShutdownOpts"
              :key="item.value"
              :label="item.label"
              :value="item.value"
            >
            </el-option>
          </el-select>
          <el-select
            v-if="safeShutdown"
            v-model="safeShutdownDelay"
            placeholder="Please Select"
            :disabled="!socketConnect"
            @change="safeShutdownDelayChange"
          >
            <el-option
              v-for="item in safeShutdownDelayOpts"
              :key="item.value"
              :label="item.label"
              :value="item.value"
            >
            </el-option>
          </el-select>
        </el-row>
      </div>

      <div class="rtc-panel">
        <div class="sys-info">
          <el-button
            icon="el-icon-refresh"
            circle
            @click="timeDialog = true"
          ></el-button>
          <div class="time-text">
            <div class="text rtc">
              <span class="label">{{ $t('rtcTime') }}</span> :
              {{ rtcTimeString }}
            </div>
            <div class="text sys">
              <span class="label">{{ $t('sysTime') }}</span> :
              {{ sysTimeString }}
            </div>
          </div>
        </div>
        <div class="version">
          {{ version && `PiSugar-server version ${version}` }}
        </div>
      </div>

      <el-dialog :title="$t('repeat')" :visible.sync="repeatDialog">
        <el-row>
          <el-checkbox v-model="alarmRepeatSun">{{
            $t('weekDay.Sunday')
          }}</el-checkbox>
          <el-checkbox v-model="alarmRepeatMon">{{
            $t('weekDay.Monday')
          }}</el-checkbox>
          <el-checkbox v-model="alarmRepeatTue">{{
            $t('weekDay.Tuesday')
          }}</el-checkbox>
          <el-checkbox v-model="alarmRepeatWed">{{
            $t('weekDay.Wednesday')
          }}</el-checkbox>
          <el-checkbox v-model="alarmRepeatThu">{{
            $t('weekDay.Thursday')
          }}</el-checkbox>
          <el-checkbox v-model="alarmRepeatFri">{{
            $t('weekDay.Friday')
          }}</el-checkbox>
          <el-checkbox v-model="alarmRepeatSat">{{
            $t('weekDay.Saturday')
          }}</el-checkbox>
        </el-row>
        <el-row class="mt20">
          <el-button size="mini" @click="checkRepeatAll">{{
            $t('checkAll')
          }}</el-button>
          <el-button size="mini" @click="uncheckRepeatAll">{{
            $t('clearAll')
          }}</el-button>
        </el-row>
        <br />
      </el-dialog>

      <!-- time setting dialog -->
      <el-dialog :title="$t('syncTime')" :visible.sync="timeDialog">
        <el-row>
          <div class="time-text">
            <div class="text rtc">
              <span class="label">{{ $t('rtcTime') }}</span> :
              {{ rtcTimeString }}
            </div>
            <div class="text sys">
              <span class="label">{{ $t('sysTime') }}</span> :
              {{ sysTimeString }}
            </div>
          </div>
        </el-row>
        <br />
        <el-row>
          <el-button @click="syncPi2RTC">Pi > RTC</el-button>
          <el-button @click="syncRTC2Pi">RTC > Pi</el-button>
          <el-button @click="syncWebTime">Web > Pi & RTC</el-button>
        </el-row>
        <br />
        <!-- model3 rtc adjustment setting -->
        <el-row v-if="model.indexOf('3') > -1">
          <el-form>
            <el-form-item label="Adjust(ms per hour)" label-width="130px">
              <el-input-number
                size="small"
                v-model="adjustMsPerHour"
                controls-position="right"
                @change="handleAdjustChange"
                :min="-1800"
                :max="1800"
              ></el-input-number>
              <div>ppm: {{ adjustPPM }}</div>
            </el-form-item>
          </el-form>
        </el-row>
      </el-dialog>

      <!-- shell dialog -->
      <el-dialog :title="editShellDialogTitle" :visible.sync="editShellDialog">
        <el-row>
          <el-form :model="editShellDialogForm">
            <el-form-item label="Shell" label-width="50px">
              <el-input
                v-model="editShellDialogCache"
                autocomplete="off"
                :placeholder="$t('shellPlaceholder')"
              ></el-input>
            </el-form-item>
          </el-form>
        </el-row>
        <br />
        <div slot="footer" class="dialog-footer">
          <el-button @click="closeShellEdit">{{ $t('cancel') }}</el-button>
          <el-button
            type="primary"
            @click="buttonFuncChange(editShellDialogForm.type)"
            >{{ $t('confirm') }}</el-button
          >
        </div>
      </el-dialog>

      <!-- language dialog -->
      <el-dialog :title="$t('selectLanguage')" :visible.sync="languageDialog">
        <el-row>
          <el-form>
            <el-form-item :label="$t('language')" label-width="100px">
              <el-select
                v-model="locale"
                placeholder="Select"
                @change="languageChange"
              >
                <el-option
                  v-for="item in languageOptions"
                  :key="item.value"
                  :label="item.label"
                  :value="item.value"
                >
                </el-option>
              </el-select>
            </el-form-item>
          </el-form>
        </el-row>
        <div slot="footer" class="dialog-footer">
          <el-button @click="languageDialog = false">{{
            $t('cancel')
          }}</el-button>
          <el-button type="primary" @click="languageConfirm">{{
            $t('confirm')
          }}</el-button>
        </div>
      </el-dialog>

      <!-- charging setting dialog -->
      <el-dialog :title="$t('advanceSetting')" :visible.sync="settingDialog">
        <el-form>
          <!-- version 3 -->
          <template v-if="isModel3">
            <el-form-item :label="$t('batteryInputProtect')">
              <el-switch
                v-model="inputProtectEnabled"
                :active-text="$t('enabled')"
                :inactive-text="$t('disabled')"
              >
              </el-switch>
              <span class="form-item-desc">{{
                $t('batteryInputProtectDesc')
              }}</span>
            </el-form-item>
            <el-form-item :label="$t('antiMistouch')">
              <el-switch
                v-model="antiMistouchEnabled"
                :active-text="$t('enabled')"
                :inactive-text="$t('disabled')"
              >
              </el-switch>
              <span class="form-item-desc">{{ $t('antiMistouchDesc') }}</span>
            </el-form-item>
            <el-form-item :label="$t('softPoweroff')">
              <el-switch
                v-model="softPoweroffEnabled"
                :active-text="$t('enabled')"
                :inactive-text="$t('disabled')"
              >
              </el-switch>
              <span class="form-item-desc">{{ $t('softPoweroffDesc') }}</span>
            </el-form-item>
            <el-form-item
              v-if="softPoweroffEnabled"
              :label="$t('softPoweroffShell')"
            >
              <el-select
                v-model="softPoweroffShellOption"
                placeholder="Select"
                @change="softShutdownShellChange"
              >
                <el-option
                  v-for="item in softPoweroffShellOptions"
                  :key="item.value"
                  :label="item.label"
                  :value="item.value"
                >
                </el-option>
              </el-select>
              <div class="form-item-desc soft-off-shell">
                {{ $t('softPoweroffShellDesc') }}
              </div>
              <el-input
                v-model="softPoweroffShell"
                autocomplete="off"
                :placeholder="$t('shellPlaceholder')"
                :disabled="softPoweroffShellOption === 0"
              ></el-input>
            </el-form-item>
          </template>
          <!-- version 2 -->
          <el-form-item v-if="isModel2">
            <el-row>
              <el-slider
                v-model="chargingRestartPoint"
                :min="50"
                :max="100"
                :format-tooltip="formatTooltip"
                :marks="chargingMarks"
                show-input
              >
              </el-slider>
            </el-row>
            <el-row>
              <span class="charging-desc">{{ chargingDesc }}</span>
            </el-row>
          </el-form-item>
        </el-form>
        <div slot="footer" class="dialog-footer">
          <el-button @click="settingDialog = false">{{
            $t('cancel')
          }}</el-button>
          <el-button type="primary" @click="handleAdvanceSettingConfirm">{{
            $t('confirm')
          }}</el-button>
        </div>
      </el-dialog>

      <!-- account dialog -->
      <el-dialog
        :title="$t('changeLoginPassword')"
        :visible.sync="passwordDialog"
      >
        <el-row>
          <el-form
            :model="passwordForm"
            ref="passwordForm"
            :rules="passwordRules"
          >
            <el-form-item
              :label="$t('username')"
              label-width="150px"
              prop="username"
            >
              <el-input
                v-model="passwordForm.username"
                autocomplete="off"
                :placeholder="$t('username')"
              ></el-input>
            </el-form-item>
            <el-form-item
              :label="$t('password')"
              label-width="150px"
              prop="password"
            >
              <el-input
                v-model="passwordForm.password"
                type="password"
                autocomplete="off"
                :placeholder="$t('password')"
              ></el-input>
            </el-form-item>
            <el-form-item label-width="150px" prop="passwordConfirm">
              <el-input
                v-model="passwordForm.passwordConfirm"
                type="password"
                autocomplete="off"
                :placeholder="$t('passwordConfirm')"
              ></el-input>
            </el-form-item>
          </el-form>
        </el-row>
        <div slot="footer" class="dialog-footer">
          <el-button @click="passwordDialog = false">{{
            $t('cancel')
          }}</el-button>
          <el-button type="primary" @click="passwordSubmit">{{
            $t('confirm')
          }}</el-button>
        </div>
      </el-dialog>

      <!-- login dialog -->
      <el-dialog
        :title="$t('loginDialog')"
        :visible.sync="loginDialog"
        :close-on-press-escape="false"
        :close-on-click-modal="false"
        :show-close="false"
      >
        <el-row>
          <el-form
            :model="loginForm"
            ref="loginForm"
          >
            <el-form-item
              :label="$t('username')"
              label-width="150px"
              prop="username"
            >
              <el-input
                v-model="loginForm.username"
                :placeholder="$t('username')"
              ></el-input>
            </el-form-item>
            <el-form-item
              :label="$t('password')"
              label-width="150px"
              prop="password"
            >
              <el-input
                v-model="loginForm.password"
                type="password"
                :placeholder="$t('password')"
              ></el-input>
            </el-form-item>
          </el-form>
        </el-row>
        <div slot="footer" class="dialog-footer">
          <el-button type="primary" @click="submitLogin">{{
            $t('login')
          }}</el-button>
        </div>
      </el-dialog>
    </div>
  </div>
</template>

<script>
import Moment from 'moment'
import { localeOptions } from '../locale'
import { ms2ppm } from '../utils'
import { onSocketMessage, initCommands, cycleCommands } from './socket'

const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
let webSocketHost = `${wsProtocol}//${window.location.hostname}:${window.location.port}/ws`
let loginApi = `${window.location.protocol}//${window.location.hostname}:${window.location.port}/login`
// const devIp = '192.168.100.176'
// const devWsHost = `ws://${devIp}:8421/ws`
// const devLoginApi = `http://${devIp}:8421/login`
// webSocketHost = devWsHost
// loginApi = devLoginApi

export default {
  name: 'index-page',
  debug: true,
  components: {},
  data () {
    return {
      version: '',
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
      isModel2Adv: false,
      alarmOption: [
        { label: this.$t('disabled'), value: 0 },
        { label: this.$t('enabled'), value: 1 },
        { label: this.$t('onPowerRestore'), value: 2 }
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
        { label: `${this.$t('batteryLevel')} <= 5%`, value: 5 },
        { label: `${this.$t('batteryLevel')} <= 10%`, value: 10 },
        { label: `${this.$t('batteryLevel')} <= 20%`, value: 20 },
        { label: `${this.$t('batteryLevel')} <= 30%`, value: 30 }
      ],
      safeShutdownDelay: 0,
      safeShutdownDelayOpts: Array(121)
        .fill(0)
        .map((i, k) => {
          return {
            label: k
              ? `${k} ${this.$t('secondsDelay')}`
              : `${this.$t('immediately')}`,
            value: k
          }
        }),
      chargingRange: [-1, -1],
      chargingRestartPoint: 50,
      timeDialog: false,
      locale: 'en',
      languageDialog: false,
      languageOptions: localeOptions,
      settingDialog: false,
      chargingMarks: {
        80: '80%'
      },
      timeUpdaterCount: 0,
      inputProtectEnabled: false,
      softPoweroffEnabled: false,
      softPoweroffShellOption: 0,
      softPoweroffShellOptions: [
        { label: this.$t('shutdown'), value: 0 },
        { label: this.$t('customShell'), value: 1 }
      ],
      softPoweroffShell: '',
      antiMistouchEnabled: false,
      isTimeEditFocused: false,
      adjustPPM: 0,
      adjustMsPerHour: 0,
      passwordDialog: false,
      passwordForm: {
        username: '',
        password: '',
        passwordConfirm: ''
      },
      loginDialog: false,
      loginForm: {
        username: localStorage.getItem('username') || '',
        password: localStorage.getItem('password') || ''
      },
      loginToken: '',
      passwordRules: {
        username: [
          {
            required: true,
            message: this.$t('usernameCannotBeEmpty'),
            trigger: 'blur'
          },
          {
            validator: (rule, value, callback) => {
              if (value && value.indexOf(' ') > -1) {
                callback(this.$t('usernameWithSpace'))
              } else {
                callback()
              }
            },
            trigger: 'blur'
          }
        ],
        password: [
          {
            required: true,
            message: this.$t('passwordCannotBeEmpty'),
            trigger: 'blur'
          },
          {
            validator: (rule, value, callback) => {
              if (value && value.indexOf(' ') > -1) {
                callback(this.$t('passwordWithSpace'))
              } else {
                callback()
              }
            },
            trigger: 'blur'
          }
        ],
        passwordConfirm: [
          {
            validator: (rule, value, callback) => {
              if (value !== this.passwordForm.password) {
                callback(this.$t('passwordNotConsistent'))
              } else {
                callback()
              }
            },
            trigger: 'blur'
          }
        ]
      }
    }
  },

  mounted () {
    setTimeout(() => {
      this.timeUpdater()
    }, 1000)
    this.locale = this.$i18n.locale
    console.log(this.$i18n.locale)
    this.checkAuth()
      .then((auth) => {
        console.log('Auth result:', auth)
        if (auth) {
          this.createWebSocketClient()
        }
      })
  },

  computed: {
    batteryColor () {
      if (this.batteryPercent < 10) return 'red'
      if (this.batteryPercent < 30) return 'yellow'
      return 'green'
    },
    isModel2 () {
      return this.model.indexOf(' 2') > 0
    },
    isModel3 () {
      return this.model.indexOf(' 3') > 0
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
      switch (this.alarmOptionValue) {
        case 0:
          return `${this.$t('wakeUpOffDesc')}`
        case 1:
          let repeatMessage = ''
          if (this.timeRepeat === 127) {
            repeatMessage = this.$t('repeatEveryday')
          } else {
            let repeatArray = []
            let days = [
              'Sat',
              'Fri',
              'Thu',
              'Wed',
              'Tue',
              'Mon',
              'Sun'
            ].reverse()
            for (let i = 0; i < 7; i++) {
              if (this.getBit(this.timeRepeat, i)) {
                repeatArray.push(this.$t(`weekDayShort.${days[i]}`))
              }
            }
            repeatMessage = `${this.$t('repeatOn')} ${repeatArray.join(', ')}`
          }
          return `${this.$t('wakeUpDesc')} ${
            this.timeEditValue.toTimeString().split(' ')[0]
          }, ${repeatMessage}`
        case 2:
          if (this.isModel3) return this.$t('powerWakeDescFor3')
          return this.$t('powerWakeDesc')
        default:
          break
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
    alarmOptionValue: function (val, oldVal) {
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
      if (this.isModel2 && oldVal === 2 && val !== 2) {
        this.$alert(
          this.$t('powerWakeOffWarning'),
          this.$t('powerWakeOffTitle'),
          {
            confirmButtonText: this.$t('confirmButtonText')
          }
        )
      }
    },
    timeRepeat: function (val) {
      if (!this.repeatDialog) return
      // 改了timeRepeat则开启定时开机
      this.alarmOptionValue = val === 0 ? 0 : 1
      this.setRtcAlarm()
    },
    timeDialog: function (val) {
      if (val) {
        this.$socket.send('get rtc_adjust_ppm')
      }
    }
  },

  methods: {
    async checkAuth () {
      const username = localStorage.getItem('username') || ''
      const password = localStorage.getItem('password') || ''
      // POST /login
      const response = await fetch(`${loginApi}?username=${encodeURIComponent(username)}&password=${encodeURIComponent(password)}`, {
        method: 'POST',
        headers: {
          // cross-origin
          'Access-Control-Allow-Origin': '*'
        }
      }).catch((error) => {
        console.error('Error during authentication:', error)
        this.$message({
          message: `[Authentication] ${error.message}`,
          type: 'error'
        })
        return false
      }).then((res) => {
        // jwt string in res
        console.log('Authentication response:', res)
        if (res.status === 401) {
          this.loginDialog = true
          return false
        }
        if (res.status !== 200) {
          if (res.status) {
            this.$message({
              message: `[Authentication] Response code ${res.status}`,
              type: 'error'
            })
          }
          return false
        }
        res.text().then((bodyText) => {
          console.log('Authentication response body:', bodyText)
          localStorage.setItem('token', bodyText)
          this.loginToken = bodyText
        })
        return true
      })
      return response
    },
    submitLogin () {
      const { username, password } = this.loginForm
      this.loginDialog = false
      localStorage.setItem('username', username)
      localStorage.setItem('password', password)
      this.checkAuth().then((auth) => {
        console.log('Auth result:', auth)
        if (auth) {
          setTimeout(() => {
            this.createWebSocketClient()
          }, 800)
        }
      })
    },
    getBit (n, pos) {
      return (n & (1 << pos)) > 0
    },
    setBit (n, pos, v) {
      if (v) {
        return n | (1 << pos)
      } else {
        return n & ~(1 << pos)
      }
    },
    createWebSocketClient () {
      const token = this.loginToken || localStorage.getItem('token') || ''
      const url = `${webSocketHost}?token=${encodeURIComponent(token)}`
      console.log(`[Websocket CLIENT] connecting to ${url}`)
      this.$connect(url)
      this.$socket.onopen = () => {
        console.log(`[Websocket CLIENT] open()`)
        this.getBatteryInfo(true)
      }
      this.$socket.onerror = (error) => {
        console.error(`[Websocket CLIENT] error()`, error)
      }
    },
    sendSocketCommands (cmds) {
      cmds.forEach(cmd => {
        this.$socket.send(cmd)
      })
    },
    bindSocket () {
      this.$socket.onmessage = onSocketMessage.bind(this)
    },
    getBatteryInfo (loop) {
      if (this.$socket.readyState === 1) {
        if (!this.socketConnect) {
          this.bindSocket()
          this.sendSocketCommands(initCommands)
        }
        this.socketConnect = true
        this.sendSocketCommands(cycleCommands)
      } else {
        this.socketConnect = false
        this.batteryPercent = 0
        this.batteryCharging = false
        this.model = 'Not Available'
      }
      if (loop) {
        setTimeout(() => {
          this.getBatteryInfo(true)
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
        this.timeUpdater()
      }, 1000)
      if (this.timeUpdaterCount % 5 === 0) {
        this.getDeviceTime()
      }
      // get alarm time in every 10s
      if (this.timeUpdaterCount % 10 === 0) {
        this.$socket && this.$socket.send('get rtc_alarm_time')
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
      switch (this.alarmOptionValue) {
        case 0:
          this.$socket.send('set_auto_power_on false')
          this.$socket.send('rtc_alarm_disable')
          this.$socket.send('get rtc_alarm_enabled')
          this.$socket.send('get rtc_alarm_time')
          break
        case 1:
          this.$socket.send('set_auto_power_on false')
          if (this.timeRepeat === 0) {
            this.timeRepeat = 127
          }
          this.setRtcAlarm()
          break
        case 2:
          this.$socket.send('set_auto_power_on true')
          break
        default:
          break
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
    handleAdvanceSettingConfirm () {
      this.settingDialog = false
      if (this.isModel2Adv) {
        const value =
          this.chargingRestartPoint === 100
            ? ''
            : ` ${this.chargingRestartPoint},100`
        this.$socket.send(`set_battery_charging_range${value}`)
      }
      if (this.isModel3) {
        this.$socket.send(
          `set_battery_input_protect ${!!this.inputProtectEnabled}`
        )
        this.$socket.send(`set_anti_mistouch ${!!this.antiMistouchEnabled}`)
        this.$socket.send(`set_soft_poweroff ${!!this.softPoweroffEnabled}`)
        this.$socket.send(`set_soft_poweroff_shell ${this.softPoweroffShell}`)
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
      var alarmTime = new Moment()
        .second(sec)
        .minute(min)
        .hour(hour)
        .parseZone()
      if (this.sysTime) {
        alarmTime.utcOffset(this.sysTime.utcOffset(), true)
      }
      this.$socket.send(
        `rtc_alarm_set ${alarmTime.toISOString(true)} ${this.timeRepeat}`
      )
    },
    handleBatteryClick () {
      if (this.isModel2Adv || this.isModel3) {
        this.settingDialog = true
      }
    },
    handleAdjustChange () {
      this.adjustPPM = ms2ppm(this.adjustMsPerHour)
      this.$socket.send(`rtc_adjust_ppm ${this.adjustPPM}`)
    },
    passwordSubmit () {
      this.$refs['passwordForm'].validate(valid => {
        if (valid) {
          this.passwordDialog = false
          const { username, password } = this.passwordForm
          this.$socket.send(`set_auth ${username} ${password}`)
          this.$message({
            message: this.$t('changePasswordSuccess'),
            type: 'success',
            duration: 3000
          })
        }
      })
    },
    softShutdownShellChange (val) {
      if (val === 0) this.softPoweroffShell = 'sudo shutdown now'
    },
    jumpAppPath () {
      window.open('https://www.pisugar.com/app')
    }
  }
}
</script>

<style>
@font-face {
  font-family: 'Source Sans Pro';
  src: url('~@/assets/source_sans_pro.woff2');
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
  padding-right: 0px !important;
}
.setting-panel .el-date-editor.el-input,
.el-date-editor.el-input__inner {
  width: 160px;
}
.el-row {
  margin-top: 6px;
}
.setting-panel .el-form-item__label {
  text-align: left;
}
.setting-panel .el-form-item {
  margin-bottom: 10px;
}
.tag-span .el-tag {
  display: none;
  opacity: 1;
}
.tag-span .el-tag.el-tag--success {
  display: inline-block;
  animation: show-once 2s ease-in-out forwards;
}
.el-checkbox {
  width: 100px;
}
.mt20 {
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
.center {
  position: relative;
  width: 900px;
  margin: 0 auto;
  text-align: left;
}
.battery-info {
  position: absolute;
  top: 0;
  left: 0;
  width: 350px;
  height: 595px;
}

.charge-tag {
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
  &.flash-tag {
    width: 120px;
    margin-left: -75px;
  }
  &.plug-tag {
    min-width: 120px;
    margin-left: -85px;
    padding-right: 15px;
  }
  .flash {
    position: absolute;
    left: 20px;
    top: 6px;
    width: 12px;
  }
  .plug {
    position: absolute;
    left: 12px;
    top: 2px;
    width: 24px;
  }
  &.show {
    transform: translateY(0);
    opacity: 1;
  }
}
.battery-shape {
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
  &:before {
    display: block;
    position: absolute;
    content: ' ';
    width: 30px;
    height: 30px;
    background-color: #fff;
    right: -15px;
    top: 25px;
    border-radius: 6px;
  }
  .charging-layer {
    position: absolute;
    top: 6px;
    left: 6px;
    width: calc(100% - 12px);
    height: calc(100% - 12px);
    border-radius: 4px;
    overflow: hidden;
    .line {
      position: absolute;
      width: 3px;
      height: 100%;
      background: white;
      top: 0;
      left: 0;
      margin-left: -2px;
      animation: breath 0.8s ease-in-out alternate-reverse infinite;
      background: linear-gradient(
        white 25%,
        transparent 0,
        transparent 50%,
        white 0,
        white 75%,
        transparent 0
      );
      background-size: 100% 30px;
      transition: all 0.5s ease-in-out;
    }
    &:hover .line {
      transform: scale(2, 1);
    }
  }
  .battery-content {
    position: relative;
    width: 0%;
    height: 100%;
    border-radius: 4px;
    transition: all 1s ease-in-out;
    &.green {
      background-color: #88e61b;
    }
    &.red {
      background-color: #ff521c;
    }
    &.yellow {
      background-color: #ffd100;
    }
  }
  &:hover {
    transform: scale(1.03);
  }
  &:active {
    transition: all 0.1s ease-in-out;
    transform: scale(0.93);
  }
}

.battery-level {
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

.battery-model {
  position: absolute;
  top: 340px;
  left: 80px;
  width: 160px;
  height: 80px;
  text-align: center;
  color: #fff;
  font-size: 16px;
}

.logo {
  position: absolute;
  width: 140px;
  bottom: 130px;
  left: 50%;
  margin-left: -85px;
}

.qrcode {
  width: 150px;
  height: 150px;
}

.app {
  position: absolute;
  width: 140px;
  bottom: 65px;
  left: 50%;
  margin-left: -85px;
  text-align: center;
  border-radius: 6px;
  border: 1px solid #fff;
  display: flex;
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
  padding: 5px 10px;
  cursor: pointer;
  &:hover {
    background-color: rgba(255, 255, 255, 0.2);
  }
  .app-logo {
    width: 30px;
    height: 30px;
  }
  .app-desc {
    text-align: left;
    color: white;
    font-size: 12px;
    .desc-line.bold {
      font-size: 14px;
      font-weight: bold;
    }
  }
  a {
    color: white;
    text-decoration: none;
    opacity: 0.8;
  }
}

.website {
  position: absolute;
  width: 140px;
  bottom: 40px;
  left: 50%;
  margin-left: -85px;
  text-align: center;
  a {
    color: white;
    text-decoration: none;
    opacity: 0.8;
  }
}

.setting-panel {
  position: absolute;
  top: 20px;
  right: 20px;
  width: 550px;
  min-height: 470px;
  padding: 10px 30px 20px;
  background-color: #fff;
  border-radius: 8px;
  box-shadow: 0 0 10px 2px rgba(157, 104, 0, 0.1);
  .global {
    position: absolute;
    top: 10px;
    right: 16px;
    opacity: 0.6;
    color: #ccc;
  }
  .title {
    font-size: 18px;
    font-weight: bold;
    color: #1f3f6b;
    margin: 20px 0 12px;
  }
  .desc {
    color: #a2a6b8;
    font-size: 12px;
  }
}
.rtc-panel {
  position: absolute;
  top: 500px;
  right: 20px;
  width: 550px;
  height: 60px;
  padding: 0 30px;
  background-color: #fff;
  border-radius: 8px;
  box-shadow: 0 0 10px 2px rgba(157, 104, 0, 0.1);
  .version {
    position: absolute;
    right: 0;
    bottom: -20px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.8);
  }
}
.sys-info {
  margin-top: 10px;
  font-size: 14px;
  color: #999;
  .time-text {
    position: absolute;
    top: 10px;
    left: 80px;
  }
  .text {
    margin-left: 10px;
    .label {
      display: inline-block;
      width: 90px;
    }
  }
}
.time-text {
  .text {
    margin-left: 15px;
    .label {
      display: inline-block;
      width: 110px;
    }
  }
}
.charging-desc {
  display: block;
  margin-top: 10px;
  color: #999;
}
.form-item-desc {
  display: block;
  color: #999;
  font-size: 13px;
  line-height: 20px;
  word-break: normal;
}
.soft-off-shell {
  margin-top: 10px;
}
</style>
