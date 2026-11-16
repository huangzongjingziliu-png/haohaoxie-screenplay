import Foundation

/// helper 与宿主之间的 JSON-lines 协议。
/// 宿主每次写一行请求，helper 回一行响应；字段名用 camelCase，方便 Rust/前端直接映射。

public struct Request: Codable, Sendable {
    public let id: String?
    public let action: String
    public let productID: String?

    public init(id: String? = nil, action: String, productID: String? = nil) {
        self.id = id
        self.action = action
        self.productID = productID
    }
}

public struct ProductInfo: Codable, Sendable {
    public let id: String
    public let displayName: String
    public let description: String
    public let displayPrice: String
    public let type: String

    public init(id: String, displayName: String, description: String, displayPrice: String, type: String) {
        self.id = id
        self.displayName = displayName
        self.description = description
        self.displayPrice = displayPrice
        self.type = type
    }
}

public struct Entitlement: Codable, Sendable {
    public let isPro: Bool
    public let productID: String?
    public let originalTransactionID: String?
    public let purchaseDate: String?
    public let environment: String
    public let verified: Bool

    public init(
        isPro: Bool,
        productID: String? = nil,
        originalTransactionID: String? = nil,
        purchaseDate: String? = nil,
        environment: String,
        verified: Bool
    ) {
        self.isPro = isPro
        self.productID = productID
        self.originalTransactionID = originalTransactionID
        self.purchaseDate = purchaseDate
        self.environment = environment
        self.verified = verified
    }

    public static let none = Entitlement(
        isPro: false,
        environment: "Unknown",
        verified: false
    )
}

public enum PurchaseStatus: String, Codable, Sendable {
    case purchased
    case cancelled
    case pending
}

public struct PurchaseOutcome: Codable, Sendable {
    public let status: PurchaseStatus
    public let entitlement: Entitlement

    public init(status: PurchaseStatus, entitlement: Entitlement) {
        self.status = status
        self.entitlement = entitlement
    }
}

public enum HelperErrorCode: String, Codable, Sendable {
    case badRequest
    case unknownAction
    case productNotFound
    case userCancelled
    case pending
    case verificationFailed
    case storeError
}

public struct HelperError: Codable, Sendable {
    public let code: HelperErrorCode
    public let message: String

    public init(code: HelperErrorCode, message: String) {
        self.code = code
        self.message = message
    }
}

public struct Response: Codable, Sendable {
    public let id: String?
    public let ok: Bool
    public let products: [ProductInfo]?
    public let entitlement: Entitlement?
    public let purchase: PurchaseOutcome?
    public let error: HelperError?

    public init(
        id: String?,
        ok: Bool,
        products: [ProductInfo]? = nil,
        entitlement: Entitlement? = nil,
        purchase: PurchaseOutcome? = nil,
        error: HelperError? = nil
    ) {
        self.id = id
        self.ok = ok
        self.products = products
        self.entitlement = entitlement
        self.purchase = purchase
        self.error = error
    }
}
