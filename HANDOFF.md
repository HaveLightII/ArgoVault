# ArgoVault 交接文件

最後更新：2026-11-14。原名 PyPasswords（2026-11-11 改名）。本文件不內嵌程式碼，程式以快照檔（第 12 節）一併提供。  

## 1. 專案與目前狀態

本機多設定檔密碼庫，是原 Python 命令列程式 `PyPasswords.py` 的 HTML 重寫。前端為單一檔案 `src/index.html`（原生 JavaScript，無外部相依），以 Tauri 2 打包成 Android App；也可直接在瀏覽器開啟（資料存 `localStorage`，Tauri 內存 AppData 檔案，以 `window.__TAURI__` 自動切換）。  

- 倉庫：`git@github.com:HaveLightII/ArgoVault.git`（分支 `main`），本機 `~/gits/ArgoVault`。  
- `identifier` 為 `com.havelight.pypasswords`，不可更動（改了等於另一個 App，資料不通）。crate 與 lib 名稱仍是 `pypasswords`、`libpypasswords_lib.so`（建置輸出已確認），刻意不改。  
- 手機：三星（2026-11-14 實測裝置為 SM-T500），arm64；安裝的是以 debug 金鑰簽署的 release APK。  

| 項目 | 狀態 |
| --- | --- |
| 基本功能、設定、備份、多選刪除、PIN、返回鍵、圖示 | 實機 OK |
| 查看密碼前是否再輸入主碼（`reauth`） | 實機 OK |
| 生物辨識解鎖 | 實機 OK |
| 改名 ArgoVault | 倉庫與 `strings.xml`、`tauri.conf.json` 已改並建置安裝；使用者未回報桌面名稱 |
| 瀏覽器自動填入 | 實機 OK：已啟用、建議（自訂版面）清楚可見、解鎖正常、僅列符合紀錄、一次點擊即填入 |
| 編輯前主碼對話框生物辨識、編輯頁複製鈕、編輯中離開不鎖庫 | 實機 OK（2026-11-13） |
| macOS 生物辨識解鎖（Touch ID，見 6.6） | 實機 OK（2026-11-14） |
| macOS 自動填入 | 2026-11-14 決定放棄（無 Xcode 與簽署憑證） |
| Windows 版 | 建置步驟見 7.1，含 Windows Hello 生物辨識解鎖（6.7）；尚未在 Windows 實機建置與測試 |
| Windows 自動填入 | 2026-11-14 決定暫緩，待生物辨識實機通過後再議（候選：全域快速鍵自動輸入、瀏覽器擴充功能） |

## 2. 使用者偏好（需遵守）

- 一律繁體中文（台灣用語），禁止表情符號；避免大陸用語，無台灣譯名時保留英文原詞。  
- 程式碼簡潔優先，函數與變數用 camelCase；數學式用 LaTeX。  
- 指令列區塊內不寫註解；需解釋時於區塊後以「行號：說明」條列。  
- Markdown：標題僅一個 `#`，次標題 `##`、`###`；段落行尾加兩個空格；標題、列表、表格前後留空白行；表格附 `| --- |`；內文中的 `\n`、`\t`、反斜線以反引號包裹。  
- 事實性主張優先引用可信來源（APA 第七版）；不確定時主動標示；使用者前提有誤時直接指出；以使用者最新指示為準。  
- 條件式需求（例如「有效能優勢才做」）必須以實測數據判斷並回報。  
- 使用者上傳檔案並要求修改時，交付完整修改後的檔案，不要求使用者再跑修補腳本；交付用 `present_files`，不用 `Artifact` 發布。  
- 使用者常貼終端機輸出，回答前逐行核對並指出實際問題；不要建議把輸出重導向到 `$(tty)`（曾因此誤建 `a`、`not`、`tty` 三個檔案），`git` 長輸出用 `--no-pager`。  
- 簽署 APK 用 `java -jar ~/bin/uber-apk-signer-1.3.0.jar -a <未簽署 APK>`（內嵌 debug 金鑰）。  
- 每完成一個步驟就更新本文件，並盡量精簡（不影響交接的前提下）；同時列出交接附檔與快照指令（第 12 節）。  
- 建置主機有兩台：Ubuntu 26.04（bash，Android 建置）與 macOS 27（zsh，只有 Command Line Tools，無 Xcode、無簽署憑證；macOS 版建置，也可建置 Android：`ANDROID_HOME=~/Android/Sdk`、NDK `28.2.13676358`、JDK 17，簽署工具檔名為 `~/bin/uber-apk-signer.jar`，同為 1.3.0 版與同一把內嵌 debug 金鑰，可覆蓋安裝），指令以當前主機為準。專案實際路徑 `~/gits/ArgoVault`（使用者曾把舊名打成 `~/PyPassword`）。GitHub 帳號 `HaveLightII`。  

## 3. 資料格式

設定檔名稱規則 `[\w\-\u4e00-\u9fff]+`。三種格式皆由檔頭自我描述；介面一律稱 PWV1 至 PWV3，程式內部識別值為 `"FERNET"`、`"PWV2"`、`"PWV3"`，不可更動。  

