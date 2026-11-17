//! The phone's key service as the older hardware layout presents it. The
//! newer layout, which the recovery talks to where a phone has one, hands
//! back a live operation; this one numbers the operation and keeps it on
//! the service, so every step of a decryption goes to the same device with
//! that number.
//!
//! The calls and answers serve the phone's own driver and the host tests.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use anyhow::{bail, Result};
#[cfg(target_os = "android")]
use skyward_binder::hidl::{self, Answer, Message};

#[cfg(target_os = "android")]
use anyhow::Context;
#[cfg(target_os = "android")]
use skyward_binder::driver::{Binder, Handle};

/// The key service as the newer of the two hardware layouts names itself.
pub const INTERFACE_41: &str = "android.hardware.keymaster@4.1::IKeymasterDevice";
/// The key service as the older of the two names itself.
pub const INTERFACE_40: &str = "android.hardware.keymaster@4.0::IKeymasterDevice";

const INSTANCE: &str = "default";

/// Every method this client calls is declared in the older of the two
/// layouts, and a service of the newer one dispatches it through the older
/// one's own code, which checks the call against the older name. A call
/// made with the newer name is turned away before its method runs.
const DECLARED_IN: &str = INTERFACE_40;

/// The methods this client uses, as this layout numbers them: the hardware
/// one, exporting a storage key, and the four steps of a decryption.
const GET_HARDWARE_INFO: u32 = 1;
const EXPORT_KEY: u32 = 10;

/// What the phone says about its key hardware, as this layout says it: how
/// well protected it is, its name and who wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hardware {
    /// 0 for software, 1 for the secure world, 2 for a separate secure chip.
    pub security_level: u32,
    pub name: String,
    pub author: String,
}

#[cfg(target_os = "android")]
pub struct Keymaster<'a> {
    device: Handle<'a>,
    interface: &'static str,
}

#[cfg(target_os = "android")]
impl<'a> Keymaster<'a> {
    /// Reach the running key service of either hardware layout, or say that
    /// none is. `hardware` is the end of the driver the phone's hardware
    /// services register with, which is a different one from the rest of
    /// the phone's.
    pub fn open(hardware: &'a Binder) -> Result<Keymaster<'a>> {
        for interface in [INTERFACE_41, INTERFACE_40] {
            if let Some(device) = hidl::find(hardware, interface, INSTANCE)? {
                return Ok(Keymaster { device, interface });
            }
        }
        bail!("the phone runs no keymaster service of the hardware layout")
    }

    /// Which layout the service speaks.
    pub fn interface(&self) -> &'static str {
        self.interface
    }

    /// What the far side says it serves.
    pub fn served(&self) -> Result<String> {
        hidl::describes(&self.device)
    }

    /// What the service says about the hardware it runs on.
    pub fn hardware(&self) -> Result<Hardware> {
        let what = "ask the key service about its hardware";
        let call = Message::call(DECLARED_IN);
        let reply = self.device.transact(GET_HARDWARE_INFO, &call).context(what)?;
        let mut read = Answer::new(reply.bytes());
        read.status().context(what)?;
        let security_level = read.i32().context(what)? as u32;
        let name = read.string().context(what)?;
        let author = read.string().context(what)?;
        Ok(Hardware { security_level, name, author })
    }

    /// The key itself, unwrapped from the form a storage key is kept in.
    /// Android's own keystore does this for vold by exporting the key in the
    /// raw form, which is what a storage key is for.
    pub fn ephemeral_storage_key(&self, key_blob: &[u8]) -> Result<Vec<u8>> {
        let mut call = Message::call(DECLARED_IN);
        // The raw form, which the phone's own key format numbers first.
        call.u32(0);
        call.bytes(key_blob);
        call.bytes(&[]);
        call.bytes(&[]);
        let what = "export the storage key";
        let reply = self.device.transact(EXPORT_KEY, &call).context(what)?;
        let mut read = Answer::new(reply.bytes());
        read.status().context(what)?;
        refused(read.i32().context(what)?, what)?;
        Ok(read.bytes().context(what)?.to_vec())
    }

