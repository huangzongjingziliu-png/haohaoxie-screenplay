//! Credentials are kept in the macOS Keychain, never in project/config JSON.
#[cfg(target_os = "macos")]
mod native {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    type Ref = *const c_void;
    const SERVICE: &str = "com.screenplay.studio.agent";
    const ACCOUNT: &str = "default-provider";

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithBytes(a: Ref, b: *const u8, n: isize, encoding: u32, external: bool) -> Ref;
        fn CFDictionaryCreateMutable(a: Ref, capacity: isize, keys: Ref, values: Ref) -> *mut c_void;
        fn CFDictionarySetValue(d: *mut c_void, key: Ref, value: Ref);
        fn CFDataCreate(a: Ref, bytes: *const u8, n: isize) -> Ref;
        fn CFDataGetLength(d: Ref) -> isize;
        fn CFDataGetBytePtr(d: Ref) -> *const u8;
        fn CFRelease(value: Ref);
        static kCFBooleanTrue: Ref;
    }
    #[link(name = "Security", kind = "framework")]
    extern "C" {
        static kSecClass: Ref;
        static kSecClassGenericPassword: Ref;
        static kSecAttrService: Ref;
        static kSecAttrAccount: Ref;
        static kSecReturnData: Ref;
        static kSecValueData: Ref;
        static kSecUseAuthenticationUI: Ref;
        static kSecUseAuthenticationUIFail: Ref;
        fn SecItemCopyMatching(query: Ref, result: *mut Ref) -> i32;
        fn SecItemAdd(attributes: Ref, result: *mut Ref) -> i32;
        fn SecItemUpdate(query: Ref, attributes: Ref) -> i32;
    }

    struct Query { dictionary: *mut c_void, service: Ref, account: Ref }
    impl Query {
        unsafe fn new() -> Self {
            let dictionary = CFDictionaryCreateMutable(null(), 0, null(), null());
            let service = CFStringCreateWithBytes(null(), SERVICE.as_ptr(), SERVICE.len() as isize, 0x08000100, false);
            let account = CFStringCreateWithBytes(null(), ACCOUNT.as_ptr(), ACCOUNT.len() as isize, 0x08000100, false);
            CFDictionarySetValue(dictionary, kSecClass, kSecClassGenericPassword);
            CFDictionarySetValue(dictionary, kSecAttrService, service);
            CFDictionarySetValue(dictionary, kSecAttrAccount, account);
            Self { dictionary, service, account }
        }
    }
    impl Drop for Query {
        fn drop(&mut self) { unsafe {
            CFRelease(self.dictionary); CFRelease(self.service); CFRelease(self.account);
        } }
    }

    pub fn load() -> Result<Option<String>, String> {
        unsafe {
            let query = Query::new();
            CFDictionarySetValue(query.dictionary, kSecUseAuthenticationUI, kSecUseAuthenticationUIFail);
            CFDictionarySetValue(query.dictionary, kSecReturnData, kCFBooleanTrue);
            let mut result: Ref = null();
            let code = SecItemCopyMatching(query.dictionary, &mut result);
            if code == -25300 { return Ok(None); }
            if code != 0 { return Err(format!("无法读取系统钥匙串（{code}）")); }
            let length = CFDataGetLength(result) as usize;
            let bytes = if length == 0 { Vec::new() } else { std::slice::from_raw_parts(CFDataGetBytePtr(result), length).to_vec() };
            CFRelease(result);
            String::from_utf8(bytes).map(Some).map_err(|_| "钥匙串密钥编码无效".into())
        }
    }

    pub fn save(secret: &str) -> Result<(), String> {
        unsafe {
            let query = Query::new();
            CFDictionarySetValue(query.dictionary, kSecUseAuthenticationUI, kSecUseAuthenticationUIFail);
            let data = CFDataCreate(null(), secret.as_ptr(), secret.len() as isize);
            let update = CFDictionaryCreateMutable(null(), 0, null(), null());
            CFDictionarySetValue(update, kSecValueData, data);
            let mut code = SecItemUpdate(query.dictionary, update);
            if code == -25300 {
                CFDictionarySetValue(query.dictionary, kSecValueData, data);
                code = SecItemAdd(query.dictionary, null_mut());
            }
            CFRelease(update); CFRelease(data);
            if code == 0 { Ok(()) } else { Err(format!("无法保存到系统钥匙串（{code}）")) }
        }
    }
}

#[cfg(target_os = "macos")]
pub use native::{load, save};
#[cfg(not(target_os = "macos"))]
pub fn load() -> Result<Option<String>, String> { Ok(None) }
#[cfg(not(target_os = "macos"))]
pub fn save(_: &str) -> Result<(), String> { Err("当前平台不支持 macOS 钥匙串".into()) }
