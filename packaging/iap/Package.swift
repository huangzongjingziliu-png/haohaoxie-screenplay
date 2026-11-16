// swift-tools-version: 6.0
import PackageDescription

// SwiftPM 只用于 StoreKitCore 的日常编译检查；生产路径是 StoreKitBridge.framework，
// 见 build-framework.sh。StoreKit Configuration 的自动化测试需要「App 宿主内的
// XCTest」，见 XcodeHost/StoreKitTestHost.xcodeproj。
let package = Package(
    name: "StoreKitBridge",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "StoreKitCore", targets: ["StoreKitCore"]),
    ],
    targets: [
        .target(name: "StoreKitCore"),
    ]
)
