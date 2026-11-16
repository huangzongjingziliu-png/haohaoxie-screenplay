import Foundation
import StoreKit

// C ABI 出口：framework 随 App 主进程加载，Rust 通过 dlopen/dlsym 调用。
// 所有函数返回 malloc 的 JSON 字符串，调用方必须用 sk_free 释放；失败也返回
// JSON（ok=false），不返回 NULL。查询类动作 4 秒超时；购买包含用户交互，
// 不能套用 4 秒，给 120 秒并由调用方兜底。

private let productID = "com.screenplay.studio.pro"
private let service = StoreKitService(productIDs: [productID])
private let readTimeout: TimeInterval = 4
private let purchaseTimeout: TimeInterval = 120

@_cdecl("sk_abi_version")
public func skABIVersion() -> Int32 { 1 }

@_cdecl("sk_products")
public func skProducts() -> UnsafeMutablePointer<CChar>? {
    let response = wait(readTimeout) { () async -> Response in
        do {
            return Response(id: nil, ok: true, products: try await service.products())
        } catch {
            return Response(id: nil, ok: false, error: HelperError(code: .storeError, message: error.localizedDescription))
        }
    }
    return encode(response ?? timeout())
}

@_cdecl("sk_purchase")
public func skPurchase(_ productPointer: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    let requested = productPointer.map { String(cString: $0) } ?? productID
    let response = wait(purchaseTimeout) { () async -> Response in
        do {
            return Response(id: nil, ok: true, purchase: try await service.purchase(productID: requested))
        } catch let failure as HelperFailure {
            return Response(id: nil, ok: false, error: HelperError(code: failure.code, message: failure.message))
        } catch {
            return Response(id: nil, ok: false, error: HelperError(code: .storeError, message: error.localizedDescription))
        }
    }
    return encode(response ?? timeout(seconds: purchaseTimeout))
}

@_cdecl("sk_entitlement")
public func skEntitlement() -> UnsafeMutablePointer<CChar>? {
    let response = wait(readTimeout) { () async -> Response in
        Response(id: nil, ok: true, entitlement: await service.entitlements())
    }
    return encode(response ?? timeout())
}

@_cdecl("sk_restore")
public func skRestore() -> UnsafeMutablePointer<CChar>? {
    let response = wait(readTimeout) { () async -> Response in
        do {
            return Response(id: nil, ok: true, entitlement: try await service.restore())
        } catch let failure as HelperFailure {
            return Response(id: nil, ok: false, error: HelperError(code: failure.code, message: failure.message))
        } catch {
            return Response(id: nil, ok: false, error: HelperError(code: .storeError, message: error.localizedDescription))
        }
    }
    return encode(response ?? timeout())
}

@_cdecl("sk_free")
public func skFree(_ pointer: UnsafeMutablePointer<CChar>?) {
    free(pointer)
}

// MARK: - 内部

private func encode(_ response: Response) -> UnsafeMutablePointer<CChar>? {
    let encoder = JSONEncoder()
    guard let data = try? encoder.encode(response),
          let text = String(data: data, encoding: .utf8) else { return nil }
    return strdup(text)
}

private func timeout(seconds: TimeInterval = readTimeout) -> Response {
    Response(
        id: nil,
        ok: false,
        error: HelperError(code: .storeError, message: "StoreKit 调用超时（\(Int(seconds)) 秒）")
    )
}

private final class Box<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: T?

    func set(_ newValue: T) {
        lock.lock()
        value = newValue
        lock.unlock()
    }

    func get() -> T? {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

/// 阻塞等待异步 StoreKit 调用，超过时限返回 nil。
/// 调用方必须不在主线程，否则会阻塞 App 的 run loop。
private func wait<T: Sendable>(_ seconds: TimeInterval, _ operation: @escaping @Sendable () async -> T) -> T? {
    let box = Box<T>()
    let semaphore = DispatchSemaphore(value: 0)
    Task {
        box.set(await operation())
        semaphore.signal()
    }
    if semaphore.wait(timeout: .now() + seconds) == .timedOut { return nil }
    return box.get()
}
