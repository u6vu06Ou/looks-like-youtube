# AGENTS

給 AI agent 的安裝與操作說明。人類版看 `README.md`。

## 這是什麼

純 Win32 + GDI+ 的 Rust 程式。開一個全透明、永遠置頂、滑鼠點穿的視窗，在畫面頂端畫一條假 YouTube 影片標題、底端畫一條假直播控制列，蓋在遊戲畫面上。中間留空，讓真實遊戲畫面透出——所以遊戲不掉幀、不掉畫質。

## 建置步驟

1. 確認 `cargo --version` 可用（沒有就 `rustup` 裝 stable）與 MSVC 建置工具存在。
2. 在此目錄執行 `cargo build --release`。
3. 執行 `target\release\looks-like-youtube.exe`。程式不會開任何可見視窗、不進工作列。
4. 若這是第一次啟動，畫面中央會出現「↑／↓ 調整控制列位置」。請使用者按方向鍵對齊遊戲快捷鍵上方、按 `Enter` 存檔。
5. 提醒使用者改標題：編輯設定檔的 `title`，換成與遊戲內容相符的 YouTube 影片標題，改完重啟程式。

## 操作

| 鍵 | 作用 |
|---|---|
| `Ctrl+Alt+Shift+Y` | 開／關 |
| `Ctrl+Alt+Shift+K` | 校準（`↑`／`↓` 移動、`Enter` 存、`Esc` 取消；校準中按開關鍵＝直接存檔退出） |

結束程式：工作管理員結束 `looks-like-youtube.exe`，或 `taskkill /IM looks-like-youtube.exe /F`。

## 設定

設定檔在 `%APPDATA%\looks-like-youtube\config.json`，欄位：`title`（標題文字）、`label`（左下角標籤）、`showTitle`（顯示標題與否）、`barBottom`（離底邊像素，會自動讓開工作列）、`toggleHotkey` 與 `calibrateHotkey`（改鍵，如 `Ctrl+Shift+F9`）。改完要重啟。

## 驗收方式

1. 啟動後按 `Ctrl+Alt+Shift+Y`，頂部應出現白色標題、底部出現紅色進度條。
2. 滑鼠移到覆蓋層上點擊，遊戲必須**照樣收到點擊**、焦點不得跑掉。
3. 連按 30 次開關，不可出現黑底或殘影，關閉後畫面要完全乾淨。
4. 遊戲 FPS 不變（覆蓋層自身 GPU 應為 0.0%）。
5. 換解析度（含 4:3）後控制列仍貼齊底部、圖示不重疊。

## 不要做

- 不要拿掉 `WS_EX_TRANSPARENT`、`WS_EX_NOACTIVATE`、`WS_EX_LAYERED`。任一項改掉就會搶焦點、吃掉遊戲滑鼠或整層變黑。
- 不要用 `ShowWindow(SW_HIDE)` 當開關。視窗必須常駐，開關只換一張（透明的）點陣圖；反覆顯隱會讓 Windows 的透明視窗重建合成層而變黑。
- 關閉狀態送出的點陣圖**一定要先清空**。那個 DIB 是重複使用的，不清空就會把上一次的內容原樣送出去（症狀：關不掉）。注意 `bits` 是 `*mut u32`，清空筆數是像素數不是位元組數。
- 不要用 `GdipDrawString` 傳 `-1` 當長度；緩衝區沒有 NUL，會亂數截字。
- 不要期待 `GdipAddPathArc` 直接拼圓角；弧線會自動從目前點連線，接錯就出黑 X。圓角用「兩矩形＋四角圓」。
- 不要對這個視窗呼叫 `ShowWindow(SW_MAXIMIZE)`；尺寸一律用 `SetWindowPos`。

## 已知地雷

- GDI+ 一定要用 `GdipCreateBitmapFromScan0` 包住 DIB 記憶體來取 Graphics；走 HDC 不會寫 alpha，整片會變黑。
- `GpUnit` 的 `UnitPixel` 是 **2**，不是 3（3 是 UnitPoint，字會變大）。
- 同一條 path 裡的多段線會被連起來。分開的線（字幕底線、四角括號）要各自 `GdipCreatePath`。
- 只對視窗化／無邊框遊戲有效，獨佔全螢幕無效。
- 前景程式是管理員權限時滑鼠穿透會失效（UIPI），本程式需同樣以管理員身分啟動。
- 只在 Windows 11 實測過。
