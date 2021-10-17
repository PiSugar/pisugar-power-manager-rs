'use strict'
import electron from 'electron'
const {
  app,
  BrowserWindow,
  Menu,
  ipcMain
} = electron

/**
 * Set `__static` path to static files in production
 * https://simulatedgreg.gitbooks.io/electron-vue/content/en/using-static-assets.html
 */
if (process.env.NODE_ENV !== 'development') {
  global.__static = require('path').join(__dirname, '/static').replace(/\\/g, '\\\\')
}

let template = [
  {
    label: 'File',
    submenu: [{
      label: 'Close',
      click: function (item, focusedWindow) {
        if (focusedWindow) {
          BrowserWindow.getAllWindows().forEach(function (win) {
            win.close()
          })
        }
      }
    }]
  },
  {
    label: 'Time',
    submenu: [{
      label: 'Sync Time Pi => RTC',
      click: function () {
        console.log('Sync Time Pi => RTC')
      }
    }, {
      label: 'Sync Time RTC => Pi',
      click: function () {
        console.log('Sync Time RTC => Pi')
      }
    }, {
      label: 'Sync Time Internet => Pi & RTC',
      click: function () {
        console.log('Sync Time Internet => Pi & RTC')
      }
    }]
  },
  {
    label: 'Help',
    role: 'help',
    submenu: [{
      label: 'About PiSugar Power Manager v1.0',
      click: function () {
        electron.shell.openExternal('https://www.pisugar.com')
      }
    }]
  }
]

let mainWindow
const winURL = process.env.NODE_ENV === 'development'
  ? `http://localhost:9080`
  : `file://${__dirname}/index.html`

function createWindow () {
  /**
   * Initial window options
   */
  mainWindow = new BrowserWindow({
    useContentSize: true,
    width: 900,
    height: 580,
    // frame: false,
    resizable: true,
    webPreferences: {
      webSecurity: false,
      nodeIntegration: true,
      nodeIntegrationInWorker: true
    }
  })

  mainWindow.loadURL(winURL)
  ipcMain.on('f12', () => {
    mainWindow.webContents.openDevTools()
  })

  mainWindow.on('closed', () => {
    mainWindow = null
  })

  const menu = Menu.buildFromTemplate(template)
  Menu.setApplicationMenu(menu) // 设置菜单部分

  console.log('set menu')
}

app.on('ready', createWindow)

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

app.on('activate', () => {
  if (mainWindow === null) {
    createWindow()
  }
})

/**
 * Auto Updater
 *
 * Uncomment the following code below and install `electron-updater` to
 * support auto updating. Code Signing with a valid certificate is required.
 * https://simulatedgreg.gitbooks.io/electron-vue/content/en/using-electron-builder.html#auto-updating
 */

/*
import { autoUpdater } from 'electron-updater'

autoUpdater.on('update-downloaded', () => {
  autoUpdater.quitAndInstall()
})

app.on('ready', () => {
  if (process.env.NODE_ENV === 'production') autoUpdater.checkForUpdates()
})
 */
