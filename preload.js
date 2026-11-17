'use strict';

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('fake', {
  config: () => ipcRenderer.invoke('config'),
  state: () => ipcRenderer.invoke('state'),
  onArmed: (cb) => ipcRenderer.on('armed', (_e, v) => cb(v)),
  onCalibrate: (cb) => ipcRenderer.on('calibrate', (_e, v) => cb(v)),
});
