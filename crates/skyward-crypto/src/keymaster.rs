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

/// The method that says what the key hardware can do, as this layout
/// numbers its methods.
const GET_HARDWARE_FEATURES: u32 = 1;

/// What the phone says about its key hardware, as this layout says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hardware {
    /// Whether the keys are used inside the secure world rather than by
    /// software beside it.
    pub secure: bool,
    /// Whether it can make and use elliptic-curve keys.
    pub ec: bool,
    /// Whether it encrypts and decrypts with symmetric keys itself.
    pub symmetric: bool,
    /// Whether it attests keys.
    pub attestation: bool,
    /// Whether one key can serve every user of the phone.
    pub all_users: bool,
    pub client_id: u32,
    pub storage_key: u32,
    pub attestation_key: u32,
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
        let reply = self.device.transact(GET_HARDWARE_FEATURES, &call).context("ask the key service about its hardware")?;
        let mut read = Answer::new(reply.bytes());
        read.status().context("ask the key service about its hardware")?;
        let answer = read.pointed().context("read what the key service said about its hardware")?;
        read_hardware(answer)
    }
}

/// The returning structure as the phone's own libraries hold it: five truth
/// values, then padding to a whole number, then three numbers.
fn read_hardware(answer: &[u8]) -> Result<Hardware> {
    if answer.len() < 20 {
        bail!("the key service answered {} bytes about its hardware", answer.len());
    }
    Ok(Hardware {
        secure: answer[0] != 0,
        ec: answer[1] != 0,
        symmetric: answer[2] != 0,
        attestation: answer[3] != 0,
        all_users: answer[4] != 0,
        client_id: u32::from_le_bytes(answer[8..12].try_into().expect("four bytes")),
        storage_key: u32::from_le_bytes(answer[12..16].try_into().expect("four bytes")),
        attestation_key: u32::from_le_bytes(answer[16..20].try_into().expect("four bytes")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hardware_answer_is_read_as_the_phone_lays_it_out() {
        // Five truth values, then padding to a whole number, then three
        // numbers, as the returning structure holds them.
        let mut answer = vec![0u8; 20];
        answer[0] = 1;
        answer[2] = 1;
        answer[8..12].copy_from_slice(&7u32.to_le_bytes());
        answer[12..16].copy_from_slice(&9u32.to_le_bytes());
        answer[16..20].copy_from_slice(&11u32.to_le_bytes());
        let h = read_hardware(&answer).unwrap();
        assert_eq!(h, Hardware { secure: true, ec: false, symmetric: true, attestation: false, all_users: false, client_id: 7, storage_key: 9, attestation_key: 11 });
    }

    #[test]
    fn an_answer_too_short_to_hold_the_hardware_is_refused() {
        assert!(read_hardware(&[0u8; 8]).is_err());
    }
}