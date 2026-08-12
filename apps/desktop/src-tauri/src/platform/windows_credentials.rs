use keyring_core::{api::CredentialStoreApi, Entry, Error as KeyringError};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};
use windows_native_keyring_store::Store;

static WINDOWS_CREDENTIAL_STORE: OnceLock<Arc<Store>> = OnceLock::new();
static WINDOWS_CREDENTIAL_IO: Mutex<()> = Mutex::new(());

fn credential_target(service: &str, user: &str) -> String {
    format!("PaperFloatTranslator/{service}/{user}")
}

fn credential_store() -> Result<&'static Arc<Store>, String> {
    if let Some(store) = WINDOWS_CREDENTIAL_STORE.get() {
        return Ok(store);
    }

    let candidate =
        Store::new().map_err(|error| format!("初始化 Windows 凭据管理器失败：{error}"))?;
    let _ = WINDOWS_CREDENTIAL_STORE.set(candidate);
    WINDOWS_CREDENTIAL_STORE
        .get()
        .ok_or_else(|| "初始化 Windows 凭据管理器后无法取得存储实例。".to_owned())
}

fn credential_io_guard() -> Result<MutexGuard<'static, ()>, String> {
    WINDOWS_CREDENTIAL_IO
        .lock()
        .map_err(|_| "Windows 凭据管理器串行访问锁已损坏；拒绝继续访问密钥。".to_owned())
}

fn entry(service: &str, user: &str) -> Result<Entry, String> {
    let target = credential_target(service, user);
    let modifiers = HashMap::from([("target", target.as_str()), ("persistence", "Local")]);
    credential_store()?
        .build(service, user, Some(&modifiers))
        .map_err(|error| format!("创建 Windows 凭据条目失败：{error}"))
}

pub(crate) fn get_secret(service: &str, user: &str) -> Result<Option<String>, String> {
    let _guard = credential_io_guard()?;
    match entry(service, user)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(format!("从 Windows 凭据管理器读取 API Key 失败：{error}")),
    }
}

pub(crate) fn secret_exists(service: &str, user: &str) -> Result<bool, String> {
    get_secret(service, user).map(|secret| secret.is_some())
}

pub(crate) fn set_secret(service: &str, user: &str, secret: &str) -> Result<(), String> {
    let _guard = credential_io_guard()?;
    entry(service, user)?
        .set_password(secret)
        .map_err(|error| format!("将 API Key 写入 Windows 凭据管理器失败：{error}"))
}

pub(crate) fn delete_secret(service: &str, user: &str) -> Result<(), String> {
    let _guard = credential_io_guard()?;
    match entry(service, user)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(format!("从 Windows 凭据管理器删除 API Key 失败：{error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_and_acceptance_targets_are_isolated() {
        let production =
            credential_target("Paper Float Translator API Key v2", "deepseek-api-key-v2");
        let acceptance = credential_target(
            "Paper Float Translator Acceptance API Key v2",
            "deepseek-api-key-acceptance-v2",
        );

        assert_ne!(production, acceptance);
        assert!(production.starts_with("PaperFloatTranslator/"));
        assert!(acceptance.contains("Acceptance"));
    }
}
