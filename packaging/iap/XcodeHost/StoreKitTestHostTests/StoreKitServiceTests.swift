import XCTest
import StoreKitTest

/// 通过 App 宿主内 XCTest + StoreKit Configuration 验证 StoreKitBridge.framework。
/// 测试用 dlopen 加载与 Rust 完全相同的二进制，再经 C ABI 调用，
/// 保证验证的是生产路径本身，而不是同源码的另一份编译产物。
final class StoreKitBridgeTests: XCTestCase {
    private let productID = "com.screenplay.studio.pro"
    private var session: SKTestSession!
    private var handle: UnsafeMutableRawPointer!

    private typealias AbiVersionFn = @convention(c) () -> Int32
    private typealias ProductsFn = @convention(c) () -> UnsafeMutablePointer<CChar>?
    private typealias PurchaseFn = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
    private typealias EntitlementFn = @convention(c) () -> UnsafeMutablePointer<CChar>?
    private typealias FreeFn = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void

    override func setUpWithError() throws {
        let bundle = Bundle(for: Self.self)
        let url = try XCTUnwrap(bundle.url(forResource: "Pro", withExtension: "storekit"))
        session = try SKTestSession(contentsOf: url)
        session.disableDialogs = true
        session.clearTransactions()

        let path = ProcessInfo.processInfo.environment["STOREKIT_BRIDGE_PATH"] ?? Self.frameworkPath
        XCTAssertTrue(FileManager.default.fileExists(atPath: path), "先运行 packaging/iap/build-framework.sh")
        handle = dlopen(path, RTLD_NOW)
        let error = dlerror()
        XCTAssertNotNil(handle, error.map { String(cString: $0) } ?? "dlopen 失败")
        if handle != nil {
            let version: AbiVersionFn = symbol("sk_abi_version")
            XCTAssertEqual(version(), 1)
        }
    }

    override func tearDown() {
        if handle != nil {
            dlclose(handle)
            handle = nil
        }
    }

    func testQueryProducts() async throws {
        let products: ProductsFn = symbol("sk_products")
        let json = await call { products() }

        XCTAssertEqual(json["ok"] as? Bool, true)
        let list = json["products"] as? [[String: Any]]
        XCTAssertEqual(list?.count, 1)
        XCTAssertEqual(list?.first?["id"] as? String, productID)
        XCTAssertFalse((list?.first?["displayPrice"] as? String ?? "").isEmpty)
    }

    func testPurchaseThenEntitlement() async throws {
        let purchase: PurchaseFn = symbol("sk_purchase")
        let entitlement: EntitlementFn = symbol("sk_entitlement")

        let before = await call { entitlement() }
        XCTAssertEqual((before["entitlement"] as? [String: Any])?["isPro"] as? Bool, false)

        let bought = await call { purchase(nil) }
        XCTAssertEqual(bought["ok"] as? Bool, true)
        let outcome = bought["purchase"] as? [String: Any]
        XCTAssertEqual(outcome?["status"] as? String, "purchased")
        XCTAssertEqual((outcome?["entitlement"] as? [String: Any])?["isPro"] as? Bool, true)

        let after = await call { entitlement() }
        let status = after["entitlement"] as? [String: Any]
        XCTAssertEqual(status?["isPro"] as? Bool, true)
        XCTAssertEqual(status?["verified"] as? Bool, true)
    }

    func testRestoreAfterPurchase() async throws {
        let purchase: PurchaseFn = symbol("sk_purchase")
        let restore: EntitlementFn = symbol("sk_restore")

        _ = await call { purchase(nil) }
        let restored = await call { restore() }
        XCTAssertEqual(restored["ok"] as? Bool, true)
        XCTAssertEqual((restored["entitlement"] as? [String: Any])?["isPro"] as? Bool, true)
    }

    // MARK: - 工具

    /// 在后台线程调用 C ABI，避免阻塞主线程导致 StoreKit 的购买弹窗无法呈现。
    private func call(_ body: @escaping () -> UnsafeMutablePointer<CChar>?) async -> [String: Any] {
        await withCheckedContinuation { continuation in
            DispatchQueue.global().async {
                guard let pointer = body() else {
                    continuation.resume(returning: [:])
                    return
                }
                let text = String(cString: pointer)
                let free: FreeFn = self.symbol("sk_free")
                free(pointer)
                let json = (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any]
                continuation.resume(returning: json ?? [:])
            }
        }
    }

    private func symbol<T>(_ name: String) -> T {
        guard let pointer = dlsym(handle, name) else {
            fatalError("StoreKitBridge 缺少符号 \(name)")
        }
        return unsafeBitCast(pointer, to: T.self)
    }

    private static var frameworkPath: String {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // StoreKitTestHostTests
            .deletingLastPathComponent() // XcodeHost
            .deletingLastPathComponent() // iap
            .appendingPathComponent("Framework/StoreKitBridge.framework/StoreKitBridge")
            .path
    }
}
