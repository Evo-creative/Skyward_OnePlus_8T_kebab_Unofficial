//! The phone's key service as the older hardware layout presents it. The
//! newer layout, which the recovery talks to where a phone has one, hands
//! back a live operation; this one numbers the operation and keeps it on
//! the service, so every step of a decryption goes to the same device with
//! that number.
//!
//! The calls and answers serve the phone's own driver and the host tests.

use anyhow::{bail, Context, Result};
use skyward_binder::hidl::{self, Answer, Message};

#[cfg(target_os = "android")]
use skyward_binder::driver::{Binder, Handle};

/// The key service as the newer of the two hardware layouts names itself.
pub const INTERFACE_41: &str = "android.hardware.keymaster@4.1::IKeymasterDevice";
/// The key service as the older of the two names itself.
pub const INTERFACE_40: &str = "android.hardware.keymaster@4.0::IKeymasterDevice";

const INSTANCE: &str = "default";

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

    /// What the service says about the hardware it runs on.
    pub fn hardware(&self) -> Result<Hardware> {
        let call = Message::call(self.interface);
        let reply = self.device.transact(GET_HARDWARE_INFO, &call).context("ask the key service about its hardware")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("ask the key service about its hardware")?;
        let answer = read.pointed().context("read what the key service said about its hardware")?;
        read_hardware(answer)
    }

    /// The key itself, unwrapped from the form a storage key is kept in.
    /// Android's own keystore does this for vold by exporting the key in the
    /// raw form, which is what a storage key is for.
    pub fn ephemeral_storage_key(&self, key_blob: &[u8]) -> Result<Vec<u8>> {
        let mut call = Message::call(self.interface);
        // The raw form, which the phone's own key format numbers first.
        call.u32(0);
        call.bytes(key_blob);
        call.bytes(&[]);
        call.bytes(&[]);
        let reply = self.device.transact(EXPORT_KEY, &call).context("export the storage key")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("export the storage key")?;
        let answer = read.pointed().context("read the storage key")?;
        refused(answer, "export the storage key")?;
        Ok(run_in(answer, 8).context("read the storage key")?.to_vec())
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
        params.enumeration(TAG_PURPOSE, PURPOSE_DECRYPT);
        params
    }

    /// Open a decryption and give the number the service knows it by.
    fn begin(&self, key_blob: &[u8], app_id: &[u8], nonce: &[u8], note: Option<&crate::token::Token>) -> Result<u64> {
        let mut call = Message::call(self.interface);
        call.u32(PURPOSE_DECRYPT);
        call.bytes(key_blob);
        let params = Self::gcm_params(app_id, nonce);
        call.structures(params.data, PARAMETER_BYTES, params.pointed);
        let (token, mac) = auth_token(note);
        call.structure(token, mac);
        let reply = self.device.transact(BEGIN, &call).context("start a decryption")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("start a decryption")?;
        let answer = read.pointed().context("read what the key service answered")?;
        refused(answer, "start a decryption")?;
        if answer.len() < 32 {
            bail!("the key service answered {} bytes to a decryption", answer.len());
        }
        Ok(u64::from_le_bytes(answer[24..32].try_into().expect("eight bytes")))
    }

    fn update(&self, operation: u64, input: &[u8]) -> Result<Vec<u8>> {
        let mut call = Message::call(self.interface);
        call.u64(operation);
        call.structures(Vec::new(), PARAMETER_BYTES, Vec::new());
        call.bytes(input);
        call.structure(vec![0u8; AUTH_TOKEN_BYTES], Vec::new());
        call.structure(vec![0u8; VERIFICATION_TOKEN_BYTES], Vec::new());
        let reply = self.device.transact(UPDATE, &call).context("feed the decryption")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("feed the decryption")?;
        let answer = read.pointed().context("read what the decryption gave back")?;
        refused(answer, "feed the decryption")?;
        Ok(run_in(answer, 24).context("read what the decryption gave back")?.to_vec())
    }

    fn finish(&self, operation: u64) -> Result<Vec<u8>> {
        let mut call = Message::call(self.interface);
        call.u64(operation);
        call.structures(Vec::new(), PARAMETER_BYTES, Vec::new());
        call.bytes(&[]);
        call.bytes(&[]);
        call.structure(vec![0u8; AUTH_TOKEN_BYTES], Vec::new());
        call.structure(vec![0u8; VERIFICATION_TOKEN_BYTES], Vec::new());
        let reply = self.device.transact(FINISH, &call).context("close the decryption")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("close the decryption")?;
        let answer = read.pointed().context("read what the decryption ended with")?;
        refused(answer, "close the decryption, which is where a wrong key is refused")?;
        Ok(run_in(answer, 24).context("read what the decryption ended with")?.to_vec())
    }

    fn abort(&self, operation: u64) -> Result<()> {
        let mut call = Message::call(self.interface);
        call.u64(operation);
        let reply = self.device.transact(ABORT, &call).context("drop the decryption")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("drop the decryption")?;
        let answer = read.pointed().context("read what dropping the decryption said")?;
        refused(answer, "drop the decryption")
    }
}

