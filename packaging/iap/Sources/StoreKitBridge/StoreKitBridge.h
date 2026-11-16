#ifndef STOREKIT_BRIDGE_H
#define STOREKIT_BRIDGE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// ABI 版本，Rust 侧加载后校验；不匹配直接拒绝。
int32_t sk_abi_version(void);

/// 查询商品。返回 JSON：{"ok":true,"products":[...]}。
char *sk_products(void);

/// 发起购买。product_id 传 NULL 时使用内置商品 ID。
/// 返回 JSON：{"ok":true,"purchase":{"status":"purchased","entitlement":{...}}}。
char *sk_purchase(const char *product_id);

/// 查询当前权益。返回 JSON：{"ok":true,"entitlement":{...}}。
char *sk_entitlement(void);

/// 恢复购买。返回 JSON：{"ok":true,"entitlement":{...}}。
char *sk_restore(void);

/// 释放以上函数返回的字符串。
void sk_free(char *pointer);

#ifdef __cplusplus
}
#endif

#endif