| 名稱 | 檔案位元組配置 | 金鑰衍生與加密 |
| --- | --- | --- |
| PWV1（Fernet 格式） | `SALT(16)` + Fernet token（ASCII urlsafe base64，開頭 `gAAAA`），無 MAGIC | PBKDF2-HMAC-SHA256，600000 次，32 位元組 |
| PWV2 | `"PWV2"` + `SALT(16)` + `NONCE(12)` + 密文含 tag | PBKDF2-HMAC-SHA256，600000 次，AES-256-GCM |
| PWV3（預設） | `"PWV3"` + `0x01` + `SALT(16)` + `NONCE(12)` + 密文含 tag | Argon2id（19456 KiB、2 次、1 lane、32 位元組、0x13），AES-256-GCM |

- 明文為 JSON `{"records":[...]}`；紀錄欄位 `id`（`token_hex(4)`）、`accountType`、`accountName`、`password`、`linkedAccount`、`name`、`notes`（字串陣列），另有選填 `url`（網站網址，自動填入比對用；舊資料沒有）。  
- 空資料庫最小長度：PWV2、PWV3 為 28 位元組（曾誤設 44，導致空庫無法解鎖）。  
- 備份：複製 `.bin` 並另存 `<檔名>.sha256`。CSV 匯入欄位 `帳戶類型,帳號名稱,密碼,連結帳戶,姓名,備註`，另可選 `網址`；含「備註」的欄名皆為備註，單欄以 `;` 或 `；` 分隔。  
- 密碼生成：長度 12 至 64；英數字加 `!@#$%^&*()-_=+[]{}`；須同時含小寫、大寫、數字、符號。  
- 儲存位置：Tauri 內 `<AppData>/vault/<設定檔>.bin`（Android 上 AppData 實測就是 App 資料目錄根，`run-as` 列出 `./vault/...`），瀏覽器內 `localStorage` 鍵 `pwv:<profile>`（值為 base64）。設定在 `<AppData>/settings.json`（瀏覽器 `pwv-settings`）。  

## 4. 功能、設定與安全設計

### 4.1 功能

- 設定檔：建立（選 PWV3、PWV2、PWV1，主碼二次確認，離線弱密碼檢查可關）、解鎖、匯入、刪除。紀錄：新增、編輯、單筆與多選刪除、依帳號名稱搜尋、備註（textarea，每行一筆）。  
- 刪除（單筆、多選、資料庫）依序要求確認、PIN（已設定時）、主碼；刪資料庫另須輸入完整名稱。  
- 「更多」選單：匯出備份（`.bin` 與 `.sha256`，Tauri 走系統儲存對話框並讀回驗證）、匯入備份（隱藏檔案選擇器，彈窗顯示檔名、大小、檔頭、偵測格式與 SHA-256）、匯入 CSV、轉換加密格式（新 salt，先解密比對，舊檔留 `.bin.bak`）、變更主碼、多選刪除、生物辨識解鎖啟用或停用。  
- 「生成密碼」對話框、強度條（可關）、中英文介面（跟隨系統或手動，立即切換）。  
- 所有 `confirm`、`prompt` 皆為自製 `<dialog id="dlgAsk">`；全頁 `user-select:none`（輸入框與 SHA-256 欄位例外）；`isEnter` 處理 Enter（忽略輸入法組字）。  

### 4.2 設定項目（`cfg`，讀取時逐項驗證，不合法退回預設）

| 設定 | 預設 | 說明 |
| --- | --- | --- |
| 語言 | 跟隨系統 | 繁體中文、English |
| 預設加密格式 | PWV3 | 建立表單預設；方向不限，選 PWV1 或 PWV2 會降級 |
| 解鎖時自動轉換為預設格式（`auto`） | 關 | 轉換失敗只顯示錯誤；自動填入模式不轉換 |
| 啟用密碼強度檢查（`weak`） | 開 | 控制弱主碼警告與強度條 |
| 生成密碼預設長度（`len`） | 20 | 12 至 64 |
| 禁止截圖與螢幕錄影（`secure`） | 關 | 僅 Android，見 6.2 |
| 閒置自動鎖定（`idle`） | 5 分 | 1、3、5、10、30 |
| 切到背景後鎖定（`bg`） | 1 分 | 30 秒、1、5、15 分；刻意不提供「立即」，因系統檔案對話框會讓 App 進背景 |
| 查看密碼顯示時間（`rev`） | 30 秒 | 15、30、60 |
| 查看密碼前需再輸入主碼（`reauth`） | 開 | 關閉時解鎖後保留不可匯出的 `CryptoKey`（`S.ses`），查看密碼不再詢問；新增、編輯、刪除、匯入 CSV、變更主碼、轉換格式一律仍要求；鎖定、轉換、變更主碼、重新開啟此設定、還原預設都會清除 |
| 自動填入的帳號欄位（`afUser`） | `accountName` | 或 `linkedAccount`；僅 Android 設定頁顯示 |
| 複製密碼後提醒清除剪貼簿（`clip`） | 開 | App 不自動清除（使用者決定） |
| 刪除用 PIN（`pin`） | 未設定 | 4 至 8 位數字，PBKDF2-SHA256 100000 次雜湊；只防誤刪，不保護資料 |

「還原預設值」保留語言，有 PIN 時須先輸入。刻意不做主題手動切換：狀態列底色來自 Android 資源 `values`、`values-night`，跟隨系統。  

