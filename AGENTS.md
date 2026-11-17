# AGENTS

給 AI agent 的安裝與操作說明。人類版看 `README.md`。

## 這是什麼

Electron 桌面程式。開一個全透明、永遠置頂、滑鼠點穿的視窗，在畫面頂端畫一條假 YouTube 影片標題、底端畫一條假直播控制列，蓋在遊戲畫面上。中間留空，讓真實遊戲畫面透出——所以遊戲不掉幀、不掉畫質。

## 安裝步驟

1. 確認 `node --version` 為 18 以上。沒有就裝 Node.js LTS。
2. 在此目錄執行 `npm ci`。（**不要**用 `npm install`，會改動 lock file。）
3. `npm start` 啟動。程式不會開任何可見視窗。
4. 若這是第一次啟動，畫面中央會出現「向上／向下調整控制列位置」。請使用者按方向鍵對齊遊戲快捷鍵上方、按 `Enter` 存檔。
5. 提醒使用者改標題：編輯設定檔的 `title`，換成與遊戲內容相符的 YouTube 影片標題，改完重啟程式。

## 操作

| 鍵 | 作用 |
|---|---|
| `Ctrl+Alt+Shift+Y` | 開／關 |
| `Ctrl+Alt+Shift+K` | 校準（方向鍵移動、`Enter` 存、`Esc` 取消） |
| `Ctrl+C` | 結束程式 |

## 設定

設定檔在 `%APPDATA%\looks-like-youtube\config.json`，欄位：`title`（標題文字）、`label`（左下角標籤）、`showTitle`（顯示標題與否）、`barBottom`（離底邊像素）、`toggleHotkey` 與 `calibrateHotkey`（改鍵）。改完要重啟。

## 驗收方式

1. 啟動後按 `Ctrl+Alt+Shift+Y`，頂部應出現白色標題、底部出現紅色進度條。
2. 滑鼠移到覆蓋層上點擊，遊戲必須**照樣收到點擊**、焦點不得跑掉。
3. 連按 10 次開關，不可出現黑底或殘影。
4. 遊戲 FPS 不變。

## 不要做

- 不要改 `main.js` 的 `focusable:false`、`setIgnoreMouseEvents`、`showInactive`。任一項改掉就會搶焦點或吃掉遊戲的滑鼠。
- 不要用 `win.show()` 取代 `showInactive()`。
- 不要用 `win.hide()` 當開關。視窗必須常駐，開關只切換 `body.armed` class；反覆 hide/show 會讓 Windows 的透明視窗重建合成層而變黑。
- 不要對這個視窗呼叫 `maximize()` 或 `setFullScreen()`，透明視窗會出黑邊。尺寸一律用 `setBounds`。

## 已知地雷

- 只對視窗化／無邊框遊戲有效，獨佔全螢幕無效。
- 前景程式是管理員權限時滑鼠穿透會失效（UIPI），本程式需同樣以管理員身分啟動。
- 只在 Windows 11 與 Electron 33 實測過。