    /// Undo an AES-GCM encryption the key service did with `key_blob`. The
    /// service only hands back the plaintext when `app_id` is the one the key
    /// was made for, so a wrong secret fails here rather than yielding
    /// something that looks like a key.
    pub fn gcm_decrypt(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], body: &[u8], note: Option<&crate::token::Token>) -> Result<Vec<u8>> {
        let operation = self.begin(key_blob, app_id, nonce, note)?;
        let mut out = self.update(operation, body)?;
        out.extend_from_slice(&self.finish(operation)?);
        Ok(out)
    }

    /// Whether the secure world will unwrap the key at all: a decryption is
    /// opened on it under a nonce of nothing and dropped again, which reads
    /// nothing and changes nothing.
    pub fn usable(&self, key_blob: &[u8], app_id: &[u8]) -> Result<()> {
        let operation = self.begin(key_blob, app_id, &[0; 12], None)?;
        self.abort(operation)
    }

    /// Open a decryption and give the number the service knows it by.
    fn begin(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], note: Option<&crate::token::Token>) -> Result<u64> {
        let mut call = Message::call(DECLARED_IN);
        call.u32(PURPOSE_DECRYPT);
        call.bytes(key_blob);
        let params = gcm_params(app_id, nonce);
        call.structures(params.data, PARAMETER_BYTES, params.pointed);
        let (token, mac) = auth_token(note);
        call.structure(token, mac);
        let what = "start a decryption";
        let reply = self.device.transact(BEGIN, &call).context(what)?;
        let mut read = Answer::new(reply.bytes());
        read.status().context(what)?;
        refused(read.i32().context(what)?, what)?;
        // The parameters the operation takes in, which a decryption leaves
        // as they were, and then the number the service knows it by.
        read.bytes().context(what)?;
        read.u64().context(what)
    }

    fn update(&self, operation: u64, input: &[u8]) -> Result<Vec<u8>> {
        let mut call = Message::call(DECLARED_IN);
        call.u64(operation);
        call.structures(Vec::new(), PARAMETER_BYTES, Vec::new());
        call.bytes(input);
        call.structure(vec![0u8; AUTH_TOKEN_BYTES], vec![(TOKEN_MAC, Vec::new())]);
        let (verification, inside) = empty_verification_token();
        call.structure(verification, inside);
        let what = "feed the decryption";
        let reply = self.device.transact(UPDATE, &call).context(what)?;
        let mut read = Answer::new(reply.bytes());
        read.status().context(what)?;
        refused(read.i32().context(what)?, what)?;
        read.i32().context(what)?;
        // The parameters the operation took in, which a decryption leaves
        // as it found them, and then what it gave back.
        read.bytes().context(what)?;
        Ok(read.bytes().context(what)?.to_vec())
    }

    fn finish(&self, operation: u64) -> Result<Vec<u8>> {
        let mut call = Message::call(DECLARED_IN);
        call.u64(operation);
        call.structures(Vec::new(), PARAMETER_BYTES, Vec::new());
        call.bytes(&[]);
        call.bytes(&[]);
        call.structure(vec![0u8; AUTH_TOKEN_BYTES], vec![(TOKEN_MAC, Vec::new())]);
        let (verification, inside) = empty_verification_token();
        call.structure(verification, inside);
        let what = "close the decryption, which is where a wrong key is refused";
        let reply = self.device.transact(FINISH, &call).context(what)?;
        let mut read = Answer::new(reply.bytes());
        read.status().context(what)?;
        refused(read.i32().context(what)?, what)?;
        // The parameters the operation took in, then what it gave back.
        read.bytes().context(what)?;
        Ok(read.bytes().context(what)?.to_vec())
    }

    fn abort(&self, operation: u64) -> Result<()> {
        let mut call = Message::call(DECLARED_IN);
        call.u64(operation);
        let what = "drop the decryption";
        let reply = self.device.transact(ABORT, &call).context(what)?;
        let mut read = Answer::new(reply.bytes());
        read.status().context(what)?;
        refused(read.i32().context(what)?, what)
    }
}

