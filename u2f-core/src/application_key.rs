extern crate client_lib;

use self::client_lib::ecdsa::PrivateShare;

use app_id::AppId;
use key_handle::KeyHandle;

// A private key is generated per application
// This stores the AppID for indexing, and a private key for signing
#[derive(Serialize, Deserialize)]
pub struct ApplicationKey {
    pub application: AppId,
    pub handle: KeyHandle,
    key: PrivateShare,
}

impl ApplicationKey {
    pub fn new(application: AppId, handle: KeyHandle, key: PrivateShare) -> ApplicationKey {
        ApplicationKey {
            application,
            handle,
            key,
        }
    }
    pub(crate) fn key(&self) -> &PrivateShare {
        &self.key
    }
}
