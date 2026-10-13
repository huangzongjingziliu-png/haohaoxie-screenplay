# 软件更新 / App updates

软件每天在后台检查一次本项目的 GitHub Releases，包括测试版。发现新版后，工具栏出现蓝色向上箭头。也可在“更多 → 检查更新”或“设置 → 软件更新”手动检查，并关闭自动检查。

点击箭头查看更新说明。下载完成后会验证发布者签名；点击“保存并重启更新”才安装。取消保存、保存失败或有新的未保存修改都会停止安装。无需配置大模型。旧版没有此功能，需要先手动安装一个支持自动更新的版本。

The app checks this repository's GitHub releases daily, including betas. A blue up arrow appears when a newer version is available. Use More → Check for updates or Settings → App updates to check manually or disable background checks. Downloading does not restart the app. Installation requires an explicit click and a saved project. Publisher signatures are verified; these are separate from Apple code signing and notarization.

## 发布者 / Publisher

使用 `bash scripts/build-update-release.sh --tag v<版本>-beta.1 --notes <更新说明文件>` 构建签名更新包。版本须与 package.json 一致，每个新版均递增桌面版本。当前生成 Apple Silicon 包；尚未提供 Intel 自动更新包。

私钥保存在本机 Application Support/ScreenplayStudio/update-signing/private.key，目录权限 700，文件权限 600，绝不进入仓库或安装包。请另行安全备份，丢失后将无法为已安装用户签署同一密钥的更新。代码中只保存公钥。其他构建机通过 TAURI_SIGNING_PRIVATE_KEY 和 TAURI_SIGNING_PRIVATE_KEY_PASSWORD 提供同一私钥，不要重新生成覆盖。

上传同一 Release 的三个文件：`*.app.tar.gz`、`*.app.tar.gz.sig`、`latest.json`；最后上传 manifest，再发布 Release。保留 DMG 作为手动安装入口。未提供 manifest 的新版只显示 GitHub 下载入口。禁止把本地测试 manifest 指向未上传的更新包后直接发布。

Build with the script above, provide the same private key on other build machines, and upload the updater archive, its signature and latest.json to the same release. Publish the manifest last. Keep the DMG as a manual fallback. Never commit the private signing key. The key must be backed up securely; updater signatures do not replace Apple Developer ID signing.
