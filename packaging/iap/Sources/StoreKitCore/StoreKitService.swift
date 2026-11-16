import Foundation
import StoreKit

public struct HelperFailure: Error, Sendable {
    public let code: HelperErrorCode
    public let message: String

    public init(_ code: HelperErrorCode, _ message: String) {
        self.code = code
        self.message = message
    }
}

/// StoreKit 2 的薄封装：只做四件事——查商品、购买、查权益、恢复购买。
/// 不缓存、不落盘；缓存由宿主（Rust 侧）负责，避免 helper 成为第二个状态源。
public actor StoreKitService {
    private let productIDs: [String]

    public init(productIDs: [String]) {
        self.productIDs = productIDs
    }

    public func products() async throws -> [ProductInfo] {
        let products = try await Product.products(for: productIDs)
        return products.map {
            ProductInfo(
                id: $0.id,
                displayName: $0.displayName,
                description: $0.description,
                displayPrice: $0.displayPrice,
                type: String(describing: $0.type)
            )
        }
    }

    public func entitlements() async -> Entitlement {
        for await result in Transaction.currentEntitlements {
            guard case .verified(let transaction) = result else { continue }
            guard productIDs.contains(transaction.productID), transaction.revocationDate == nil else { continue }
            return Entitlement(
                isPro: true,
                productID: transaction.productID,
                originalTransactionID: String(transaction.originalID),
                purchaseDate: ISO8601DateFormatter().string(from: transaction.purchaseDate),
                environment: transaction.environment.rawValue,
                verified: true
            )
        }
        return .none
    }

    public func purchase(productID: String) async throws -> PurchaseOutcome {
        guard let product = try await Product.products(for: [productID]).first else {
            throw HelperFailure(.productNotFound, "未找到商品：\(productID)")
        }
        switch try await product.purchase() {
        case .success(let verification):
            switch verification {
            case .verified(let transaction):
                await transaction.finish()
                return PurchaseOutcome(status: .purchased, entitlement: await entitlements())
            case .unverified(_, let error):
                throw HelperFailure(.verificationFailed, "交易校验失败：\(error.localizedDescription)")
            }
        case .userCancelled:
            return PurchaseOutcome(status: .cancelled, entitlement: await entitlements())
        case .pending:
            return PurchaseOutcome(status: .pending, entitlement: await entitlements())
        @unknown default:
            throw HelperFailure(.storeError, "未知购买结果")
        }
    }

    public func restore() async throws -> Entitlement {
        do {
            try await AppStore.sync()
        } catch {
            throw HelperFailure(.storeError, "恢复购买失败：\(error.localizedDescription)")
        }
        return await entitlements()
    }
}
