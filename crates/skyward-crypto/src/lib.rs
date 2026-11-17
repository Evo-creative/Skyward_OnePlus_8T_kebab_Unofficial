pub mod encryption;
pub mod fscrypt;
pub mod gatekeeper;
pub mod keymaster;
pub mod keystore;
pub mod locksettings;
pub mod sharedsecret;
pub mod token;
pub mod user;
pub mod weaver;

#[cfg(target_os = "android")]
pub mod data;
#[cfg(target_os = "android")]
pub mod keymint;
#[cfg(target_os = "android")]
pub mod keystore2;

/// Do `work` with the phone's key service, reached afresh: the newer
/// layout where the phone runs it, else the older one over the hardware
/// driver.
#[cfg(target_os = "android")]
pub fn with_key_service<T>(work: impl FnOnce(&dyn KeyService) -> anyhow::Result<T>) -> anyhow::Result<T> {
    use skyward_binder::driver::Binder;
    let binder = Binder::open()?;
    if let Ok(service) = keymint::KeyMint::open(&binder) {
        return work(&service);
    }
    let hardware = Binder::hardware()?;
    let service = keymaster::Keymaster::open(&hardware)?;
    work(&service)
}

/// What unlocks the keys the phone keeps: its key service, whichever of the
/// two layouts it speaks.
#[cfg(target_os = "android")]
pub trait KeyService {
    /// The key in the form this start of the phone can use it in, from the
    /// form a storage key is kept in.
    fn ephemeral_storage_key(&self, key: &[u8]) -> anyhow::Result<Vec<u8>>;

    /// Undo an AES-GCM encryption the key service did with `key_blob`. A
    /// key guarded by a screen lock takes the note the secure world wrote
    /// when it was answered.
    fn gcm_decrypt(
        &self,
        key_blob: &[u8],
        app_id: &[u8],
        nonce: &[u8],
        body: &[u8],
        note: Option<&token::Token>,
    ) -> anyhow::Result<Vec<u8>>;
}

#[cfg(target_os = "android")]
impl KeyService for keymint::KeyMint<'_> {
    fn ephemeral_storage_key(&self, key: &[u8]) -> anyhow::Result<Vec<u8>> {
        keymint::KeyMint::ephemeral_storage_key(self, key)
    }

    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8], note: Option<&token::Token>) -> anyhow::Result<Vec<u8>> {
        keymint::KeyMint::gcm_decrypt(self, key_blob, app_id, nonce, body, note)
    }
}

#[cfg(target_os = "android")]
impl KeyService for keymaster::Keymaster<'_> {
    fn ephemeral_storage_key(&self, key: &[u8]) -> anyhow::Result<Vec<u8>> {
        keymaster::Keymaster::ephemeral_storage_key(self, key)
    }

    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8], note: Option<&token::Token>) -> anyhow::Result<Vec<u8>> {
        keymaster::Keymaster::gcm_decrypt(self, key_blob, app_id, nonce, body, note)
    }
}

#[cfg(target_os = "android")]
impl keystore::Decryptor for dyn KeyService + '_ {
    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8]) -> anyhow::Result<Vec<u8>> {
        // The keys unwrapped this way belong to the phone rather than to
        // any user, so none of them is guarded by a screen lock.
        KeyService::gcm_decrypt(self, key_blob, app_id, nonce, body, None)
    }
}

#[cfg(target_os = "android")]
impl keystore::Decryptor for keymaster::Keymaster<'_> {
    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8]) -> anyhow::Result<Vec<u8>> {
        keymaster::Keymaster::gcm_decrypt(self, key_blob, app_id, nonce, body, None)
    }
}

#[cfg(target_os = "android")]
impl keystore::Decryptor for keymint::KeyMint<'_> {
    fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8]) -> anyhow::Result<Vec<u8>> {
        // The keys unwrapped this way belong to the phone rather than to
        // any user, so none of them is guarded by a screen lock.
        keymint::KeyMint::gcm_decrypt(self, key_blob, app_id, nonce, body, None)
    }
}