/// What a returning structure says of the work: a number that is nothing
/// when it went through, and the layout's own name for what stopped it.
fn refused(answer: &[u8], what: &str) -> Result<()> {
    if answer.len() < 4 {
        bail!("the key service answered {} bytes to {what}", answer.len());
    }
    let said = i32::from_le_bytes(answer[0..4].try_into().expect("four bytes"));
    if said != 0 {
        bail!("the key service refused {what} with its code {said}");
    }
    Ok(())
}

/// The note the secure world wrote when the user answered their screen lock,
/// as this layout carries it: who it was for, what they did, when, and the
/// mark the secure world checks it by. A key that is not guarded by a lock
/// takes a token of nothing.
fn auth_token(note: Option<&crate::token::Token>) -> (Vec<u8>, Vec<(usize, Vec<u8>)>) {
    let mut token = vec![0u8; AUTH_TOKEN_BYTES];
    let Some(note) = note else { return (token, Vec::new()) };
    token[0..8].copy_from_slice(&(note.challenge as u64).to_le_bytes());
    token[8..16].copy_from_slice(&(note.user as u64).to_le_bytes());
    token[16..24].copy_from_slice(&(note.authenticator as u64).to_le_bytes());
    token[24..28].copy_from_slice(&(note.kind as u32).to_le_bytes());
    token[32..40].copy_from_slice(&(note.when as u64).to_le_bytes());
    // The mark sits at the end of the token as a run of bytes.
    token[48..52].copy_from_slice(&(note.mac.len() as u32).to_le_bytes());
    (token, vec![(40, note.mac.clone())])
}

/// The returning structure as the phone's own libraries hold it: the
/// security level, then the name and the author as runs of text.
fn read_hardware(answer: &[u8]) -> Result<Hardware> {
    if answer.len() < 40 {
        bail!("the key service answered {} bytes about its hardware", answer.len());
    }
    let security_level = u32::from_le_bytes(answer[0..4].try_into().expect("four bytes"));
    let name = text_in(answer, 8).context("read the key service's name")?;
    let author = text_in(answer, 24).context("read the key service's author")?;
    Ok(Hardware { security_level, name, author })
}

/// The text a returning structure holds at `offset`: its address, how long
/// it is, and the terminator the length leaves out.
fn text_in(structure: &[u8], offset: usize) -> Result<String> {
    let run = run_in(structure, offset)?;
    Ok(String::from_utf8_lossy(run.split(|b| *b == 0).next().unwrap_or(run)).into_owned())
}


/// The methods this client uses, as this layout numbers them.
const BEGIN: u32 = 16;
const UPDATE: u32 = 17;
const FINISH: u32 = 18;
const ABORT: u32 = 19;