### 4.3 安全設計（使用者明確要求）

- 解鎖後記憶體只保留清單中繼資料（`id`、`accountType`、`accountName`、`name`）；例外是 `reauth` 關閉時的 `S.ses`。  
- 主碼輸入後立即轉 `Uint8Array` 並清空輸入框，用畢 `fill(0)`；Argon2id 在 Blob Worker 執行，緩衝運算後清零，Worker 用完即終止。JavaScript 無法保證覆寫字串記憶體。  
- 查看密碼預設遮罩，自動隱藏並倒數；編輯期間只保留不可匯出的 `CryptoKey`，儲存時重新解密最新資料後合併，編輯表單閒置 120 秒取消。  
- `pagehide` 立即鎖定；進背景立即清除已顯示密碼與編輯表單，超過設定時間整體鎖定。  
- 寫入：先寫 `.bin.tmp`，讀回比對，現有檔通過格式檢查才複製為 `.bin.bak`，最後改名取代；啟動時清掉殘留 `.tmp`。  

## 5. `index.html` 結構與不可破壞的約定

由上而下：`<style>`（`:root` 設計 token、深色模式、480px 與 820px 斷點）、HTML（`#gate`、`#afp`、`#vault`、各 `dialog`、`#busy`、`#toast`）、`/*A2*/…/*A2END*/`（BLAKE2b 與 Argon2id）、`argon2Async`、`/*CORE*/…/*COREEND*/`（`hdr`、`derive`、`unseal`、`seal`、`openVault`）、設定與多語系、儲存層、事件與介面、啟動區塊。  

- `/*A2*/` 與 `/*A2END*/` 各只能出現一次（程式內以字串拼接避免自我比對），且 HTML 不可被壓縮或改寫：Worker 以 `document.currentScript.textContent` 切出 A2 區段。  
- 新增任何使用者看得到的字串，必須同時加進 `EN` 字典（以中文原句為鍵）；`toast()`、`work()`、`Error.message` 都會經 `t()`；標記 `data-nt` 的元素由 JS 填入，不經 `trDom`，產生時就呼叫 `t()`。  
- `storeGet` 與 `rawVault` 是同步函式，依賴啟動時載入的 `vaultCache`，不可改成非同步。  
- `askDlg` 與 `ask` 的 `onclose` 必須寫成 `if(!d.open)`，否則連續兩次使用同一對話框時，前一次延後送出的 `close` 事件會被當成第二次的取消。  
- `authOpen(reason, soft)`：`soft` 為真（只有查看密碼）、`reauth` 為假且 `S.ses` 與檔頭 `fmt`、`salt` 相符時，直接 `unseal`，否則退回詢問主碼；成功後若 `reauth` 為假就快取金鑰。  
- `ask(reason, bio)`：`bio` 為真且該設定檔已註冊生物辨識時，`#dlgAuth` 顯示「生物辨識驗證」鈕，成功即以主碼位元組走原流程；只有 `authOpen` 傳 `true`（查看、新增、編輯、刪除、匯入 CSV 皆可用，啟用生物辨識與變更主碼不用）。失敗訊息顯示在對話框內 `#aErr`（對話框在 top layer，`toast` 會被蓋住）；驗證後主碼無法開庫則停用註冊。
- 解鎖流程為 `doUnlock(name, pw, fromBio)`（主碼與生物辨識共用）；生物辨識解鎖後無法開庫（主碼已變）會自動停用該設定檔的註冊。  
- 橋接物件（不存在時相關按鈕隱藏）：`window.PwBio`（生物辨識與自動填入設定；Android 為原生橋接，macOS 與 Windows Tauri 為前端 shim，見 6.6、6.7）、`window.PwAF`（僅 Android）（自動填入模式，`body.af`，只讀，解鎖後 `afList` 顯示比對清單，不建立 `S`）。  
- 過去的陷阱：`lock()` 內不可引用不存在的元素 ID；鍵盤 `onkeydown` 對非 Enter 鍵不可 `return false`（會擋住倒退刪字）；`unseal` 最小長度 28。  

## 6. Android 整合

### 6.1 檔案與職責

| 檔案（相對於 `src-tauri/gen/android/app/`） | 職責 |
| --- | --- |
| `src/main/java/com/havelight/pypasswords/MainActivity.kt` | 狀態列內縮、`FLAG_SECURE`（監看 `secure.flag`）、返回鍵攔截、`onWebViewCreate` 掛上 `PwBio` |
| 同目錄 `PwBio.kt` | 生物辨識橋接（Keystore 加 `BiometricPrompt`）、`afEnabled()`、`afSettings()` |
| 同目錄 `ArgoAutofillService.kt` | `AutofillService`：解析欄位與 `webDomain`，回傳需驗證的 `Dataset` |
| 同目錄 `AutofillAuthActivity.kt` | 驗證畫面：純 WebView 載入 `assets/argovault-af.html`；內含 `PwAF` 橋接 |
| `src/main/res/xml/autofill_service.xml` | 自動填入服務設定 |
| `src/main/res/layout/autofill_item.xml` | 自動填入建議的 `RemoteViews` 版面（固定深藍底 `#234E70`、白字、App 圖示，不隨系統主題） |
| `src/main/AndroidManifest.xml` | `USE_BIOMETRIC` 權限、服務與驗證畫面宣告 |
| `build.gradle.kts` | `androidx.biometric:1.1.0`、`androidx.webkit:1.14.0`、`minSdk = 24`、任務 `syncAutofillAsset`（把 `src/index.html` 複製成 `assets/argovault-af.html`，掛在 `preBuild`） |
| `proguard-rules.pro` | 保留 `PwBio`、`PwAF` 的 `@JavascriptInterface` 方法 |

