# 移动端扩展功能配置（M4）

语音输入、图片输入已在代码中完成（M4-1/M4-2）。以下两项需要 **native 平台配置**,
无法仅靠 Flutter 代码 + headless 构建完成 —— 它们需要在 Xcode / Google Cloud Console /
Apple Developer 里有凭证、并新增 native target,然后真机验证。本文件给出步骤。

## 后台推送（agent 完成提醒）

用途:用户切走 app 后,长任务(agent 多轮工具调用)完成时弹本地/远程通知。

### 最简方案:本地通知(无需服务端,推荐先做)
- `flutter pub add flutter_local_notifications`
- iOS: Info.plist 无需额外;Capability 加 "Push Notifications" 仅远程才需。
- Android: `AndroidManifest.xml` 加 `POST_NOTIFICATIONS`(API 33+)。
- server 侧在 `run_turn` 结束时,若 WS 已断开,无法主动推送(WS 是长连接,
  断了就没了)。本地通知适合"app 在后台、WS 暂挂"场景:客户端在 turn 完成时
  自己弹通知。

### 远程推送(多设备 / 真·后台)
- **iOS (APNs)**:
  1. Apple Developer → Keys → 创建 APNs Auth Key(`.p8`),记下 Key ID、Team ID。
  2. Xcode → Runner target → Signing & Capabilities → + "Push Notifications"。
  3. 服务端(`hermes-server`)用 `a2`/`p8` 库向 APNs 发:`POST https://api.push.apple.com/3/device/<device-token>`。
  4. app 启动注册 `UIApplication.shared.registerForRemoteNotifications`,把 device token 上报 server(需新 REST `/api/v1/devices`)。
- **Android (FCM)**:
  1. Firebase Console → 项目 → 项目设置 → 云消息传递 → 拿 Server Key / 新建 service account JSON。
  2. `flutter pub add firebase_messaging`,按 Firebase 指引加 `google-services.json`(Android)/`GoogleService-Info.plist`(iOS)。
  3. server 用 FCM HTTP v1 API 发送。
- **Hermes 侧改动**(远程推送才需要):
  - 新增设备注册 REST(`/api/v1/devices` POST {token, platform})。
  - `hermes-server` 在 `handle_send` 的 turn 完成回调里,查该 session 的设备,
    若 WS 已关闭则发推送(需要推送客户端 crate,如 `a2` for APNs、`fcm` for FCM)。
  - 这会把 hermes-server 耦合到推送 provider —— 建议放 feature flag 后。

## 分享扩展(从其他 app 分享内容给 Hermes)

用途:在 Safari/相册/文件 app 里点"分享"→ 直接把文本/图片发给 Hermes 起一轮对话。

### iOS Share Extension
1. Xcode → File → New → Target → "Share Extension",命名 `HermesShare`。
2. 新 target 的 `Info.plist` `NSExtension` 配置:
   - `NSExtensionAttributes` → `NSExtensionActivationRule`:允许 text / image / url。
   - `NSExtensionPointIdentifier` = `com.apple.share-services`。
   - `NSExtensionPrincipalClass` = `$(PRODUCT_MODULE_NAME).ShareViewController`。
3. `ShareViewController.swift`:把用户输入/选中的内容 POST 到
   `http://<server>:8765/api/v1/sessions`(新建会话)或经 app group 把内容传给主 app。
4. App Group:主 app 和 share extension 共享一个 App Group,
   `NSUserDefaults(suiteName:)` / 共享文件传递草稿;或 share extension 直接调 hermes-server REST。
5. 主 app `Runner.entitlements` 加 `com.apple.security.application-groups`。

### Android Share (intent-filter)
1. `AndroidManifest.xml` 的 `MainActivity` 加 `<intent-filter>` 接收 `android.intent.action.SEND`
   (text/plain, image/*):
   ```xml
   <intent-filter>
     <action android:name="android.intent.action.SEND"/>
     <category android:name="android.intent.category.DEFAULT"/>
     <data android:mimeType="text/plain"/>
     <data android:mimeType="image/*"/>
   </intent-filter>
   ```
2. `MainActivity.kt` 的 `onCreate`/`onNewIntent` 读 `Intent.EXTRA_TEXT` / `EXTRA_STREAM`,
   通过 MethodChannel 传给 Flutter。
3. Flutter 侧用 `platform_channel` 接收,自动填入输入框(文本)或加入待发附件(图片)。

### 实现优先级
- **本地通知**:最小、无凭证,适合"agent 完成提醒"的 MVP。
- **Android 分享**:intent-filter 纯 manifest + MethodChannel,无需凭证,可较快落地。
- **远程推送 / iOS Share Extension**:需要 APNs key / App Group / 新 target,
  工作量较大,建议作为后续迭代。