/// How large one `KeyParameter` is on the wire: the tag, the union's own
/// tag, and the widest arm of the union.
const PARAMETER_BYTES: usize = 32;
/// Which arm of a parameter's value is used.
const ARM_ENUM: u32 = 0;
const ARM_INTEGER: u32 = 1;
const ARM_BLOB: u32 = 4;

/// How large the note the secure world wrote for a credential is on the
/// wire, and the token that vouches for a decryption with it. Both are all
/// zero when there is none, which is the case for every key of the phone's
/// own, as the metadata key is.
const AUTH_TOKEN_BYTES: usize = 56;
const VERIFICATION_TOKEN_BYTES: usize = 40;

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
    fn push(&mut self, tag: i32, arm: u32, value: u32) {
        let mut p = vec![0u8; PARAMETER_BYTES];
        p[0..4].copy_from_slice(&tag.to_le_bytes());
        p[8..12].copy_from_slice(&arm.to_le_bytes());
        p[16..20].copy_from_slice(&value.to_le_bytes());
        self.data.extend_from_slice(&p);
    }

    fn enumeration(&mut self, tag: i32, value: u32) {
        self.push(tag, ARM_ENUM, value);
    }

    fn integer(&mut self, tag: i32, value: u32) {
        self.push(tag, ARM_INTEGER, value);
    }

    /// A run of bytes: the arm says where it is, and how long it is, in the
    /// place its structure holds the run's own address frame.
    fn blob(&mut self, tag: i32, bytes: Vec<u8>) {
        let index = self.data.len() / PARAMETER_BYTES;
        let mut p = vec![0u8; PARAMETER_BYTES];
        p[0..4].copy_from_slice(&tag.to_le_bytes());
        p[8..12].copy_from_slice(&ARM_BLOB.to_le_bytes());
        // The run's frame: its address, how long it is, and that the reader
        // does not free it.
        p[24..28].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.data.extend_from_slice(&p);
        self.pointed.push((index, 16, bytes));
    }
}

/// A run of bytes a returning structure points at, which the driver copied
/// into this program's own room.
fn run(at: u64, length: u32) -> &'static [u8] {
    if at == 0 || length == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(at as *const u8, length as usize) }
}

/// The run a returning structure holds at `offset`: its address, then how
/// long it is.
fn run_in(structure: &[u8], offset: usize) -> Result<&'static [u8]> {
    if structure.len() < offset + 16 {
        bail!("the key service answered {} bytes, too few to hold a run at {offset}", structure.len());
    }
    let at = u64::from_le_bytes(structure[offset..offset + 8].try_into().expect("eight bytes"));
    let length = u32::from_le_bytes(structure[offset + 8..offset + 12].try_into().expect("four bytes"));
    Ok(run(at, length))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hardware_answer_is_read_as_the_phone_lays_it_out() {
        // The security level, then two runs of text, each an address and a
        // length among the values.
        let name = b"qti-keymaster\0";
        let author = b"Qualcomm\0";
        let mut answer = vec![0u8; 40];
        answer[0..4].copy_from_slice(&1u32.to_le_bytes());
        answer[8..16].copy_from_slice(&(name.as_ptr() as u64).to_le_bytes());
        answer[16..20].copy_from_slice(&(name.len() as u32).to_le_bytes());
        answer[24..32].copy_from_slice(&(author.as_ptr() as u64).to_le_bytes());
        answer[32..36].copy_from_slice(&(author.len() as u32).to_le_bytes());
        let h = read_hardware(&answer).unwrap();
        assert_eq!(h, Hardware { security_level: 1, name: "qti-keymaster".into(), author: "Qualcomm".into() });
    }

    #[test]
    fn an_answer_too_short_to_hold_the_hardware_is_refused() {
        assert!(read_hardware(&[0u8; 8]).is_err());
    }
}