### 6.2 禁止截圖

JS 把 `1` 或 `0` 寫入 `<AppData>/secure.flag`；`MainActivity` 在 `onCreate` 讀取並對視窗加減 `FLAG_SECURE`，以 `FileObserver` 監看（同時監看 `filesDir` 作保險），切換即時生效。依賴「`appDataDir` 就是 `dataDir`」（`run-as ls` 已支持）。最近工作畫面縮圖會變黑，屬預期。  

### 6.3 返回鍵

JS `window.__pwBack()` 回傳 `true` 表示已處理；`MainActivity` 以 `OnBackPressedCallback` 攔截並 `evaluateJavascript`，結果不是 `"true"` 就 `finish()`。WebView 由走訪 `android.R.id.content` 取得。  

- 順序：先關最上層對話框（檢查順序 `#dlgAsk`、`#dlgAuth`、`#dlgConv`、`#dlgPw`、`#dlgImp`、`#dlg`、`#dlgSet`，以其取消鈕 `click()`）；運算中不處理；編輯表單取消；多選結束；詳情回清單；搜尋有字則清空；清單最上層 `lock()` 並留在解鎖頁（2026-11-14 起不再退出，實機 OK）；建立分頁（已有設定檔）回解鎖分頁；解鎖頁退出。  

### 6.4 生物辨識解鎖（已實機驗證）

- 不用 Tauri 外掛。官方 `tauri-plugin-biometric` 只回傳成功或失敗、不保管金鑰，無法解開 vault（Tauri, n.d.-a）；改為自寫橋接。  
- 啟用：先以主碼驗證，再把主碼位元組交給 `PwBio.enroll`；原生端建立每設定檔一把 Keystore AES-256-GCM 金鑰（`setUserAuthenticationRequired(true)`、`setInvalidatedByBiometricEnrollment(true)`，Android 11 以上 `setUserAuthenticationParameters(0, AUTH_BIOMETRIC_STRONG)`），經 `BiometricPrompt` 加 `CryptoObject` 通過後加密主碼，密文與 IV 存私有 `SharedPreferences`（`pw_bio`），別名為設定檔名 SHA-256 前 32 個十六進位字元。  
- 包裝主碼而非衍生金鑰：轉換格式與重新加鹽不會使其失效。主碼變更後前端以新主碼重新註冊（取消則停用）；新增指紋、密文遺失、解密後無法開庫都會清除註冊並提示。建立同名、匯入覆蓋、刪除設定檔也清除。  
- 回傳協定：`window.__pwBioDone(id, code, data)`；`code` 為 `ok`、`cancel`、`lockout`、`missing`、`invalidated`、`error`。只接受 `BIOMETRIC_STRONG`（許多裝置的臉部辨識只屬 Class 2，不符）；主碼的 base64 字串會短暫存在於 JS 堆積。威脅模型：便利功能，強度低於主碼（Android Developers, n.d.-a）。  

### 6.5 瀏覽器自動填入（實機 OK）

流程：Chrome 登入頁點欄位 → 系統顯示「ArgoVault」建議 → `AutofillAuthActivity` 解鎖（主碼或指紋）→ 顯示網址符合的紀錄 → 點選後回傳 `Dataset`，欄位被填入。  

- 鎖定時 `ArgoAutofillService` 不洩漏任何帳號，只回傳一個 `setAuthentication` 的 `Dataset`（欄位值為 `null`）；請求來自本 App 自己的畫面時略過；找不到密碼欄位時不回應；帳號欄位取密碼欄位前最近的文字欄位（優先看 `autofillHints` 與 HTML `type=email`）。  
- `AutofillAuthActivity`：`FLAG_SECURE`、禁止導覽、`WebViewAssetLoader` 載入 `https://appassets.androidplatform.net/assets/argovault-af.html`，同時掛 `PwAF` 與 `PwBio`；`PwAF` 提供 `request()`、`profiles()`、`read(name, ext)`（只讀 `<dataDir>/vault/<name>.bin` 或 `.bin.bak`，拒絕含路徑字元的名稱）、`settings()`、`fill()`、`cancel()`。  
- 比對（`afMatch`）：請求網域去掉 `www.` 後等於紀錄網址的主機，或為其子網域；`evil-example.com` 不符合 `example.com`。無網域（非瀏覽器）時，只在紀錄網址等於套件名稱時符合。沒有符合項目時須按「顯示所有紀錄」手動選，並顯示警告。不使用公共尾碼清單，網址不要填 `co.uk` 這類。  
- 建議按鈕不可用 `android.R.layout.simple_list_item_1`：文字色來自深色主題而下拉底色為白，呈現空白方塊（實機截圖證實）；現用自訂版面 `autofill_item`，副標依系統語言（`zh` 或英文）。
- 切到背景或閒置 90 秒即取消；每次填入都要解鎖；不寫入 vault。  
- 使用者端設定：系統設定選 ArgoVault 為自動填入服務（設定頁有捷徑）；Chrome 設定的「自動填入服務」選「使用其他服務自動填入」（Android Developers Blog, 2024）。  
- 已知限制：不支援先填帳號、下一頁才填密碼的多步驟登入；只針對 Chrome；網域取決於瀏覽器是否提供；`afUser` 是暫時做法（紀錄沒有專屬登入帳號欄位），不合用時可改為新增獨立欄位。  
- 已驗證（2026-11-13）：資料集層級驗證一次點擊即填入；`profiles()` 找得到檔案；`assets/` 額外檔案未被清除。未驗證：Argon2id 的 Blob Worker 可在該 WebView 運作。  