/// What the service says of the work: a number that is nothing when it
/// went through, and the layout's own name for what stopped it.
fn refused(said: i32, what: &str) -> Result<()> {
    if said != 0 {
        bail!("the key service refused {what} with its code {said}");
    }
    Ok(())
}

/// The note the secure world wrote when the user answered their screen lock,
/// as this layout carries it: who it was for, what they did, when, and the
/// mark the secure world checks it by. A key that is not guarded by a lock
/// takes a token of nothing. The mark is a run of bytes inside the note,
/// and the service looks for it even when it holds nothing.
fn auth_token(note: Option<&crate::token::Token>) -> (Vec<u8>, Vec<(usize, Vec<u8>)>) {
    let mut token = vec![0u8; AUTH_TOKEN_BYTES];
    let Some(note) = note else { return (token, vec![(TOKEN_MAC, Vec::new())]) };
    token[0..8].copy_from_slice(&(note.challenge as u64).to_le_bytes());
    token[8..16].copy_from_slice(&(note.user as u64).to_le_bytes());
    token[16..24].copy_from_slice(&(note.authenticator as u64).to_le_bytes());
    token[24..28].copy_from_slice(&(note.kind as u32).to_le_bytes());
    token[32..40].copy_from_slice(&(note.when as u64).to_le_bytes());
    // The mark sits at the end of the token as a run of bytes.
    token[48..52].copy_from_slice(&(note.mac.len() as u32).to_le_bytes());
    (token, vec![(TOKEN_MAC, note.mac.clone())])
}



/// The parameters an AES-GCM decryption is opened with.
fn gcm_params(app_id: &[u8], nonce: &[u8]) -> Params {
    let mut params = Params::default();
    params.enumeration(TAG_BLOCK_MODE, BLOCK_MODE_GCM);
    params.enumeration(TAG_PADDING, PADDING_NONE);
    params.integer(TAG_MAC_LENGTH, 128);
    if !app_id.is_empty() {
        params.blob(TAG_APPLICATION_ID, app_id.to_vec());
    }
    params.blob(TAG_NONCE, nonce.to_vec());
    params
}

/// The methods this client uses, as this layout numbers them.
const BEGIN: u32 = 16;
const UPDATE: u32 = 17;
const FINISH: u32 = 18;
const ABORT: u32 = 19;

/// How large one `KeyParameter` is on the wire: its tag, the union of
/// values, which numbers its own arm but has it chosen by the tag, and the
/// run of bytes that sits beside the union rather than in it.
const PARAMETER_BYTES: usize = 32;
/// Where a parameter's value and its run of bytes sit in it: the union's
/// own arm, its widest value, and then the run's address and how many
/// bytes it holds.
const PARAMETER_VALUE: usize = 8;
const PARAMETER_BLOB: usize = 16;
const PARAMETER_BLOB_LENGTH: usize = 24;

/// How large the note the secure world wrote for a credential is on the
/// wire, and the token that vouches for a decryption with it. Both are all
/// zero when there is none, which is the case for every key of the phone's
/// own, as the metadata key is.
const AUTH_TOKEN_BYTES: usize = 56;
const VERIFICATION_TOKEN_BYTES: usize = 56;

/// Where the runs of bytes inside those notes sit: the mark a note ends
/// with, and the parameters a verification vouched for, which comes before
/// its own mark.
const TOKEN_MAC: usize = 40;
const VERIFICATION_PARAMETERS: usize = 16;

/// A note that vouches for nothing: every value is nothing, and the runs
/// inside it are empty, as the service looks for them all the same.
fn empty_verification_token() -> (Vec<u8>, Vec<(usize, Vec<u8>)>) {
    (vec![0u8; VERIFICATION_TOKEN_BYTES], vec![(VERIFICATION_PARAMETERS, Vec::new()), (TOKEN_MAC, Vec::new())])
}

