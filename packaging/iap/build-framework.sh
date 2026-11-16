#!/bin/bash
# 构建 StoreKitBridge.framework（arm64 + x86_64，ad-hoc 签名）。
# 产物随 App 放入 Contents/Frameworks，由 Rust 侧 dlopen 后经 C ABI 调用。
set -euo pipefail
cd "$(dirname "$0")"

root="$PWD"
out="$root/Framework/StoreKitBridge.framework"
versioned="$out/Versions/A"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

rm -rf "$out"
mkdir -p "$versioned/Headers" "$versioned/Modules" "$versioned/Resources"

for arch in arm64 x86_64; do
  xcrun swiftc -emit-library -module-name StoreKitBridge -swift-version 5 \
    -target "$arch-apple-macos13.0" \
    -Xlinker -install_name -Xlinker "@rpath/StoreKitBridge.framework/Versions/A/StoreKitBridge" \
    -o "$tmp/StoreKitBridge-$arch" \
    Sources/StoreKitCore/Protocol.swift \
    Sources/StoreKitCore/StoreKitService.swift \
    Sources/StoreKitBridge/Bridge.swift
done

lipo -create "$tmp"/StoreKitBridge-arm64 "$tmp"/StoreKitBridge-x86_64 -output "$versioned/StoreKitBridge"
cp Sources/StoreKitBridge/StoreKitBridge.h "$versioned/Headers/"
cp Sources/StoreKitBridge/module.modulemap "$versioned/Modules/"
cp Sources/StoreKitBridge/Info.plist "$versioned/Resources/Info.plist"

ln -s A "$out/Versions/Current"
ln -s Versions/Current/StoreKitBridge "$out/StoreKitBridge"
ln -s Versions/Current/Headers "$out/Headers"
ln -s Versions/Current/Modules "$out/Modules"
ln -s Versions/Current/Resources "$out/Resources"

codesign --force --sign - "$out"
codesign --verify --strict "$out"

echo "StoreKitBridge.framework 已生成：$out"
lipo -archs "$versioned/StoreKitBridge"