### 6.6 macOS 生物辨識解鎖（Touch ID，實機 OK）

- 設計等同 6.4：包裝主碼而非衍生金鑰。`src-tauri/macos/bio.swift` 以 CryptoKit 建立每設定檔一把 Secure Enclave P-256 金鑰（存取控制 `privateKeyUsage` 加 `biometryCurrentSet`），與一次性軟體金鑰做 ECDH，經 HKDF-SHA256 得 AES-256-GCM 金鑰加密主碼（Apple, n.d.）。金鑰資料、一次性公鑰、密文與註冊當下的生物辨識狀態雜湊存 `<AppData>/bio/<設定檔>.bio`（JSON，權限 600）；金鑰資料只有本機 Secure Enclave 能用。  
- 流程：先 `LAContext.evaluatePolicy`（僅生物辨識，不提供密碼後備），同一個 context 交給金鑰使用，預期只跳一次 Touch ID。啟用時會立即解密比對一次。狀態雜湊與註冊時不同即回 `invalidated`。  
- 串接：`build.rs` 在目標為 macOS 時以 `swiftc` 編成靜態庫 `libargobio.a` 並連結（不需 Xcode 與 `swift-rs`）；`lib.rs` 的 `bio` 模組（平台實作在子模組 `imp`，macOS 與 Windows 各一份）提供 command `bio_init`、`bio_remove`、`bio_enroll`、`bio_unlock`（皆 `#[tauri::command(async)]`，不可在主執行緒呼叫，否則 Touch ID 對話框會卡死）。  
- 前端：`window.PwBio` 不存在、為 Tauri 且 User-Agent 含 `Macintosh` 或 `Windows NT` 時注入同協定 shim（`desktop:true`）；`status()`、`has()` 是同步呼叫，由啟動時的 `bioInit()` 載入快取 `macBio`。Touch ID 對話框文字為「ArgoVault 正在嘗試」加上 shim 傳入的理由字串（已加進 `EN`）。  
- 已實測（2026-11-14）：ad-hoc 簽署的程式可建立並還原 Secure Enclave 金鑰；release `.app` 建置成功並可啟動；使用者回報實測通過（檢查項目：啟用時 Touch ID 只跳一次、Touch ID 解鎖、取消後仍可用主碼；原始碼未變動下重建 App 後舊註冊仍可解鎖）。未實測：改過程式碼再重建後舊註冊是否仍可用、新增指紋後失效。  
- 已知限制：無 Touch ID 的 Mac 狀態為 `nohw`，按鈕仍顯示但會提示無法使用；`.bio` 檔不隨備份匯出，換機須重新啟用。  

## 7. 建置與安裝（Ubuntu）

環境：`ANDROID_HOME=$HOME/Android/Sdk`，NDK `27.2.12479018`，JDK 17，Rust 目標 `aarch64-linux-android`，簽署工具 `~/bin/uber-apk-signer-1.3.0.jar`（內含 zipalign 與 debug 金鑰）。Tauri 外掛：`fs`、`dialog`、`clipboard-manager`，權限在 `capabilities/default.json`，`app.withGlobalTauri` 為 `true`。  

```
cd ~/gits/ArgoVault
npm run tauri android build -- --apk --target aarch64
java -jar ~/bin/uber-apk-signer-1.3.0.jar -a src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk
adb install --no-incremental -r src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-aligned-debugSigned.apk
```

1：進入專案。  
2：建置 release APK；改了 `src/index.html` 必須重建才會更新（也會經 `syncAutofillAsset` 複製給自動填入畫面）。  
3：以 debug 金鑰對齊並簽署。  
4：覆蓋安裝；`--no-incremental` 避免這支手機不支援增量安裝的錯誤訊息。  

- 簽章與先前不同時會出現 `INSTALL_FAILED_UPDATE_INCOMPATIBLE`，須先匯出備份再 `adb uninstall com.havelight.pypasswords`（等於清空資料）。release 版不能用 `run-as`。  
- 圖示：`app-icon.png`（1024×1024、透明背景、主體在中央約 66%）執行 `npm run tauri icon app-icon.png -- --ios-color "#234e70"`；圖示不更新是啟動器快取。  
- 自有金鑰簽署尚未執行：流程為 `keytool` 產生 `.jks`、`zipalign`、`apksigner sign`；金鑰遺失即無法原地更新，執行前先匯出 vault 並備份金鑰與密碼。  
- macOS：`npm run tauri build -- --bundles app` 產生 `src-tauri/target/release/bundle/macos/ArgoVault.app`（ad-hoc 簽署，已驗證可建置與啟動；最低系統版本 11.0）；`.dmg`（`--bundles app,dmg`）可建置；視窗寬度低於 820 會顯示手機版面；禁止截圖與返回鍵只在 Android 有效。  
- 編譯警告（`setValue` 棄用、AGP 的 `android.newDsl` 等）皆可忽略。  