const PURPOSE_DECRYPT: u32 = 1;
const BLOCK_MODE_GCM: u32 = 32;
const PADDING_NONE: u32 = 1;

const TAG_PURPOSE: i32 = 536_870_913;
const TAG_BLOCK_MODE: i32 = 536_870_916;
const TAG_PADDING: i32 = 536_870_918;
const TAG_MAC_LENGTH: i32 = 805_307_371;
const TAG_APPLICATION_ID: i32 = -1_879_047_591;
const TAG_NONCE: i32 = -1_879_047_191;

/// The parameters of a decryption being built, as one run of structures.
#[derive(Default)]
struct Params {
    data: Vec<u8>,
    pointed: Vec<(usize, usize, Vec<u8>)>,
}

impl Params {
    /// A parameter whose value is a number, which a purpose, a block mode,
    /// a padding and a length are alike. Every parameter carries a run of
    /// bytes after its value whether it uses one or not, and the service
    /// reads that run for each in turn, so an empty one is still written.
    fn value(&mut self, tag: i32, value: u32) {
        let index = self.data.len() / PARAMETER_BYTES;
        let mut p = vec![0u8; PARAMETER_BYTES];
        p[0..4].copy_from_slice(&tag.to_le_bytes());
        // The union's own arm is left at the one it starts with: the
        // service reads the value the tag names, not the arm.
        p[PARAMETER_VALUE..PARAMETER_VALUE + 4].copy_from_slice(&value.to_le_bytes());
        self.data.extend_from_slice(&p);
        self.pointed.push((index, PARAMETER_BLOB, Vec::new()));
    }

    fn enumeration(&mut self, tag: i32, value: u32) {
        self.value(tag, value);
    }

    fn integer(&mut self, tag: i32, value: u32) {
        self.value(tag, value);
    }


    /// A run of bytes: the arm says where it is, and how long it is, in the
    /// place its structure holds the run's own address frame.
    fn blob(&mut self, tag: i32, bytes: Vec<u8>) {
        let index = self.data.len() / PARAMETER_BYTES;
        let mut p = vec![0u8; PARAMETER_BYTES];
        p[0..4].copy_from_slice(&tag.to_le_bytes());
        // The run's frame: its address, how long it is, and that the reader
        // does not free it.
        p[PARAMETER_BLOB_LENGTH..PARAMETER_BLOB_LENGTH + 4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.data.extend_from_slice(&p);
        self.pointed.push((index, PARAMETER_BLOB, bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_that_refused_the_work_is_an_error() {
        assert!(refused(0, "export the storage key").is_ok());
        assert!(refused(-30, "export the storage key").is_err(), "the layout's own name for a key that is gone");
    }

    #[test]
    fn the_parameters_of_a_decryption_are_written_as_the_layout_lays_them_out() {
        let params = gcm_params(b"app", &[1u8; 12]);
        assert_eq!(params.data.len(), 5 * PARAMETER_BYTES, "mode, padding, length, identifier and nonce, never the purpose, which a decryption is opened with");
        let first = &params.data[..PARAMETER_BYTES];
        assert_eq!(i32::from_le_bytes(first[0..4].try_into().unwrap()), TAG_BLOCK_MODE);
        assert_eq!(u32::from_le_bytes(first[PARAMETER_VALUE..PARAMETER_VALUE + 4].try_into().unwrap()), BLOCK_MODE_GCM);
        assert_eq!(params.pointed.len(), 5, "every parameter carries a run, empty or not");
        assert_eq!(params.pointed[0].1, PARAMETER_BLOB, "a run is named where the structure holds its address");
        assert!(params.pointed[0].2.is_empty(), "a number's run holds nothing");
        assert_eq!(params.pointed[3].2, b"app", "the identifier is kept for its address to point at");
    }
}
