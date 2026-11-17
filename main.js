'use strict';

const { app, BrowserWindow, globalShortcut, ipcMain, screen } = require('electron');
const path = require('path');
const fs = require('fs');

const CONFIG_PATH = path.join(app.getPath('userData'), 'config.json');

const DEFAULTS = {
  // 底部控制列離畫面下緣幾 px（1080p 設計座標，會隨解析度等比縮放）
  barBottom: 6,
  // 影片標題
  title: '《魔獸世界：永恆》開服不到兩週！暴雪緊急改規則？地城練等、陣營限制一次看！　#魔獸世界永恆 #魔獸世界',
  // 左下角狀態文字
  label: '直播中',
  // 顯示標題
  showTitle: true,
  // 開關鍵
  toggleHotkey: 'Control+Alt+Shift+Y',
  // 校準鍵
  calibrateHotkey: 'Control+Alt+Shift+K',
};

let cfg = { ...DEFAULTS };
let win = null;
let armed = false;
let calibrating = false;
let calValue = 0;
let calOriginal = 0;

function loadConfig() {
  try {
    cfg = { ...DEFAULTS, ...JSON.parse(fs.readFileSync(CONFIG_PATH, 'utf8')) };
  } catch {
    cfg = { ...DEFAULTS };
  }
}

function saveConfig() {
  try {
    fs.mkdirSync(path.dirname(CONFIG_PATH), { recursive: true });
    fs.writeFileSync(CONFIG_PATH, JSON.stringify(cfg, null, 2), 'utf8');
  } catch (err) {
    console.error('save config failed', err);
  }
}

function send(channel, payload) {
  if (win && !win.isDestroyed()) win.webContents.send(channel, payload);
}

function createWindow() {
  const { bounds } = screen.getPrimaryDisplay();

  win = new BrowserWindow({
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
    transparent: true,
    frame: false,
    resizable: false,
    movable: false,
    skipTaskbar: false,
    focusable: false,
    hasShadow: false,
    fullscreenable: false,
    backgroundColor: '#00000000',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      backgroundThrottling: false,
    },
  });

  // 壓在遊戲之上（screen-saver 是 Windows 最高層級）
  win.setAlwaysOnTop(true, 'screen-saver');
  // 滑鼠完全穿透：點擊與移動都會落到遊戲，遊戲永遠保持焦點
  win.setIgnoreMouseEvents(true, { forward: true });
  win.loadFile('index.html');
}

// 視窗「永遠不藏」，只切換內容透明度。
// 反覆 win.hide()/show() 在 Windows 上會讓透明視窗重建合成層而變黑底，
// 這是 Electron transparent window 的長期問題；改成常駐＋CSS 切換可完全避開。
function showOverlay() {
  armed = true;
  win.showInactive();
  send('armed', true);
}

function hideOverlay() {
  armed = false;
  send('armed', false);
}

function toggle() {
  if (calibrating) return;
  armed ? hideOverlay() : showOverlay();
}

function registerCalibrateKeys() {
  globalShortcut.register('Up', () => {
    calValue = Math.min(calValue + 2, 900);
    send('calibrate', { on: true, barBottom: calValue });
  });
  globalShortcut.register('Down', () => {
    calValue = Math.max(calValue - 2, 0);
    send('calibrate', { on: true, barBottom: calValue });
  });
  globalShortcut.register('Return', () => {
    cfg.barBottom = calValue;
    saveConfig();
    stopCalibrate(true);
  });
  globalShortcut.register('Escape', () => stopCalibrate(false));
}

function unregisterCalibrateKeys() {
  for (const k of ['Up', 'Down', 'Return', 'Escape']) globalShortcut.unregister(k);
}

function startCalibrate() {
  calibrating = true;
  calOriginal = cfg.barBottom;
  calValue = cfg.barBottom;
  win.showInactive();
  if (win.isMinimized()) win.restore();
  send('calibrate', { on: true, barBottom: calValue });
  registerCalibrateKeys();
}

function stopCalibrate(saved) {
  calibrating = false;
  unregisterCalibrateKeys();
  if (!saved) {
    cfg.barBottom = calOriginal;
    send('calibrate', { on: false, barBottom: calOriginal });
  } else {
    send('calibrate', { on: false, barBottom: cfg.barBottom });
  }
  if (!armed) send('armed', false);
}

ipcMain.handle('config', () => cfg);
ipcMain.handle('state', () => ({ armed, calibrating }));

app.whenReady().then(() => {
  const firstRun = !fs.existsSync(CONFIG_PATH);
  loadConfig();
  createWindow();

  globalShortcut.register(cfg.toggleHotkey, toggle);
  globalShortcut.register(cfg.calibrateHotkey, () => {
    calibrating ? stopCalibrate(false) : startCalibrate();
  });

  // 第一次啟動直接把校準叫出來，不必先讀文件
  if (firstRun) {
    win.webContents.once('did-finish-load', () => setTimeout(startCalibrate, 300));
  } else {
    // 平常閒置時整層隱形，但視窗本身不關
    win.webContents.once('did-finish-load', () => {
      win.showInactive();
      send('armed', false);
    });
  }
});

app.on('window-all-closed', () => app.quit());
app.on('will-quit', () => globalShortcut.unregisterAll());