### 6.7 Windows 生物辨識解鎖（Windows Hello，未實測）

- 同樣包裝主碼，command 與前端 shim 與 6.6 共用；實作在 `lib.rs` 的 `bio::imp`（`cfg(target_os = "windows")`），相依 `windows`、`aes-gcm`、`sha2`（僅 Windows 目標）。  
- 做法：每設定檔一把 `KeyCredential`（名稱 `ArgoVault-<設定檔>`，有 TPM 時由硬體保護），對隨機 32 位元組挑戰值 `RequestSignAsync`（觸發 Windows Hello），以 SHA-256(`"ArgoVault-bio-v1"` 加簽章) 為 AES-256-GCM 金鑰加密主碼；挑戰值、nonce、密文存 `<AppData>\bio\<設定檔>.bio`（Microsoft, n.d.）。前提是簽章為確定性（RSA PKCS#1 v1.5），未在實機確認。  
- 回傳碼：使用者取消或選擇密碼為 `cancel`，裝置鎖定為 `lockout`，金鑰不存在或解密失敗為 `invalidated`。`status` 依 `IsSupportedAsync`，未設定 Windows Hello 時為 `unenrolled`。  
- 啟用時預期跳兩次 Windows Hello（建立金鑰一次、簽章一次）；對話框無法自訂說明文字。`focusPrompt` 會輪詢 `Credential Dialog Xaml Host` 視窗並帶到前景，避免對話框開在 App 後方。  
- 驗證狀態：Windows 專屬程式碼已在 macOS 上以 `cargo check --target x86_64-pc-windows-msvc` 通過型別檢查（獨立小專案，因主專案的資源編譯需要 Windows 工具）；完整建置、連結與執行皆未驗證。  
- 實機待測：啟用（跳幾次、是否在前景）、解鎖、取消、未設定 Windows Hello 時的提示、變更主碼後更新、停用後金鑰是否刪除。  

### 7.1 Windows 建置（未驗證）

前置（Tauri, n.d.-b）：Microsoft C++ Build Tools（勾選「使用 C++ 的桌面開發」）、WebView2 Runtime（Windows 10 1803 以上與 Windows 11 已內建）、Rust（`stable-msvc` 工具鏈）、Node.js LTS、Git。於 PowerShell 執行：

```
git clone git@github.com:HaveLightII/ArgoVault.git
cd ArgoVault
npm install
npm run tauri build
```

1：取得原始碼；已有倉庫時改為 `git pull`。  
2：進入專案。  
3：安裝 Tauri CLI。  
4：建置；首次會自動下載 WiX 與 NSIS。產物在 `src-tauri\target\release\bundle\msi\`（`.msi`）與 `bundle\nsis\`（`-setup.exe`），免安裝執行檔為 `src-tauri\target\release\pypasswords.exe`。  

- `build.rs` 的 Swift 編譯只在目標為 macOS 時執行；Windows 專屬程式碼只有 6.7 的生物辨識。  
- 資料位置預期為 `%APPDATA%\com.havelight.pypasswords\`（`vault\`、`settings.json`）。  
- 生物辨識為 Windows Hello（6.7）；無自動填入；禁止截圖與返回鍵只在 Android 有效。  
- 安裝檔未簽署，SmartScreen 會警告。  
- `.gitattributes` 固定 `src/index.html` 為 LF，避免 Git for Windows 轉成 CRLF。  
- 建置後待測：建立與解鎖、匯出入備份（系統檔案對話框）、變更主碼與轉換格式（涉及暫存檔改名覆蓋）、剪貼簿、閒置與背景鎖定、Windows Hello（6.7）。  

## 8. 驗證狀態

### 8.1 已驗證

- 互通：Python 產生的 PWV1、PWV2、PWV3 可由 JS 解密，JS 寫出的 PWV1、PWV3 可由 Python 解密；Argon2id 與 `hashlib.blake2b` 輸出一致；Node 中 Argon2id 約 1.6 秒。  
- 容器內 Playwright（Chromium，模擬 Tauri 與橋接）：先前 36 項前端檢查；本輪 `reauth` 設定、生物辨識流程（啟用、解鎖、主碼變更後更新、註冊過期清除、刪除設定檔清除、無橋接時按鈕隱藏）、自動填入模式（網域與子網域比對、假網域不符、手動顯示全部與警告、無來源、`afUser`、錯誤主碼、取消）全部通過，無頁面錯誤。  
- Worker 實測：總時間不變（約 1.7 秒），但運算期間畫面不再凍結（最大影格間隔由 1670 ms 降至 約 18 ms），故採用。  
- 實機（三星，arm64，release APK）：基本功能、設定、Enter、狀態列、轉換、匯出入、變更主碼、多選刪除、PIN、返回鍵、圖示皆 OK；`reauth` 與生物辨識 OK；自動填入版 APK 成功建置與安裝。  

### 8.2 自動填入實機結果

2026-11-13 實機：第 1 至 4 項與第 7 項（APK 內有 `argovault-af.html`）通過，建議按鈕外觀修正後正常。第 5 項（自家登入畫面不出現建議、無設定檔時提示）、第 6 項（失敗蒐集用）尚未實測。

檢查清單：

1. 設定頁狀態顯示「已啟用」；Chrome 已選「使用其他服務自動填入」。  
2. 紀錄填了「網站網址」後，登入頁點欄位出現「ArgoVault」建議。  
3. 點選後進入解鎖畫面，主碼與指紋都能解鎖，是一次點擊還是兩次。  
4. 清單只含符合紀錄；點選後欄位有值；取消、返回鍵、切背景都不留殘值。  
5. ArgoVault 自己的登入畫面不出現建議；沒有設定檔時提示並結束。  
6. 失敗時收集：`adb logcat -v time | grep -iE "autofill|ArgoAutofill|AutofillAuth|chromium"`；若解鎖頁顯示「尚無設定檔」，先確認 `adb shell run-as com.havelight.pypasswords ls vault`。  
7. 確認 APK 內有網頁資產：`unzip -l <APK> | grep argovault-af`。  

### 8.3 未驗證

- 匯出的 `.bin` 與 `.sha256` 以 `sha256sum` 比對、以原 Python 版解密；JS 寫出的 PWV2 由 Python 解密。  
- Android SAF 寫入是否截斷既有檔、`defaultPath` 是否生效；系統檔案對話框讓 App 進背景時為何不觸發 `pagehide` 鎖定（實測正常，原因未確認）。  
- 禁止截圖的最近工作畫面縮圖細節；重開偶發卡頓的真正原因（目前未再重現，復發時收集 `adb logcat -v time | grep -iE "\[PW\]|pypasswords|AndroidRuntime|FATAL|ANR|chromium|tauri"`）。  
- `allowBackup` 預設值；自有金鑰簽署；Windows 建置與執行（見 7.1）；macOS 改過程式碼再重建後舊註冊是否仍可用、新增指紋後註冊失效（見 6.6）。  

## 9. 已知限制與待辦

- 每次驗證都重跑 KDF；Argon2id 單 lane 的 JS 版不會更快，真正加速需 WebAssembly（容器無網路，未試）。  
- 瀏覽器版資料綁定瀏覽器與網址來源；Tauri 版解除安裝即清空，更換簽章前必須先匯出備份。  
- 設定檔不加密，PIN 與設定可被能讀取 App 資料的人修改，與「裝置本身需可信」一致。  
- 備註逐筆編輯尚未實作；搜尋只比對 `accountName`。
- 編輯中切到背景（2026-11-13 起）：不取消編輯、不啟動背景鎖定計時，密碼欄改回遮罩，編輯逾時延長為 5 分鐘；逾時才取消編輯並重啟閒置與背景鎖定計時。閒置鎖定在編輯中不觸發；`pagehide` 在編輯中不鎖定。編輯頁密碼旁有「複製」鈕，複製欄位目前的值。  
- 建議但未決定：`AndroidManifest.xml` 設 `android:allowBackup="false"`（避免 vault 與生物辨識密文被備份；檢查 `grep -rn allowBackup src-tauri/gen/android/app/src/main/AndroidManifest.xml`）；記住上次使用的設定檔；提醒訊息改為需確認的對話框。  
- 自動填入後續：視實測決定資料集層級或回應層級驗證、是否新增專屬登入帳號欄位、是否支援多步驟登入與其他瀏覽器。  

## 10. 開發與測試方式（容器）

容器 `bash_tool` 網路停用，不可安裝套件；有 `node`、Python `cryptography`、Playwright（Chromium）。沒有 Kotlin 編譯器與 Android 工具，原生部分只能在使用者機器驗證。  

- 語法檢查：抽出 `<script>` 內容存成 JS 檔，以 `node --check` 檢查。  
- 介面測試：Playwright 以 `add_init_script` 在載入前注入假的 `window.__TAURI__`（記憶體檔案系統）、`window.PwBio`、`window.PwAF`，再以 `file://` 開啟 `index.html`，逐項操作並檢查 `pageerror`。測試用 PWV2（PBKDF2 由瀏覽器原生執行），PWV3 每次約 1.7 秒。  
- 用 `cut -c` 截斷含中文的輸出會產生無效 UTF-8，改用 Python 截取。  
- 互通測試：取 `/*A2*/…/*A2END*/` 與 `/*CORE*/…/*COREEND*/` 合併成一個 JS，開頭加 `globalThis.crypto??=require('crypto').webcrypto;`，與 Python 產生的檔案互相讀寫。  

## 11. Git

- 倉庫 `HaveLightII/ArgoVault`，遠端為 SSH，`main` 已推送。`src-tauri/gen/android`、`src-tauri/icons`、`app-icon.png`、兩個 lock 檔納入版控；絕不納入金鑰（`*.jks`、`*.keystore`、`keystore.properties`）、`*.apk`、`*.dmg`、`node_modules`、`src-tauri/target`、vault 資料（`*.bin`、`*.sha256`、`*.bak`、`*.tmp`），規則在根目錄 `.gitignore`。  
- 2026-11-13 提交 `a7bee98` 才首次納入自動填入相關檔案（先前一直是未追蹤或未提交）。`src-tauri/gen/android/app/src/main/assets/argovault-af.html` 是 `syncAutofillAsset` 每次建置從 `src/index.html` 複製的產物，應列入 `.gitignore` 並 `git rm --cached`（已建議，待使用者執行）。
- 待清：根目錄的 `index.html.before-patch`（舊備份，已被追蹤）建議 `git rm`。  
- 提交前檢查：`git ls-files | grep -E '\.(bin|jks|keystore|apk|dmg|sha256)$'` 不應有輸出。  

## 12. 交接時附上的檔案與快照指令

必附：`HANDOFF.md`、`src/index.html`（單一前端來源）。做 Android 或 Tauri 相關工作時加附 6.1 表內的檔案，以及 `tauri.conf.json`、`capabilities/default.json`、`Cargo.toml`、`build.rs`、`lib.rs`、`main.rs`、`macos/bio.swift`、`package.json`；改文件時加附 `README.md`。不要附 `target/`、`node_modules/`、建置輸出、圖示、lock 檔、`.git`。  

完整快照（工作目錄的現況，含未提交修改）：

```
cd ~/gits/ArgoVault
a=src-tauri/gen/android/app
k=$a/src/main/java/com/havelight/pypasswords
tar -czf ~/ArgoVault_snapshot_$(date +%Y%m%d).tar.gz HANDOFF.md README.md package.json src/index.html src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/build.rs src-tauri/capabilities/default.json src-tauri/src/lib.rs src-tauri/src/main.rs src-tauri/macos/bio.swift $a/build.gradle.kts $a/proguard-rules.pro $a/src/main/AndroidManifest.xml $a/src/main/res/xml/autofill_service.xml $a/src/main/res/values/strings.xml $k/MainActivity.kt $k/PwBio.kt $k/ArgoAutofillService.kt $k/AutofillAuthActivity.kt $a/src/main/res/layout/autofill_item.xml
tar -tzf ~/ArgoVault_snapshot_$(date +%Y%m%d).tar.gz
```

2、3：縮短路徑用的變數。  
4：打包；任何路徑不存在會報錯，依訊息修正。  
5：列出內容確認沒有漏帶；若對話介面拒收 `.tar.gz`，改用 `zip` 或逐檔上傳。  

只做前端或文件時的精簡快照：  

```
cd ~/gits/ArgoVault
tar -czf ~/ArgoVault_min_$(date +%Y%m%d).tar.gz HANDOFF.md README.md src/index.html
```

## 13. 變更紀錄

- 2026-11-11：改名 ArgoVault；新增 `reauth` 設定；新增生物辨識解鎖（實機 OK）。  
- 2026-11-12：桌面名稱（`strings.xml`、`tauri.conf.json`）改為 ArgoVault 並推送；README 與交接文件進倉庫；新增自動填入（方案 A）並成功建置安裝，待實機測試；本文件精簡重寫（移除內嵌程式碼與歷史敘事，改附快照）。  

- 2026-11-13：自動填入實機驗證通過；建議按鈕改自訂版面；編輯前主碼對話框加生物辨識；編輯頁加密碼複製鈕；編輯中切背景不鎖庫（見 9）；三項皆實機通過，自動填入相關檔案首次提交（`a7bee98`）。
- 2026-11-14：放棄 macOS 自動填入；新增 macOS 生物辨識解鎖（Touch ID 加 Secure Enclave，見 6.6），實機通過。清單頁按返回鍵改為只鎖定、不退出 App（實機 OK，APK 於 macOS 主機建置）。版本改為 0.1.1（`tauri.conf.json`、`Cargo.toml`、`package.json` 與兩個 lock 檔），發佈 GitHub Release `0.1.1`（附 APK 與 arm64 `.dmg`；`.dmg` 建置已驗證，`.deb` 須在 Ubuntu 建置，未附）。  
- 2026-11-14：新增 Windows 建置說明（7.1）、`.gitattributes`，`.gitignore` 加 `*.msi`、`*.exe`、`*.deb`；新增 Windows Hello 生物辨識解鎖（6.7，僅通過型別檢查）；Windows 自動填入暫緩。  

## 參考資料

- Apple. (n.d.). *SecureEnclave*. Apple Developer Documentation. https://developer.apple.com/documentation/cryptokit/secureenclave  
- Android Developers. (n.d.-a). *Show a biometric authentication dialog*. Google. https://developer.android.com/identity/sign-in/biometric-auth  
- Android Developers. (n.d.-b). *Build autofill services*. Google. https://developer.android.com/identity/autofill/autofill-services  
- Android Developers Blog. (2024, October 16). *Chrome on Android to support third-party autofill services natively*. Google. https://android-developers.googleblog.com/2024/10/chrome-3p-autofill-services.html  
- GitHub Docs. (n.d.). *Renaming a repository*. GitHub. https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository  
- Microsoft. (n.d.). *KeyCredentialManager class*. Microsoft Learn. https://learn.microsoft.com/en-us/uwp/api/windows.security.credentials.keycredentialmanager  
- Tauri. (n.d.-a). *Biometric*. https://v2.tauri.app/fr/plugin/biometric/  
- Tauri. (n.d.-b). *Prerequisites*. https://v2.tauri.app/start/prerequisites/  
